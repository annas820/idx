//! `idx stocks ...`

use clap::Subcommand;

use crate::analysis::screen::{parse_filters, Key};
use crate::error::IdxError;
use crate::error::Result;
use crate::model::{Interval, Period, StatementPeriod, Symbol};
use crate::output::json::{
    Envelope, SCHEMA_COMPARE, SCHEMA_DIVIDENDS, SCHEMA_EARNINGS, SCHEMA_FINANCIALS,
    SCHEMA_FUNDAMENTALS, SCHEMA_HISTORY, SCHEMA_NEWS, SCHEMA_PROFILE, SCHEMA_QUOTES,
    SCHEMA_RETURNS, SCHEMA_SCREEN, SCHEMA_SENTIMENT, SCHEMA_TECHNICAL,
};
use crate::output::table_news as newstbl;
use crate::output::table_reports::{self as reports, StatementFlags};
use crate::output::{self, table};
use crate::providers::Sourced;
use crate::service::{self, ScreenOptions};
use crate::universe;

use super::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Statement {
    All,
    Income,
    Balance,
    Cash,
}

#[derive(Debug, Subcommand)]
pub enum StocksCmd {
    /// Delayed quote for one or more tickers (comma or space separated).
    #[command(
        after_help = "Examples:\n  idx stocks quote BBCA\n  idx stocks quote BBCA,BBRI,TLKM\n  idx -o json stocks quote bbca bbri"
    )]
    Quote {
        /// IDX tickers, e.g. BBCA or BBCA,BBRI. The .JK suffix is optional.
        #[arg(required = true, value_name = "SYMBOLS")]
        symbols: Vec<String>,
    },

    /// OHLCV price history.
    #[command(
        after_help = "Examples:\n  idx stocks history BBCA\n  idx stocks history TLKM --period 5y --interval 1mo\n  idx stocks history BBRI --period 5d --interval 1h --limit 0"
    )]
    History {
        /// IDX ticker, e.g. BBCA.
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        /// Look-back window.
        #[arg(long, short = 'p', value_enum, default_value = "1y")]
        period: Period,
        /// Bar size. Intraday sizes (5m..1h) only work with short periods.
        #[arg(long, short = 'i', value_enum, default_value = "1d")]
        interval: Interval,
        /// Rows shown in table mode (most recent first kept); 0 shows everything. JSON is never limited.
        #[arg(long, short = 'n', default_value_t = 30)]
        limit: usize,
    },

    /// Company profile: sector, industry, description, size, management.
    #[command(
        after_help = "Examples:\n  idx stocks profile BBCA\n  idx -o json stocks profile TLKM | jq .data.sector"
    )]
    Profile {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
    },

    /// Income statement, balance sheet and cash flow (annual, quarterly or TTM).
    #[command(
        after_help = "Examples:\n  idx stocks financials BBCA\n  idx stocks financials TLKM --period quarterly --statement income\n  idx -o json stocks financials ASII --period ttm"
    )]
    Financials {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        /// Reporting series.
        #[arg(long, short = 'p', value_enum, default_value = "annual")]
        period: StatementPeriod,
        /// Which statement to show in table mode (JSON always carries all three).
        #[arg(long, short = 's', value_enum, default_value = "all")]
        statement: Statement,
    },

    /// Reported quarterly EPS vs estimates, yearly revenue and earnings, next earnings date.
    #[command(
        after_help = "Examples:\n  idx stocks earnings BBCA\n  idx -o json stocks earnings BBRI | jq .data.next"
    )]
    Earnings {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
    },

    /// Cash dividend and split history with yearly totals, trailing-twelve-month yield and the payout streak.
    #[command(
        after_help = "Examples:\n  idx stocks dividends BBCA\n  idx stocks dividends TLKM --limit 20\n  idx -o json stocks dividends BBRI | jq '.data | {ttm_total, ttm_yield_pct, consecutive_years}'\n\nAmounts are per share as reported by the source (split-adjusted on Yahoo), keyed by ex-date.\nThe yearly totals, TTM yield (TTM total / last price) and streak are computed by idx."
    )]
    Dividends {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        /// Most recent payments listed in table mode (JSON carries all).
        #[arg(long, short = 'n', default_value_t = 12)]
        limit: usize,
    },

    /// Price returns over 1w..5y, YTD, CAGR, volatility, drawdowns and 52-week position, with excess returns over IHSG.
    #[command(
        after_help = "Examples:\n  idx stocks returns BBCA\n  idx stocks returns BBCA BBRI BMRI BBNI          # side by side, plus IHSG\n  idx stocks returns TLKM --benchmark ^JKLQ45\n  idx stocks returns ASII --no-benchmark\n  idx -o json stocks returns BBCA | jq '.data[0] | {returns, benchmark}'\n\nComputed by idx from five years of daily closes (total price return, dividends excluded).\nIndex codes keep their caret: ^JKSE is IHSG, ^JKLQ45 is LQ45."
    )]
    Returns {
        /// One or more IDX tickers, comma or space separated.
        #[arg(required = true, value_name = "SYMBOLS")]
        symbols: Vec<String>,
        /// Index to measure excess returns against.
        #[arg(long, default_value = "^JKSE", value_name = "INDEX")]
        benchmark: String,
        /// Skip the benchmark fetch.
        #[arg(long)]
        no_benchmark: bool,
    },

    /// Valuation, profitability, balance-sheet, growth and dividend ratios with a plain-language verdict.
    #[command(
        after_help = "Examples:\n  idx stocks fundamental BBCA\n  idx -o json stocks fundamental TLKM | jq '.data.verdict'\n\nThe verdict is a transparent rule score over public ratios (every rule that fires is listed). It is not investment advice."
    )]
    Fundamental {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
    },

    /// RSI, MACD, moving averages, Bollinger bands, support/resistance, trend and a buy/sell/neutral signal.
    #[command(
        after_help = "Examples:\n  idx stocks technical BBRI\n  idx stocks technical TLKM --period 2y\n  idx -o json stocks technical BBCA | jq '.data.signal'\n\nIndicators are computed locally from daily candles; the signal is a rule score, not investment advice."
    )]
    Technical {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        /// Daily candles to analyse (1y gives the 200-day average room to work).
        #[arg(long, short = 'p', value_enum, default_value = "1y")]
        period: Period,
    },

    /// Screen the IDX universe (~950 tickers) by valuation, size, dividend and, optionally, profitability.
    #[command(
        after_help = "Examples:\n  idx stocks screen -f \"pe<10,div>5,mcap>5T\"\n  idx stocks screen -f \"roe>15,pe<15,sector=financial\" --sort roe\n  idx stocks screen -f \"mcap>50T\" --fundamentals --sort margin -n 10\n\nKeys: pe fpe pb ps peg mcap price div eps volume chg (from one batch quote call);\n      roe roa margin opmargin de growth epsgrowth sector industry (per-company fundamentals,\n      fetched for the largest --max-fundamentals companies that pass the quote filters).\nOperators: < <= > >= = !=. Numbers accept K M B T suffixes and a trailing %.\nUniverse: bundled list of IDX issuers plus any seen by `idx ownership import`; override with --universe or IDX_UNIVERSE."
    )]
    Screen {
        /// Comma-separated clauses, e.g. "pe<15,roe>15,sector=Banking".
        #[arg(long, short = 'f', value_name = "SPEC", required = true)]
        filter: String,
        /// Sort key (any filter key); descending unless --asc.
        #[arg(long, default_value = "mcap", value_name = "KEY")]
        sort: String,
        #[arg(long)]
        asc: bool,
        /// Rows shown; 0 for all.
        #[arg(long, short = 'n', default_value_t = 25)]
        limit: usize,
        /// Cap on per-company fundamentals calls.
        #[arg(long, default_value_t = 50, value_name = "N")]
        max_fundamentals: usize,
        /// Fetch fundamentals for the results even when no filter needs them.
        #[arg(long)]
        fundamentals: bool,
        /// CSV of symbol,name to screen instead of the bundled universe.
        #[arg(long, value_name = "PATH")]
        universe: Option<std::path::PathBuf>,
    },

    /// Latest headlines from Kontan and CNBC Indonesia; with a ticker, the company's headlines first, then market context.
    #[command(
        after_help = "Examples:\n  idx stocks news                # market headlines\n  idx stocks news BBCA           # BBCA / Bank Central Asia first, then market headlines\n  idx stocks news BBCA --company-only\n  idx stocks news TLKM --source kontan --limit 40\n\nSources: kontan, cnbcindonesia (defaults), bisnis (blocked for non-browser clients; tried on request), all."
    )]
    News {
        #[arg(value_name = "SYMBOL")]
        symbol: Option<String>,
        /// Publisher names; repeatable.
        #[arg(long, short = 's', value_name = "NAME")]
        source: Vec<String>,
        /// Headlines shown; 0 for all.
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: usize,
        /// Only headlines that mention the ticker; no market context.
        #[arg(long, requires = "symbol")]
        company_only: bool,
    },

    /// Lexicon sentiment over the headlines that mention one ticker.
    #[command(
        after_help = "Examples:\n  idx stocks sentiment BBCA\n  idx -o json stocks sentiment BBRI | jq '.data | {label, score, items}'\n\nCounts Indonesian and English finance terms per headline. Indicative only."
    )]
    Sentiment {
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        #[arg(long, short = 's', value_name = "NAME")]
        source: Vec<String>,
        /// Strongest headlines shown.
        #[arg(long, short = 'n', default_value_t = 10)]
        limit: usize,
    },

    /// Side-by-side fundamentals for two or more tickers.
    #[command(
        after_help = "Examples:\n  idx stocks compare BBCA BBRI BMRI BBNI\n  idx -o json stocks compare TLKM,ISAT,EXCL | jq '.data[].valuation.pe_trailing'"
    )]
    Compare {
        /// Two or more IDX tickers, comma or space separated.
        #[arg(required = true, value_name = "SYMBOLS")]
        symbols: Vec<String>,
    },
}

