//! `idx stocks ...`

use clap::Subcommand;

use crate::error::Result;
use crate::model::{Interval, Period, StatementPeriod, Symbol};
use crate::output::json::{
    Envelope, SCHEMA_COMPARE, SCHEMA_EARNINGS, SCHEMA_FINANCIALS, SCHEMA_FUNDAMENTALS,
    SCHEMA_HISTORY, SCHEMA_PROFILE, SCHEMA_QUOTES, SCHEMA_TECHNICAL,
};
use crate::output::table_reports::{self as reports, StatementFlags};
use crate::output::{self, table};
use crate::providers::Sourced;
use crate::service;

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
        StocksCmd::Fundamental { symbol } => fundamental(ctx, &symbol).await,
        StocksCmd::Technical { symbol, period } => technical(ctx, &symbol, period).await,
        StocksCmd::Compare { symbols } => compare(ctx, &symbols).await,
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
