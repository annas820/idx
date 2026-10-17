//! `idx mcp`: the same capabilities as the CLI, exposed as MCP tools over
//! stdio. Every tool returns the exact JSON envelope `idx -o json` prints
//! (see `docs/json-output.md`), so an agent that learned one learned both.
//! Failures come back as tool errors carrying the same `{error, exit_code}`
//! object the CLI writes to stderr.
//!
//! Nothing here may write to stdout except the protocol; logs go to stderr.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerConfig,
};
use rmcp::{schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use serde::Deserialize;

use crate::error::{IdxError, Result};
use crate::model::{Interval, Period, StatementPeriod, Symbol};
use crate::output::json::{
    self, Envelope, ErrorEnvelope, SCHEMA_COMPARE, SCHEMA_EARNINGS, SCHEMA_FINANCIALS,
    SCHEMA_FUNDAMENTALS, SCHEMA_HISTORY, SCHEMA_OWNERSHIP_CHANGES, SCHEMA_OWNERSHIP_ENTITY,
    SCHEMA_OWNERSHIP_HOLDERS, SCHEMA_OWNERSHIP_RELEASES, SCHEMA_PROFILE, SCHEMA_QUOTES,
    SCHEMA_TECHNICAL,
};
use crate::ownership::OwnershipDb;
use crate::providers::{Provider, Sourced};
use crate::service;

const INSTRUCTIONS: &str = "\
idx: Indonesian stock market (IDX / IHSG) data and analysis from free public sources.
Tickers are IDX codes such as BBCA, BBRI, TLKM (no .JK suffix needed).
Every tool returns the same JSON envelope as the `idx -o json` CLI:
{schema, provider, fetched_at, cached, stale, warnings?, data}. Fields the source did
not return are null; nothing is estimated. Prices are delayed as published.
Errors return isError=true with {error:{code,message}, exit_code}; code not_found
means the ticker is unknown, offline_miss means --offline with nothing cached.
Ownership tools read the local SQLite register of holders above 1% (KSEI via IDX,
monthly); if it is empty they say so and the user must run `idx ownership import --latest`.
Verdicts and signals are transparent rule scores, not investment advice.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolsArg {
    /// One or more IDX tickers, comma or space separated, e.g. "BBCA,BBRI,TLKM".
    pub symbols: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolArg {
    /// One IDX ticker, e.g. "BBCA".
    pub symbol: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryArgs {
    /// One IDX ticker, e.g. "TLKM".
    pub symbol: String,
    /// Look-back window: 1d, 5d, 1mo, 3mo, 6mo, 1y (default), 2y, 5y, 10y, ytd, max.
    #[serde(default)]
    pub period: Option<String>,
    /// Candle size: 5m, 15m, 30m, 1h, 1d (default), 1wk, 1mo. Intraday sizes need short periods.
    #[serde(default)]
    pub interval: Option<String>,
    /// Keep only the most recent N candles (default 60). 0 returns all of them.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FinancialsArgs {
    /// One IDX ticker, e.g. "BBCA".
    pub symbol: String,
    /// annual (default, ~4 fiscal years), quarterly (~5 quarters) or ttm (trailing twelve months).
    #[serde(default)]
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TechnicalArgs {
    /// One IDX ticker, e.g. "BBRI".
    pub symbol: String,
    /// Daily candles to analyse: 6mo, 1y (default), 2y, 5y. Shorter windows leave the 200-day averages null.
    #[serde(default)]
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipTickerArgs {
    /// One IDX ticker, e.g. "BBCA".
    pub symbol: String,
    /// Release date YYYY-MM-DD (default: newest imported). idx_ownership_releases lists them.
    #[serde(default)]
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipEntityArgs {
    /// Part of a holder's name (person or institution), at least 3 characters, e.g. "Dwimuria".
    pub name: String,
    /// Release date YYYY-MM-DD (default: newest imported).
    #[serde(default)]
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipChangesArgs {
    /// Earlier release date YYYY-MM-DD.
    pub from: String,
    /// Later release date YYYY-MM-DD.
    pub to: String,
    /// Restrict to one ticker.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Ignore moves smaller than this many percentage points (default 0).
    #[serde(default)]
    pub min_pct_change: Option<f64>,
    /// Rows kept, largest moves first (default 50; 0 = all).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Clone)]
pub struct IdxServer {
    provider: Arc<dyn Provider>,
    ownership_db: PathBuf,
    tool_router: ToolRouter<IdxServer>,
}

fn parse_date(v: &Option<String>) -> Result<Option<chrono::NaiveDate>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| IdxError::Usage(format!("invalid date {s:?}; use YYYY-MM-DD"))),
    }
}

fn ok<T: serde::Serialize>(schema: &str, s: Sourced<T>) -> Result<CallToolResult> {
    let env = Envelope::from_sourced(schema, s);
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json::to_pretty(&env)?,
    )]))
}

/// Tool-level failure: the agent sees the same error object the CLI prints.
fn fail(e: IdxError) -> CallToolResult {
    let env = ErrorEnvelope::from(&e);
    let text = serde_json::to_string_pretty(&env).unwrap_or_else(|_| e.to_string());
    CallToolResult::error(vec![ContentBlock::text(text)])
}

fn finish(r: Result<CallToolResult>) -> std::result::Result<CallToolResult, McpError> {
    Ok(r.unwrap_or_else(fail))
}

fn parse_opt<T: std::str::FromStr<Err = IdxError>>(v: &Option<String>, default: T) -> Result<T> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.parse(),
        None => Ok(default),
    }
}