pub async fn run(ctx: &Context, cmd: StocksCmd) -> Result<()> {
    match cmd {
        StocksCmd::Quote { symbols } => quote(ctx, &symbols).await,
        StocksCmd::History {
            symbol,
            period,
            interval,
            limit,
        } => history(ctx, &symbol, period, interval, limit).await,
        StocksCmd::Profile { symbol } => profile(ctx, &symbol).await,
        StocksCmd::Financials {
            symbol,
            period,
            statement,
        } => financials(ctx, &symbol, period, statement).await,
        StocksCmd::Earnings { symbol } => earnings(ctx, &symbol).await,
        StocksCmd::Dividends { symbol, limit } => dividends(ctx, &symbol, limit).await,
        StocksCmd::Returns {
            symbols,
            benchmark,
            no_benchmark,
        } => returns(ctx, &symbols, &benchmark, no_benchmark).await,
        StocksCmd::Fundamental { symbol } => fundamental(ctx, &symbol).await,
        StocksCmd::Technical { symbol, period } => technical(ctx, &symbol, period).await,
        StocksCmd::Compare { symbols } => compare(ctx, &symbols).await,
        StocksCmd::Screen {
            filter,
            sort,
            asc,
            limit,
            max_fundamentals,
            fundamentals,
            universe,
        } => {
            let filters = parse_filters(&filter)?;
            let Some(sort_key) = Key::parse(&sort) else {
                return Err(IdxError::Usage(format!(
                    "unknown sort key {sort:?}; use one of {}",
                    Key::ALL
                )));
            };
            let mut list = universe::load(universe.as_deref())?;
            universe::extend_from_ownership(&mut list, &ctx.settings.ownership_db);
            let opts = ScreenOptions {
                filters,
                sort: sort_key,
                descending: !asc,
                limit,
                max_fundamentals,
                with_fundamentals: fundamentals,
            };
            let result = service::screen(ctx.provider.as_ref(), &list, &opts).await?;
            if ctx.json() {
                let env = Envelope::from_sourced(SCHEMA_SCREEN, result);
                return output::print_stdout(&output::json::to_pretty(&env)?);
            }
            notices(ctx, &result);
            output::print_stdout(&newstbl::screen(&result.data, ctx.color))
        }
        StocksCmd::News {
            symbol,
            source,
            limit,
            company_only,
        } => {
            let sym = symbol.map(|s| Symbol::parse(&s)).transpose()?;
            let list = universe::load(None).unwrap_or_default();
            let result = service::news(
                &ctx.fetcher,
                &list,
                sym.as_ref(),
                &source,
                limit,
                company_only,
            )
            .await?;
            if ctx.json() {
                let env = Envelope::from_sourced(SCHEMA_NEWS, result);
                return output::print_stdout(&output::json::to_pretty(&env)?);
            }
            notices(ctx, &result);
            output::print_stdout(&newstbl::news(&result.data, ctx.color))
        }
        StocksCmd::Sentiment {
            symbol,
            source,
            limit,
        } => {
            let sym = Symbol::parse(&symbol)?;
            let list = universe::load(None).unwrap_or_default();
            let result = service::sentiment(&ctx.fetcher, &list, &sym, &source, limit).await?;
            if ctx.json() {
                let env = Envelope::from_sourced(SCHEMA_SENTIMENT, result);
                return output::print_stdout(&output::json::to_pretty(&env)?);
            }
            notices(ctx, &result);
            output::print_stdout(&newstbl::sentiment(&result.data, ctx.color))
        }
    }
}

