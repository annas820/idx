//! Technical analysis output for `stocks technical`. Computed locally from
//! daily candles (`analysis::technical`), so every field is derived, never
//! fetched. Values are `null` when there is not enough history.

use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Period, Symbol};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MovingAverages {
    pub sma20: Option<f64>,
    pub sma50: Option<f64>,
    pub sma200: Option<f64>,
    pub ema20: Option<f64>,
    pub ema50: Option<f64>,
    pub ema200: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Macd {
    pub line: f64,
    pub signal: f64,
    pub histogram: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bollinger {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    /// Where the close sits inside the bands: 0 = lower band, 1 = upper band.
    pub percent_b: f64,
    /// Band width as a percent of the middle band.
    pub bandwidth_pct: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VolumeStats {
    pub last: Option<u64>,
    pub avg20: Option<u64>,
    /// `last / avg20`.
    pub ratio: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Levels {
    /// Recent swing lows below the current price, nearest first.
    pub support: Vec<f64>,
    /// Recent swing highs above the current price, nearest first.
    pub resistance: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Uptrend,
    Downtrend,
    Sideways,
    Unknown,
}

impl Trend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Trend::Uptrend => "uptrend",
            Trend::Downtrend => "downtrend",
            Trend::Sideways => "sideways",
            Trend::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignalLabel {
    Buy,
    Sell,
    Neutral,
}

impl SignalLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignalLabel::Buy => "buy",
            SignalLabel::Sell => "sell",
            SignalLabel::Neutral => "neutral",
        }
    }
}

/// Rule-based signal. Heuristic, not advice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Signal {
    pub label: SignalLabel,
    /// Sum of rule scores; positive is bullish.
    pub score: i32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Technical {
    pub symbol: Symbol,
    pub name: Option<String>,
    pub currency: Option<String>,
    /// Time of the last candle used.
    pub as_of: DateTime<FixedOffset>,
    /// Look-back window of the candles that fed the indicators.
    pub period: Period,
    pub candles_used: usize,
    pub price: f64,
    pub change_pct_1d: Option<f64>,
    pub change_pct_1w: Option<f64>,
    pub change_pct_1m: Option<f64>,
    pub change_pct_3m: Option<f64>,
    pub high_52w: Option<f64>,
    pub low_52w: Option<f64>,
    pub moving_averages: MovingAverages,
    /// 14-period Wilder RSI.
    pub rsi14: Option<f64>,
    /// 12/26/9 MACD.
    pub macd: Option<Macd>,
    /// 20-period, 2 standard deviations.
    pub bollinger: Option<Bollinger>,
    /// 14-period average true range.
    pub atr14: Option<f64>,
    pub volume: VolumeStats,
    pub levels: Levels,
    pub trend: Trend,
    pub signal: Signal,
}
