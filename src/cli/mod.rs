//! Command-line surface. Each subcommand module owns its argument structs and
//! its handler; this module wires global flags into [`Context`].

use std::path::PathBuf;
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};

use crate::cache::Cache;
use crate::config::{self, CliOverrides, FileConfig, Settings};
use crate::error::{IdxError, Result};
use crate::http::{Fetcher, ReqwestBackend};
use crate::output::{self, ColorMode, OutputFormat};
use crate::providers::{self, Provider, ProviderKind, ProviderOptions, Ttls};

pub mod cache_cmd;
pub mod config_cmd;
pub mod ownership;
pub mod stocks;

const LONG_ABOUT: &str = "\
Indonesian stock market (IDX / IHSG) analysis for humans and AI agents.

Tables go to stdout for people; add `-o json` for a stable, schema-documented
envelope that scripts and agents can parse. Errors go to stderr with a
meaningful exit code. Data comes from free public endpoints, cached locally.

Examples:
  idx stocks quote BBCA,BBRI,TLKM
  idx stocks history TLKM --period 1y --interval 1wk
  idx stocks fundamental BBCA              # ratios plus a plain-language verdict
  idx stocks technical BBRI                # RSI, MACD, moving averages, signal
  idx stocks compare BBCA BBRI BMRI        # side-by-side fundamentals
  idx ownership import --latest            # KSEI >1% holders into local SQLite
  idx ownership ticker BBCA                # who holds more than 1%
  idx mcp                                  # MCP server over stdio for agents
  idx -o json stocks quote BBCA | jq '.data[0].price'
  idx --offline stocks quote BBCA        # serve from cache, never touch the network
  idx schema quotes                      # JSON Schema of the quote envelope";

const AFTER_HELP: &str = "\
Exit codes: 0 ok · 1 internal · 2 usage · 3 network/provider · 4 not found · 5 offline cache miss · 6 config
Env: IDX_PROVIDER, IDX_OUTPUT, IDX_COLOR / NO_COLOR, IDX_TIMEOUT_SECS, IDX_CACHE_DIR, IDX_OFFLINE, IDX_NO_CACHE, IDX_CONFIG, IDX_MSN_API_KEY, IDX_OWNERSHIP_DB
Docs: https://github.com/fzn0x/idx";

#[derive(Debug, Parser)]
#[command(
    name = "idx",
    version,
    about = "Indonesian stock market (IDX / IHSG) analysis CLI for humans and AI agents",
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP,
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, clap::Args)]
pub struct GlobalOpts {
    /// Output format (table for humans, json for scripts and agents).
    #[arg(short = 'o', long, global = true, value_enum, value_name = "FORMAT")]
    pub output: Option<OutputFormat>,

    /// Data provider. `auto` tries each one in turn.
    #[arg(long, global = true, value_enum, value_name = "NAME")]
    pub provider: Option<ProviderKind>,

    /// Never touch the network; serve from the local cache even if stale.
    #[arg(long, global = true, conflicts_with = "no_cache")]
    pub offline: bool,

    /// Ignore cached responses (still writes fresh ones).
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Disable ANSI colours (also honours NO_COLOR).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Per-request network timeout in seconds.
    #[arg(long, global = true, value_name = "SECS")]
    pub timeout: Option<u64>,

    /// Cache directory override.
    #[arg(long, global = true, value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Config file path.
    #[arg(long, global = true, value_name = "PATH", env = "IDX_CONFIG")]
    pub config: Option<PathBuf>,

    /// Verbose diagnostics on stderr (-v info, -vv debug, -vvv trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Quotes, history, profile, financials, earnings, fundamentals, technicals, compare.
    #[command(subcommand)]
    Stocks(stocks::StocksCmd),

    /// KSEI shareholder register (holders above 1 %): import monthly files, query holders, entities, changes.
    #[command(subcommand)]
    Ownership(ownership::OwnershipCmd),

    /// Read or write the config file.
    #[command(subcommand)]
    Config(config_cmd::ConfigCmd),

    /// Inspect or clear the local response cache.
    #[command(subcommand)]
    Cache(cache_cmd::CacheCmd),

    /// Start the MCP server on stdin/stdout (for Claude Code, Cursor and other MCP clients).
    #[command(
        after_help = "Register with Claude Code:\n  claude mcp add idx -- idx mcp\n\nEvery stocks command becomes a tool (idx_quote, idx_history, idx_profile, idx_financials,\nidx_earnings, idx_fundamental, idx_technical, idx_compare) returning the same JSON as -o json.\nLogs go to stderr; stdout carries only the protocol."
    )]
    Mcp,

    /// Print the JSON Schema for a JSON output shape (run without a name to list them).
    Schema {
        /// quotes | history | profile | fundamentals | compare | financials | earnings | technical | ownership-* | error | quote | candle
        name: Option<String>,
    },

    /// Generate shell completions (bash, zsh, fish, powershell, elvish).
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Print version and build information.
    Version,
}

/// Everything a handler needs. Built once per invocation.
pub struct Context {
    pub settings: Settings,
    pub provider: Arc<dyn Provider>,
    pub color: bool,
}

impl Context {
    pub fn json(&self) -> bool {
        self.settings.output == OutputFormat::Json
    }
}

