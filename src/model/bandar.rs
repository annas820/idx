//! `idx bandar`: bandarmology readings built on the broker summary.
//!
//! "Bandar" is market slang for the large, concentrated money that moves a
//! stock. The broker summary only shows the top 10 buyers and top 10 sellers
//! (ranked by lots), so the reading compares those: when the top N buyers
//! bought much more value than the top N sellers sold, the buying is
//! concentrated (a few hands accumulate) and the selling is dispersed. Every figure here is derived
//! from the published summary and the price, with the rules in
//! `analysis::bandar`; nothing is estimated beyond the source's own rounding.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{BrokerBoard, BrokerFlow, BrokerInvestor, Symbol};
use crate::error::{IdxError, Result};

/// Look-back ending at the latest session with trades.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, clap::ValueEnum,
)]
pub enum BandarPeriod {
    /// The latest session.
    #[value(name = "1d")]
    #[serde(rename = "1d")]
    Day1,
    /// The seven calendar days to the latest session (five sessions).
    #[value(name = "1w")]
    #[serde(rename = "1w")]
    Week1,
    /// One calendar month to the latest session.
    #[value(name = "1m")]
    #[serde(rename = "1m")]
    Month1,
    /// Three calendar months to the latest session.
    #[value(name = "3m")]
    #[serde(rename = "3m")]
    Month3,
}

impl BandarPeriod {
    pub const ALL: [BandarPeriod; 4] = [
        BandarPeriod::Day1,
        BandarPeriod::Week1,
        BandarPeriod::Month1,
        BandarPeriod::Month3,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            BandarPeriod::Day1 => "1d",
            BandarPeriod::Week1 => "1w",
            BandarPeriod::Month1 => "1m",
            BandarPeriod::Month3 => "3m",
        }
    }

    /// First date of the period that ends on `end`, moved off a weekend.
    pub fn start(&self, end: NaiveDate) -> NaiveDate {
        use chrono::{Datelike, Weekday};
        let back = |months: u32| {
            end.checked_sub_months(chrono::Months::new(months))
                .map(|d| d + chrono::Days::new(1))
                .unwrap_or(end)
        };
        let mut d = match self {
            BandarPeriod::Day1 => end,
            BandarPeriod::Week1 => end - chrono::Days::new(6),
            BandarPeriod::Month1 => back(1),
            BandarPeriod::Month3 => back(3),
        };
        while d < end && matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            d = d + chrono::Days::new(1);
        }
        d
    }
}

impl std::str::FromStr for BandarPeriod {
    type Err = IdxError;
    fn from_str(s: &str) -> Result<Self> {
        <BandarPeriod as clap::ValueEnum>::from_str(s, true)
            .map_err(|_| IdxError::Usage(format!("invalid period {s:?}; use 1d, 1w, 1m or 3m")))
    }
}

/// Reading of the top-N net flow as a share of turnover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BandarSignal {
    /// Net at or above +15 % of turnover.
    BigAccumulation,
    /// Net from +5 % up to +15 %.
    Accumulation,
    /// Net between -5 % and +5 %, or nothing traded.
    Neutral,
    /// Net from -5 % down to -15 %.
    Distribution,
    /// Net at or below -15 % of turnover.
    BigDistribution,
}

impl BandarSignal {
    pub fn as_str(&self) -> &'static str {
        match self {
            BandarSignal::BigAccumulation => "big_accumulation",
            BandarSignal::Accumulation => "accumulation",
            BandarSignal::Neutral => "neutral",
            BandarSignal::Distribution => "distribution",
            BandarSignal::BigDistribution => "big_distribution",
        }
    }

    pub fn is_accumulation(&self) -> bool {
        matches!(
            self,
            BandarSignal::BigAccumulation | BandarSignal::Accumulation
        )
    }

    pub fn is_distribution(&self) -> bool {
        matches!(
            self,
            BandarSignal::BigDistribution | BandarSignal::Distribution
        )
    }
}

/// The signal read against the price move over the same range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BandarPhase {
    /// Accumulation while the price is flat or falling (within +2 %): buying quietly.
    QuietAccumulation,
    /// Accumulation with the price up more than 2 %.
    Markup,
    /// Distribution while the price is flat or rising (within -2 %): selling quietly.
    QuietDistribution,
    /// Distribution with the price down more than 2 %.
    Markdown,
    /// No accumulation or distribution signal.
    Neutral,
}

impl BandarPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            BandarPhase::QuietAccumulation => "quiet_accumulation",
            BandarPhase::Markup => "markup",
            BandarPhase::QuietDistribution => "quiet_distribution",
            BandarPhase::Markdown => "markdown",
            BandarPhase::Neutral => "neutral",
        }
    }
}

/// The top N buyers against the top N sellers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandarTier {
    /// N: brokers counted per side (1, 3, 5 or 10).
    pub top: u32,
    /// Value bought by the top N buyers, IDR.
    pub buy_value: f64,
    /// Value sold by the top N sellers, IDR.
    pub sell_value: f64,
    /// Bandar value: `buy_value - sell_value`, IDR.
    pub net_value: f64,
    /// Lots bought by the top N buyers.
    pub buy_lots: f64,
    /// Lots sold by the top N sellers.
    pub sell_lots: f64,
    /// Bandar volume: `buy_lots - sell_lots`.
    pub net_lots: f64,
    /// `net_value` as a percent of the range's total value; `null` when nothing traded.
    pub net_pct: Option<f64>,
    /// Top N buyers' share of total value, percent: how concentrated the buying is.
    pub buy_share_pct: Option<f64>,
    /// Top N sellers' share of total value, percent.
    pub sell_share_pct: Option<f64>,
    /// Volume-weighted average price of the top N buyers, IDR.
    pub buy_average: Option<f64>,
    /// Volume-weighted average price of the top N sellers, IDR.
    pub sell_average: Option<f64>,
    pub signal: BandarSignal,
}