/// Prints warnings and the stale notice in table mode.
fn notices<T>(ctx: &Context, s: &Sourced<T>) {
    if ctx.json() {
        return;
    }
    for w in &s.warnings {
        output::print_stderr(&format!("warning: {w}"));
    }
    if s.stale {
        output::print_stderr("warning: showing stale cached data");
    }
}

async fn quote(ctx: &Context, raw: &[String]) -> Result<()> {
    let symbols = Symbol::parse_list(raw)?;
    let result = ctx.provider.quotes(&symbols).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_QUOTES, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&table::quotes(&result.data, ctx.color))
}

async fn history(
    ctx: &Context,
    raw: &str,
    period: Period,
    interval: Interval,
    limit: usize,
) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = ctx.provider.history(&symbol, period, interval).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_HISTORY, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    let limit = if limit == 0 { None } else { Some(limit) };
    output::print_stdout(&table::history(&result.data, limit, ctx.color))
}

async fn profile(ctx: &Context, raw: &str) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = ctx.provider.profile(&symbol).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_PROFILE, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::profile(&result.data, ctx.color))
}

async fn financials(
    ctx: &Context,
    raw: &str,
    period: StatementPeriod,
    statement: Statement,
) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = ctx.provider.financials(&symbol, period).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_FINANCIALS, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    let flags = StatementFlags {
        income: matches!(statement, Statement::All | Statement::Income),
        balance: matches!(statement, Statement::All | Statement::Balance),
        cash_flow: matches!(statement, Statement::All | Statement::Cash),
    };
    output::print_stdout(&reports::financials(&result.data, &flags, ctx.color))
}

