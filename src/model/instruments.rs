//! Directories the exchange publishes: listed stocks (`idx stocks list`),
//! exchange members (`idx market members`) and the daily index board
//! (`idx market indices`).

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

/// One listed stock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Instrument {
    pub symbol: Symbol,
    /// Company name as the source publishes it (IDX cuts long names at 30 characters).
    pub name: Option<String>,
    /// First listing date on IDX.
    pub listing_date: Option<NaiveDate>,
    /// Listed shares.
    pub shares: Option<f64>,
    /// Listing board as IDX names it: `Utama`, `Pengembangan`, `Akselerasi`,
    /// `Ekonomi Baru` or `Pemantauan Khusus`.
    pub board: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InstrumentList {
    /// Stocks on the list before the filters.
    pub total: usize,
    /// Board filter applied (case-insensitive substring), if any.
    pub board: Option<String>,
    /// Code or name filter applied (case-insensitive substring), if any.
    pub search: Option<String>,
    /// Sorted by symbol.
    pub instruments: Vec<Instrument>,
}

/// One exchange member (Anggota Bursa): a broker code such as `CC`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Member {
    pub code: String,
    pub name: Option<String>,
    /// Licences as IDX lists them (e.g. `Perantara Pedagang Efek`,
    /// `Penjamin Emisi Efek`); `null` when the source gives none.
    pub licenses: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemberList {
    /// Members on the list before the filter.
    pub total: usize,
    /// Code or name filter applied (case-insensitive substring), if any.
    pub search: Option<String>,
    /// Sorted by code.
    pub members: Vec<Member>,
}

/// One index on one trading day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexRow {
    /// IDX index code: `COMPOSITE` (IHSG, `^JKSE` on Yahoo), `LQ45`, `IDX30`, ...
    pub code: String,
    pub previous: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub change: Option<f64>,
    /// `change / previous * 100`, computed by idx.
    pub change_pct: Option<f64>,
    /// Stocks in the index.
    pub constituents: Option<u32>,
    /// Shares traded in the constituents.
    pub volume: Option<f64>,
    /// Value traded in the constituents, IDR.
    pub value: Option<f64>,
    /// Trades in the constituents.
    pub frequency: Option<f64>,
    /// Constituents' market capitalisation, IDR.
    pub market_cap: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexBoard {
    /// Trading day the figures belong to.
    pub date: NaiveDate,
    /// In the order the source lists them (IHSG first).
    pub indices: Vec<IndexRow>,
}
