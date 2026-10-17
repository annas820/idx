//! `idx market ...`: the whole board in one call.

use clap::Subcommand;

use crate::error::Result;
use crate::model::Symbol;
use crate::output::json::SCHEMA_MARKET;
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
    }
}