async fn earnings(ctx: &Context, raw: &str) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = ctx.provider.earnings(&symbol).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_EARNINGS, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::earnings(&result.data, ctx.color))
}

async fn dividends(ctx: &Context, raw: &str, limit: usize) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = service::dividends(ctx.provider.as_ref(), &symbol).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_DIVIDENDS, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::dividends(&result.data, limit, ctx.color))
}

async fn returns(ctx: &Context, raw: &[String], benchmark: &str, none: bool) -> Result<()> {
    let symbols = Symbol::parse_list(raw)?;
    let bench = if none || benchmark.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(Symbol::parse(benchmark)?)
    };
    let result = service::returns(ctx.provider.as_ref(), &symbols, bench.as_ref()).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_RETURNS, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::returns(&result.data, ctx.color))
}

async fn fundamental(ctx: &Context, raw: &str) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = service::fundamentals(ctx.provider.as_ref(), &symbol).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_FUNDAMENTALS, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::fundamentals(&result.data, ctx.color))
}

async fn technical(ctx: &Context, raw: &str, period: Period) -> Result<()> {
    let symbol = Symbol::parse(raw)?;
    let result = service::technical(ctx.provider.as_ref(), &symbol, period).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_TECHNICAL, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::technical(&result.data, ctx.color))
}

async fn compare(ctx: &Context, raw: &[String]) -> Result<()> {
    let symbols = Symbol::parse_list(raw)?;
    let result = service::compare(ctx.provider.as_ref(), &symbols).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_COMPARE, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    notices(ctx, &result);
    output::print_stdout(&reports::compare(&result.data, ctx.color))
}