#[tool_router]
impl IdxServer {
    pub fn new(provider: Arc<dyn Provider>, ownership_db: PathBuf) -> Self {
        IdxServer {
            provider,
            ownership_db,
            tool_router: Self::tool_router(),
        }
    }

    fn db(&self) -> Result<OwnershipDb> {
        OwnershipDb::open(&self.ownership_db)
    }

    /// Names of the tools, for tests and docs.
    pub fn tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    #[tool(
        name = "idx_quote",
        description = "Delayed quotes for one or more IDX tickers: price, change, previous close, OHLC, volume, market cap, P/E, dividend yield, 52-week range. Returns the idx.quotes.v1 envelope; unknown tickers are listed in warnings."
    )]
    async fn quote(
        &self,
        Parameters(a): Parameters<SymbolsArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbols = Symbol::parse_list([a.symbols.as_str()])?;
                ok(SCHEMA_QUOTES, self.provider.quotes(&symbols).await?)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_history",
        description = "OHLCV candles for one IDX ticker, oldest first, with a summary (first/last close, change %, high, low, average volume). Returns the idx.history.v1 envelope. Use limit to bound the payload; the summary always covers the whole period."
    )]
    async fn history(
        &self,
        Parameters(a): Parameters<HistoryArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let period = parse_opt(&a.period, Period::Y1)?;
                let interval = parse_opt(&a.interval, Interval::Day1)?;
                let mut h = self.provider.history(&symbol, period, interval).await?;
                let limit = a.limit.unwrap_or(60) as usize;
                if limit > 0 && h.data.candles.len() > limit {
                    let skip = h.data.candles.len() - limit;
                    h.data.candles.drain(..skip);
                    h.warnings.push(format!(
                        "candles truncated to the last {limit}; summary covers the full period"
                    ));
                }
                ok(SCHEMA_HISTORY, h)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_profile",
        description = "Company profile for one IDX ticker: name, sector, industry, business description, employees, founding year, headquarters, website, market cap, shares outstanding, board and management. Returns the idx.profile.v1 envelope."
    )]
    async fn profile(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(SCHEMA_PROFILE, self.provider.profile(&symbol).await?)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_financials",
        description = "Income statement, balance sheet and cash flow for one IDX ticker, oldest period first, amounts in reporting currency (IDR). Returns the idx.financials.v1 envelope. Banks have no EBITDA/gross profit series (null)."
    )]
    async fn financials(
        &self,
        Parameters(a): Parameters<FinancialsArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let period = match a.period.as_deref().map(str::trim) {
                    None | Some("") | Some("annual") => StatementPeriod::Annual,
                    Some("quarterly") => StatementPeriod::Quarterly,
                    Some("ttm") => StatementPeriod::Ttm,
                    Some(other) => {
                        return Err(IdxError::Usage(format!(
                            "invalid period {other:?}; use annual, quarterly or ttm"
                        )))
                    }
                };
                ok(
                    SCHEMA_FINANCIALS,
                    self.provider.financials(&symbol, period).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_earnings",
        description = "Reported quarterly EPS versus analyst estimates with surprise %, fiscal-year and quarterly revenue/earnings, the next earnings date with consensus, and dividend dates for one IDX ticker. Returns the idx.earnings.v1 envelope."
    )]
    async fn earnings(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(SCHEMA_EARNINGS, self.provider.earnings(&symbol).await?)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_fundamental",
        description = "Fundamentals report for one IDX ticker: valuation (P/E, forward P/E, PEG, P/B, P/S, EV/EBITDA), profitability (ROE, ROA, margins), balance sheet (debt/equity, current ratio, cash, debt), growth, dividend, per-share figures, analyst targets, and a plain-language verdict (undervalued | fair | overvalued | insufficient_data) with every rule that fired listed in reasons. Returns the idx.fundamentals.v1 envelope."
    )]
    async fn fundamental(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(
                    SCHEMA_FUNDAMENTALS,
                    service::fundamentals(self.provider.as_ref(), &symbol).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_technical",
        description = "Technical analysis of one IDX ticker from daily candles: RSI(14), MACD(12,26,9), SMA/EMA 20/50/200, Bollinger(20,2), ATR(14), volume vs 20-day average, support/resistance from swing points, 52-week range, trend label and a buy | sell | neutral signal with reasons. Returns the idx.technical.v1 envelope."
    )]
    async fn technical(
        &self,
        Parameters(a): Parameters<TechnicalArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let period = parse_opt(&a.period, Period::Y1)?;
                ok(
                    SCHEMA_TECHNICAL,
                    service::technical(self.provider.as_ref(), &symbol, period).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_compare",
        description = "Side-by-side fundamentals (same fields as idx_fundamental, verdict included) for two or more IDX tickers. Returns the idx.compare.v1 envelope: data is an array in the order requested; tickers that failed are listed in warnings."
    )]
    async fn compare(
        &self,
        Parameters(a): Parameters<SymbolsArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbols = Symbol::parse_list([a.symbols.as_str()])?;
                ok(
                    SCHEMA_COMPARE,
                    service::compare(self.provider.as_ref(), &symbols).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_ticker",
        description = "Shareholders above 1% of one IDX ticker from KSEI's monthly register (local SQLite): holder name, investor type, local/foreign, shares, percent, sorted by percent. Returns the idx.ownership.holders.v1 envelope. Needs a prior `idx ownership import`."
    )]
    async fn ownership_ticker(
        &self,
        Parameters(a): Parameters<OwnershipTickerArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let as_of = parse_date(&a.as_of)?;
                let report = self.db()?.holders(symbol.as_str(), as_of)?;
                ok(SCHEMA_OWNERSHIP_HOLDERS, Sourced::fresh(report, "local-db"))
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_entity",
        description = "Every IDX position above 1% held by a person or institution whose name contains the query, from KSEI's monthly register (local SQLite). Spelling variants (PT/Tbk placement, punctuation) are grouped. Returns the idx.ownership.entity.v1 envelope."
    )]
    async fn ownership_entity(
        &self,
        Parameters(a): Parameters<OwnershipEntityArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                if a.name.trim().len() < 3 {
                    return Err(IdxError::Usage("name needs at least 3 characters".into()));
                }
                let as_of = parse_date(&a.as_of)?;
                let report = self.db()?.entity(&a.name, as_of)?;
                ok(SCHEMA_OWNERSHIP_ENTITY, Sourced::fresh(report, "local-db"))
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_changes",
        description = "Month-over-month changes in holders above 1% between two imported release dates: new holders, exits and position changes with percentage-point and share deltas, largest moves first. Optional ticker filter. Returns the idx.ownership.changes.v1 envelope."
    )]
    async fn ownership_changes(
        &self,
        Parameters(a): Parameters<OwnershipChangesArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let from = parse_date(&Some(a.from.clone()))?.expect("checked");
                let to = parse_date(&Some(a.to.clone()))?.expect("checked");
                let symbol = a.symbol.as_deref().map(Symbol::parse).transpose()?;
                let limit = match a.limit {
                    Some(0) => None,
                    Some(n) => Some(n as usize),
                    None => Some(50),
                };
                let report = self.db()?.changes(
                    from,
                    to,
                    symbol.as_ref().map(Symbol::as_str),
                    a.min_pct_change.unwrap_or(0.0),
                    limit,
                )?;
                ok(SCHEMA_OWNERSHIP_CHANGES, Sourced::fresh(report, "local-db"))
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_releases",
        description = "Lists the monthly KSEI register releases imported into the local database (as-of date, row counts, source). Use it to pick dates for idx_ownership_changes. Returns the idx.ownership.releases.v1 envelope."
    )]
    async fn ownership_releases(&self) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let list = self.db()?.releases()?;
                let mut s = Sourced::fresh(list, "local-db");
                if s.data.is_empty() {
                    s.warnings.push(
                        "no ownership data imported yet; run `idx ownership import --latest`"
                            .into(),
                    );
                }
                ok(SCHEMA_OWNERSHIP_RELEASES, s)
            }
            .await,
        )
    }
}