pub fn resolve_settings(global: &GlobalOpts) -> Result<Settings> {
    let config_path = global
        .config
        .clone()
        .unwrap_or_else(config::default_config_path);
    let file = FileConfig::load(&config_path)?;
    let overrides = CliOverrides {
        provider: global.provider,
        output: global.output,
        no_color: global.no_color,
        offline: global.offline,
        no_cache: global.no_cache,
        timeout_secs: global.timeout,
        cache_dir: global.cache_dir.clone(),
    };
    config::resolve(&overrides, &file, config_path, &|k| std::env::var(k).ok())
}

fn build_context(global: &GlobalOpts) -> Result<Context> {
    let settings = resolve_settings(global)?;
    let backend = ReqwestBackend::new(settings.timeout())?;
    let fetcher = Arc::new(Fetcher::new(
        Arc::new(backend),
        Cache::new(&settings.cache_dir),
        settings.cache_mode,
    ));
    let opts = ProviderOptions {
        ttls: Ttls {
            quote: settings.quote_ttl(),
            history: settings.history_ttl(),
            fundamentals: settings.fundamentals_ttl(),
            financials: settings.financials_ttl(),
            ..Ttls::default()
        },
        msn_api_key: settings.msn_api_key.clone(),
    };
    let provider: Arc<dyn Provider> =
        Arc::from(providers::build(settings.provider, fetcher, opts)?);
    let color = settings
        .color
        .enabled(std::io::IsTerminal::is_terminal(&std::io::stdout()));
    Ok(Context {
        settings,
        provider,
        color,
    })
}

/// Entry point used by `main`. Returns the error for `main` to print and map
/// to an exit code.
pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Stocks(cmd) => {
            let ctx = build_context(&cli.global)?;
            stocks::run(&ctx, cmd).await
        }
        Command::Ownership(cmd) => {
            let ctx = build_context(&cli.global)?;
            ownership::run(&ctx, cmd).await
        }
        Command::Mcp => {
            let ctx = build_context(&cli.global)?;
            crate::mcp::serve(ctx.provider, ctx.settings.ownership_db.clone()).await
        }
        Command::Config(cmd) => config_cmd::run(&cli.global, cmd),
        Command::Cache(cmd) => cache_cmd::run(&cli.global, cmd),
        Command::Schema { name } => {
            let names = output::json::schema_names();
            match name {
                None => {
                    let json = cli.global.output == Some(OutputFormat::Json);
                    if json {
                        output::print_stdout(&output::json::to_pretty(&names)?)
                    } else {
                        output::print_stdout(&format!(
                            "Available schemas: {}\nUsage: idx schema <name>\n",
                            names.join(", ")
                        ))
                    }
                }
                Some(n) => {
                    let schema = output::json::schema_for(&n)?;
                    output::print_stdout(&output::json::to_pretty(&schema)?)
                }
            }
        }
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            let mut buf = Vec::new();
            clap_complete::generate(shell, &mut cmd, "idx", &mut buf);
            output::print_stdout(&String::from_utf8_lossy(&buf))
        }
        Command::Version => {
            let json = cli.global.output == Some(OutputFormat::Json);
            let info = VersionInfo::current();
            if json {
                output::print_stdout(&output::json::to_pretty(&info)?)
            } else {
                output::print_stdout(&format!(
                    "idx {} ({} {})\nproviders: {}\nconfig: {}\ncache:  {}\n",
                    info.version,
                    info.target,
                    info.profile,
                    info.providers.join(", "),
                    config::default_config_path().display(),
                    config::default_cache_dir().display(),
                ))
            }
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct VersionInfo {
    pub version: &'static str,
    pub target: &'static str,
    pub profile: &'static str,
    pub providers: Vec<&'static str>,
}

impl VersionInfo {
    pub fn current() -> Self {
        VersionInfo {
            version: env!("CARGO_PKG_VERSION"),
            target: std::env::consts::ARCH,
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            providers: providers::AUTO_ORDER.to_vec(),
        }
    }
}

/// Prints an error the way the chosen output format expects.
pub fn report_error(global: Option<&GlobalOpts>, err: &IdxError) {
    let json = global
        .map(|g| g.output == Some(OutputFormat::Json))
        .unwrap_or(false)
        || std::env::var("IDX_OUTPUT").is_ok_and(|v| v.eq_ignore_ascii_case("json"));
    if json {
        let env = output::json::ErrorEnvelope::from(err);
        output::print_stderr(&serde_json::to_string(&env).unwrap_or_default());
    } else {
        output::print_stderr(&format!("error: {err}"));
        if let IdxError::OfflineMiss(_) = err {
            output::print_stderr("hint: run once without --offline to populate the cache");
        }
    }
}

pub fn color_mode_for_tests() -> ColorMode {
    ColorMode::Never
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_global_flags_anywhere() {
        let cli = Cli::parse_from(["idx", "stocks", "quote", "BBCA", "-o", "json", "--offline"]);
        assert_eq!(cli.global.output, Some(OutputFormat::Json));
        assert!(cli.global.offline);
        let cli = Cli::parse_from([
            "idx", "-o", "table", "stocks", "history", "TLKM", "--period", "5y",
        ]);
        assert_eq!(cli.global.output, Some(OutputFormat::Table));
        assert!(matches!(cli.command, Command::Stocks(_)));
    }

    #[test]
    fn offline_and_no_cache_conflict() {
        assert!(Cli::try_parse_from(["idx", "--offline", "--no-cache", "version"]).is_err());
    }
}
