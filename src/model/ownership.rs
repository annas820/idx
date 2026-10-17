//! Shareholder-register types for `idx ownership ...`. The data is KSEI's
//! monthly "Pemegang Saham di Atas 1%" list as published by IDX, imported into
//! a local SQLite file. These structs are the JSON contract.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One holder line as imported (one entity's position in one issuer on one date).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Holding {
    pub as_of: NaiveDate,
    pub symbol: String,
    pub issuer_name: Option<String>,
    /// Normalised display name of the holder (see `entity_key`).
    pub holder: String,
    /// Name exactly as printed in the source file.
    pub holder_raw: String,
    /// Stable key that groups spelling variants of the same holder.
    pub entity_key: String,
    /// KSEI investor type, upper-case (`CORPORATE`, `INDIVIDUAL`, `MUTUAL FUND`, ...).
    pub investor_type: Option<String>,
    /// `L` (local) or `A` (foreign) as reported.
    pub local_foreign: Option<String>,
    pub nationality: Option<String>,
    pub domicile: Option<String>,
    pub shares_scripless: Option<i64>,
    pub shares_scrip: Option<i64>,
    pub shares_total: i64,
    /// Percentage of issued shares, e.g. `54.94`.
    pub pct: f64,
}

/// `ownership ticker`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HoldersReport {
    pub symbol: String,
    pub issuer_name: Option<String>,
    pub as_of: NaiveDate,
    /// Sum of `pct` over the listed holders (the rest is below 1 % each).
    pub listed_pct: f64,
    pub holders: Vec<Holding>,
}

/// `ownership entity`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EntityReport {
    pub query: String,
    pub as_of: NaiveDate,
    /// Distinct entity keys that matched the query.
    pub entities: Vec<String>,
    pub holdings: Vec<Holding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Present in both releases with a different position.
    Changed,
    /// Absent from the earlier release (crossed above 1 % or newly listed).
    New,
    /// Absent from the later release (dropped below 1 % or sold out).
    Exited,
}

/// One holder's movement between two releases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Change {
    pub symbol: String,
    pub holder: String,
    pub entity_key: String,
    pub investor_type: Option<String>,
    pub kind: ChangeKind,
    pub pct_from: Option<f64>,
    pub pct_to: Option<f64>,
    /// `pct_to - pct_from` in percentage points (missing side counts as 0).
    pub pct_change: f64,
    pub shares_from: Option<i64>,
    pub shares_to: Option<i64>,
    pub shares_change: i64,
}

/// `ownership changes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChangesReport {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub symbol: Option<String>,
    /// Minimum absolute change in percentage points that was applied.
    pub min_pct_change: f64,
    pub total_matches: usize,
    /// Largest absolute moves first; may be truncated by `--limit`.
    pub changes: Vec<Change>,
}

/// One imported monthly file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Release {
    pub as_of: NaiveDate,
    /// `xlsx` or `pdf`.
    pub format: String,
    /// File path or URL the rows came from.
    pub source: String,
    pub sha256: String,
    pub rows: u64,
    pub issuers: u64,
    pub entities: u64,
    pub imported_at: String,
}

/// `ownership import`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportReport {
    pub release: Release,
    /// `true` when this `as_of` already existed and was replaced.
    pub replaced: bool,
    pub database: String,
}

/// A downloadable monthly file found on IDX's "Data Kepemilikan Saham" page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiscoveredFile {
    pub as_of: NaiveDate,
    pub title: String,
    pub url: String,
    pub format: String,
}
