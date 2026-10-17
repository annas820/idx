//! `idx watch`: a local list of tickers kept next to the config file.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Watchlist {
    /// File the list is stored in (one ticker per line, `#` comments allowed).
    pub path: String,
    /// In file order, upper-case, unique.
    pub symbols: Vec<Symbol>,
}
