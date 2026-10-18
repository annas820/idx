//! `idx market ...`: the whole board in one call.

use chrono::Datelike;
use clap::Subcommand;

use crate::error::Result;
use crate::model::Symbol;
use crate::output::json::{SCHEMA_HOLIDAYS, SCHEMA_MARKET};
use crate::output::table_exchange as tx;
use crate::output::table_reports as reports;
use crate::service;
use crate::universe;

use super::{emit, Context};

#[derive(Debug, Subcommand)]
pub enum MarketCmd {
    /// IHSG, breadth, gainers, losers, most traded and heavyweights from one batch of quotes.
    #[command(
        after_help = "Examples:\n  idx market overview\n  idx market overview --top 15\n  idx -o json market overview | jq '.data | {index, advancing, declining}'\n\nWalks the bundled universe (~950 tickers) in batched quote calls; gainers and losers are\nlimited to companies above the liquidity floor (--min-mcap) with volume today."
    )]
    Overview {
        /// Rows per list.
        #[arg(long, short = 'n', default_value_t = 10)]
        top: usize,
        /// Market-cap floor for gainers/losers, in IDR (accepts K/M/B/T).
        #[arg(long, default_value = "1T", value_name = "IDR")]
        min_mcap: String,
        /// Index symbol shown at the top.
        #[arg(long, default_value = "^JKSE", value_name = "INDEX")]
        index: String,
        /// CSV of symbol,name to walk instead of the bundled universe.
        #[arg(long, value_name = "PATH")]
        universe: Option<std::path::PathBuf>,
    },
    /// Trading holidays (Kalender Libur Bursa), whether today is a trading day, and the next one.
    #[command(
        after_help = "Examples:\n  idx market holidays\n  idx market holidays --year 2027\n  idx market holidays --upcoming\n  idx -o json market holidays | jq '.data.next_trading_day'\n\nBundled from the yearly calendar idx.co.id publishes (weekend holidays are not listed);\nIDX can add a holiday later. Override the list with IDX_HOLIDAYS=/path/to.csv (date,name,name_en)."
    )]
    Holidays {
        /// Calendar year; default: the current year.
        #[arg(long, value_name = "YEAR", conflicts_with = "all")]
        year: Option<i32>,
        /// Every year on the list.
        #[arg(long)]
        all: bool,
        /// Only holidays from today on.
        #[arg(long)]
        upcoming: bool,
    },
}

pub async fn run(ctx: &Context, cmd: MarketCmd) -> Result<()> {
    match cmd {
        MarketCmd::Overview {
            top,
            min_mcap,
            index,
            universe: path,
        } => {
            let floor = crate::analysis::screen::parse_number(&min_mcap).ok_or_else(|| {
                crate::error::IdxError::Usage(format!(
                    "--min-mcap {min_mcap:?} is not a number (try 1T, 500B)"
                ))
            })?;
            let index = Symbol::parse(&index)?;
            let mut list = universe::load(path.as_deref())?;
            universe::extend_from_ownership(&mut list, &ctx.settings.ownership_db);
            let result =
                service::market_overview(ctx.provider.as_ref(), &list, &index, top, floor).await?;
            emit(ctx, SCHEMA_MARKET, result, |d| {
                reports::market(d, ctx.color)
            })
        }
        MarketCmd::Holidays {
            year,
            all,
            upcoming,
        } => {
            let today = service::today_wib();
            let year = if all {
                None
            } else {
                Some(year.unwrap_or(today.year()))
            };
            let result = service::holidays(year, upcoming, today)?;
            emit(ctx, SCHEMA_HOLIDAYS, result, |d| tx::holidays(d, ctx.color))
        }
    }
}
