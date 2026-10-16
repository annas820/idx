//! `idx stocks ...`

use clap::Subcommand;

use crate::error::Result;
use crate::model::{Interval, Period, Symbol};
use crate::output::json::{Envelope, SCHEMA_HISTORY, SCHEMA_QUOTES};
use crate::output::{self, table};

use super::Context;

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
    }
}

async fn quote(ctx: &Context, raw: &[String]) -> Result<()> {
    let symbols = Symbol::parse_list(raw)?;
    let result = ctx.provider.quotes(&symbols).await?;
    if ctx.json() {
        let env = Envelope::from_sourced(SCHEMA_QUOTES, result);
        return output::print_stdout(&output::json::to_pretty(&env)?);
    }
    for w in &result.warnings {
        output::print_stderr(&format!("warning: {w}"));
    }
    if result.stale {
        output::print_stderr("warning: showing stale cached data");
    }
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
    for w in &result.warnings {
        output::print_stderr(&format!("warning: {w}"));
    }
    if result.stale {
        output::print_stderr("warning: showing stale cached data");
    }
    let limit = if limit == 0 { None } else { Some(limit) };
    output::print_stdout(&table::history(&result.data, limit, ctx.color))
}