#[tool_handler]
impl ServerHandler for IdxServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("idx", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::LATEST)
            .with_instructions(INSTRUCTIONS.to_string())
    }
}

/// Runs the server on stdin/stdout until the client disconnects.
pub async fn serve(provider: Arc<dyn Provider>, ownership_db: PathBuf) -> Result<()> {
    use rmcp::{transport::stdio, ServiceExt};
    let server = IdxServer::new(provider, ownership_db);
    tracing::info!(tools = ?server.tool_names(), "idx mcp server starting on stdio");
    let running = server
        .serve(stdio())
        .await
        .map_err(|e| IdxError::Other(format!("mcp: {e}")))?;
    running
        .waiting()
        .await
        .map_err(|e| IdxError::Other(format!("mcp: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fundamentals, History, Quote};
    use async_trait::async_trait;

    struct Fake;

    #[async_trait]
    impl Provider for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn quotes(&self, s: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
            if s[0].as_str() == "ZZZZ" {
                return Err(IdxError::NotFound("ZZZZ".into()));
            }
            let mut q = Quote::empty(s[0].clone(), "fake");
            q.price = Some(5175.0);
            Ok(Sourced::fresh(vec![q], "fake"))
        }
        async fn history(&self, _: &Symbol, _: Period, _: Interval) -> Result<Sourced<History>> {
            Err(IdxError::OfflineMiss("nothing cached".into()))
        }
        async fn fundamentals(&self, s: &Symbol) -> Result<Sourced<Fundamentals>> {
            Ok(Sourced::fresh(
                Fundamentals::empty(s.clone(), "fake"),
                "fake",
            ))
        }
    }

    fn server() -> IdxServer {
        let dir = std::env::temp_dir().join(format!("idx-mcp-test-{}", std::process::id()));
        IdxServer::new(Arc::new(Fake), dir.join("ownership.sqlite"))
    }

    fn text(r: &CallToolResult) -> String {
        r.content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect()
    }

    #[test]
    fn every_tool_is_listed_with_a_schema() {
        let s = server();
        let mut names = s.tool_names();
        names.sort();
        assert_eq!(
            names,
            [
                "idx_compare",
                "idx_earnings",
                "idx_financials",
                "idx_fundamental",
                "idx_history",
                "idx_ownership_changes",
                "idx_ownership_entity",
                "idx_ownership_releases",
                "idx_ownership_ticker",
                "idx_profile",
                "idx_quote",
                "idx_technical",
            ]
        );
        for t in s.tool_router.list_all() {
            let d = t.description.as_deref().unwrap_or("");
            assert!(d.len() > 60, "{} needs a real description", t.name);
            assert!(
                t.input_schema.get("properties").is_some(),
                "{} has no typed input",
                t.name
            );
        }
    }

    #[tokio::test]
    async fn quote_tool_returns_the_cli_envelope() {
        let s = server();
        let r = s
            .quote(Parameters(SymbolsArg {
                symbols: "bbca".into(),
            }))
            .await
            .unwrap();
        assert_ne!(r.is_error, Some(true));
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["schema"], "idx.quotes.v1");
        assert_eq!(v["data"][0]["symbol"], "BBCA");
        assert_eq!(v["data"][0]["price"], 5175.0);
    }

    #[tokio::test]
    async fn failures_are_tool_errors_with_the_cli_error_shape() {
        let s = server();
        let r = s
            .quote(Parameters(SymbolsArg {
                symbols: "ZZZZ".into(),
            }))
            .await
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["error"]["code"], "not_found");
        assert_eq!(v["exit_code"], 4);

        let r = s
            .history(Parameters(HistoryArgs {
                symbol: "BBCA".into(),
                period: Some("7d".into()),
                interval: None,
                limit: None,
            }))
            .await
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("usage"));

        let r = s
            .history(Parameters(HistoryArgs {
                symbol: "BBCA".into(),
                period: None,
                interval: None,
                limit: None,
            }))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["error"]["code"], "offline_miss");
    }

    #[tokio::test]
    async fn fundamental_tool_adds_a_verdict() {
        let s = server();
        let r = s
            .fundamental(Parameters(SymbolArg {
                symbol: "BBCA".into(),
            }))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["schema"], "idx.fundamentals.v1");
        assert_eq!(v["data"]["verdict"]["label"], "insufficient_data");
    }
}
