//! `idx broker`: which brokers bought and sold a ticker over a date range.
//!
//! A broker summary as the source publishes it: the top buyers and the top
//! sellers by lots, each with lots, value and average price, plus totals
//! for the whole range. The source abbreviates large figures (`1.6 M` lots,
//! `841.2 B` IDR); those become plain numbers at the published precision and
//! `rounded` says so. Nothing is netted or estimated here.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;
use crate::error::{IdxError, Result};

/// Whose trades are counted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum BrokerInvestor {
    /// Every trade.
    All,
    /// Trades by foreign investors only.
    Foreign,
    /// Trades by domestic investors only.
    Domestic,
}

impl BrokerInvestor {
    pub fn as_str(&self) -> &'static str {
        match self {
            BrokerInvestor::All => "all",
            BrokerInvestor::Foreign => "foreign",
            BrokerInvestor::Domestic => "domestic",
        }
    }
}

impl std::str::FromStr for BrokerInvestor {
    type Err = IdxError;
    fn from_str(s: &str) -> Result<Self> {
        <BrokerInvestor as clap::ValueEnum>::from_str(s, true).map_err(|_| {
            IdxError::Usage(format!(
                "invalid investor {s:?}; use all, foreign or domestic"
            ))
        })
    }
}

/// Which market board's trades are counted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum BrokerBoard {
    /// Every board.
    All,
    /// Regular market (RG).
    Regular,
    /// Cash market (TN).
    Cash,
    /// Negotiated market (NG).
    Negotiated,
}

impl BrokerBoard {
    pub fn as_str(&self) -> &'static str {
        match self {
            BrokerBoard::All => "all",
            BrokerBoard::Regular => "regular",
            BrokerBoard::Cash => "cash",
            BrokerBoard::Negotiated => "negotiated",
        }
    }
}

impl std::str::FromStr for BrokerBoard {
    type Err = IdxError;
    fn from_str(s: &str) -> Result<Self> {
        <BrokerBoard as clap::ValueEnum>::from_str(s, true).map_err(|_| {
            IdxError::Usage(format!(
                "invalid board {s:?}; use all, regular, cash or negotiated"
            ))
        })
    }
}

/// The source's classification of a broker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrokerKind {
    Local,
    Foreign,
    /// State-owned securities house (BUMN).
    StateOwned,
}

impl BrokerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BrokerKind::Local => "local",
            BrokerKind::Foreign => "foreign",
            BrokerKind::StateOwned => "state_owned",
        }
    }
}

/// One broker on one side of the summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrokerFlow {
    /// Position on its side as the source ranks it, 1 = most lots.
    pub rank: u32,
    /// IDX member code, e.g. `CC`.
    pub code: String,
    /// Member name from the IDX exchange-member list bundled with idx;
    /// `null` for a code that is not on it.
    pub name: Option<String>,
    /// The source's classification; `null` when it gives none.
    pub kind: Option<BrokerKind>,
    /// Lots bought (buyers) or sold (sellers); one lot is 100 shares.
    pub lots: Option<f64>,
    /// Value in IDR.
    pub value: Option<f64>,
    /// Average price in IDR as published.
    pub average_price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrokerSummary {
    pub symbol: Symbol,
    /// Company name from the bundled universe; `null` when unknown.
    pub name: Option<String>,
    /// First trading date covered (WIB), as applied by the source.
    pub from: NaiveDate,
    /// Last trading date covered (WIB), as applied by the source.
    pub to: NaiveDate,
    pub investor: BrokerInvestor,
    pub board: BrokerBoard,
    /// Top buyers by lots, most first, as published (up to 10).
    pub buyers: Vec<BrokerFlow>,
    /// Top sellers by lots, most first, as published (up to 10).
    pub sellers: Vec<BrokerFlow>,
    /// Value traded over the range, IDR.
    pub total_value: Option<f64>,
    /// Lots traded over the range.
    pub total_lots: Option<f64>,
    /// Foreign buy value minus foreign sell value, IDR, as published.
    pub foreign_net_value: Option<f64>,
    /// Average traded price, IDR; `null` when nothing traded.
    pub average_price: Option<f64>,
    /// `true` when the source abbreviated at least one figure (`1.6 M`), so
    /// those numbers carry only its one-decimal precision.
    pub rounded: bool,
}

impl BrokerSummary {
    /// No trades in the range (weekend, holiday, suspension).
    pub fn is_empty(&self) -> bool {
        self.buyers.is_empty() && self.sellers.is_empty()
    }
}