/// One session of the daily series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandarDay {
    pub date: NaiveDate,
    /// Value traded that session, IDR.
    pub total_value: Option<f64>,
    /// Top-N net value that session, IDR.
    pub net_value: f64,
    pub net_pct: Option<f64>,
    pub signal: BandarSignal,
    /// Running sum of `net_value` from the first session of the series.
    pub cumulative_net_value: f64,
    /// Foreign buy minus foreign sell value that session, IDR, as published.
    pub foreign_net_value: Option<f64>,
}

/// Bandar movement: the reading session by session over the range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandarDaily {
    /// Sessions with trades, oldest first (holidays and weekends left out).
    pub days: Vec<BandarDay>,
    pub accumulation_days: u32,
    pub distribution_days: u32,
    pub neutral_days: u32,
    /// Consecutive latest sessions with the same direction: +3 = three
    /// accumulation sessions in a row, -2 = two distribution sessions, 0 = the
    /// latest session was neutral.
    pub streak: i32,
    /// Sum of the daily top-N net values, IDR. It differs from the range's
    /// own `net_value` because each day has its own top N.
    pub cumulative_net_value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandarReport {
    pub symbol: Symbol,
    /// Company name from the bundled universe; `null` when unknown.
    pub name: Option<String>,
    /// `1d`, `1w`, `1m`, `3m`, or `custom` for explicit dates.
    pub period: String,
    /// First trading date covered (WIB), as applied by the source.
    pub from: NaiveDate,
    /// Last trading date covered (WIB), as applied by the source.
    pub to: NaiveDate,
    pub investor: BrokerInvestor,
    pub board: BrokerBoard,
    /// N used for the headline reading (default 5).
    pub top: u32,
    /// Headline reading of the top-N tier.
    pub signal: BandarSignal,
    /// The signal read against `price_change_pct`; `null` without a price move.
    pub phase: Option<BandarPhase>,
    /// Top-N net value, IDR (the headline tier's `net_value`).
    pub net_value: f64,
    /// Top-N net lots (the headline tier's `net_lots`).
    pub net_lots: f64,
    /// Top-N net value as a percent of total value (the headline tier's `net_pct`).
    pub net_pct: Option<f64>,
    /// Last price of the range's final session, IDR.
    pub price: Option<f64>,
    /// Price change over the range: from the close before `from` to `price`, percent.
    pub price_change_pct: Option<f64>,
    /// Average price of the top-N buyers, IDR (the headline tier's `buy_average`).
    pub bandar_average: Option<f64>,
    /// `price` against `bandar_average`, percent: above zero the top buyers sit on a gain.
    pub bandar_gain_pct: Option<f64>,
    /// Value traded over the range, IDR.
    pub total_value: Option<f64>,
    /// Lots traded over the range.
    pub total_lots: Option<f64>,
    /// Foreign buy value minus foreign sell value, IDR, as published.
    pub foreign_net_value: Option<f64>,
    /// `foreign_net_value` as a percent of total value.
    pub foreign_net_pct: Option<f64>,
    /// Top 1, 3, 5 and 10 tiers.
    pub tiers: Vec<BandarTier>,
    /// The top-N buyers, as the source ranks them (by lots).
    pub buyers: Vec<BrokerFlow>,
    /// The top-N sellers, as the source ranks them (by lots).
    pub sellers: Vec<BrokerFlow>,
    /// Session-by-session series; only when asked for (`--daily`).
    pub daily: Option<BandarDaily>,
    /// The rules that produced the reading, in plain words.
    pub reasons: Vec<String>,
    /// `true` when the source abbreviated at least one figure (`1.6 M`).
    pub rounded: bool,
}

/// One stock in a bandar screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandarScreenRow {
    pub symbol: Symbol,
    pub name: Option<String>,
    pub signal: BandarSignal,
    pub phase: Option<BandarPhase>,
    /// Top-N net value as a percent of total value.
    pub net_pct: Option<f64>,
    /// Top-N net value, IDR.
    pub net_value: f64,
    /// Top-N net lots.
    pub net_lots: f64,
    pub price: Option<f64>,
    pub price_change_pct: Option<f64>,
    pub bandar_average: Option<f64>,
    pub bandar_gain_pct: Option<f64>,
    /// Top-N buyers' share of total value, percent.
    pub buy_share_pct: Option<f64>,
    pub total_value: Option<f64>,
    pub foreign_net_value: Option<f64>,
    pub foreign_net_pct: Option<f64>,
    /// Codes of the top-N buyers, as the source ranks them (by lots).
    pub top_buyers: Vec<String>,
    /// Codes of the top-N sellers, as the source ranks them (by lots).
    pub top_sellers: Vec<String>,
    pub rounded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandarScreen {
    pub period: BandarPeriod,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub top: u32,
    /// Preset name when one was used.
    pub preset: Option<String>,
    /// Every clause applied, the preset's included.
    pub filters: Vec<String>,
    pub sort: String,
    pub descending: bool,
    /// Tickers considered: the most traded by value, or the ones given.
    pub scanned: usize,
    /// Scanned tickers with a broker summary that had trades.
    pub checked: usize,
    pub total_matches: usize,
    pub rows: Vec<BandarScreenRow>,
}
