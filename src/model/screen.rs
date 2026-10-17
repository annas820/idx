//! `stocks screen` output.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

/// One matching stock. Quote-level fields come from the batch quote call;
/// the fundamentals block is filled only when a filter or `--fundamentals`
/// asked for it (see `fundamentals_checked`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenRow {
    pub symbol: Symbol,
    pub name: Option<String>,
    pub price: Option<f64>,
    pub change_pct: Option<f64>,
    pub market_cap: Option<f64>,
    pub pe_trailing: Option<f64>,
    pub pe_forward: Option<f64>,
    pub price_to_book: Option<f64>,
    pub dividend_yield_pct: Option<f64>,
    pub eps_ttm: Option<f64>,
    pub avg_volume_3m: Option<u64>,
    pub week52_high: Option<f64>,
    pub week52_low: Option<f64>,
    /// `true` when the fundamentals block below was fetched for this row.
    pub fundamentals_checked: bool,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub roe_pct: Option<f64>,
    pub roa_pct: Option<f64>,
    pub net_margin_pct: Option<f64>,
    pub operating_margin_pct: Option<f64>,
    pub debt_to_equity: Option<f64>,
    pub revenue_growth_pct: Option<f64>,
    pub earnings_growth_pct: Option<f64>,
    pub peg: Option<f64>,
    pub price_to_sales: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenReport {
    /// Filters as parsed, one per clause.
    pub filters: Vec<String>,
    pub sort: String,
    pub descending: bool,
    /// Tickers in the universe that was screened.
    pub universe: usize,
    /// Tickers the quote source returned.
    pub quoted: usize,
    /// Rows left after the quote-level filters.
    pub matched_quote_level: usize,
    /// Rows for which fundamentals were fetched.
    pub fundamentals_checked: usize,
    /// `true` when more rows qualified for a fundamentals check than the cap allowed.
    pub fundamentals_capped: bool,
    pub total_matches: usize,
    pub rows: Vec<ScreenRow>,
}
