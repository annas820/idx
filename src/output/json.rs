//! The JSON envelope every command emits with `-o json`.

use chrono::{DateTime, SubsecRound, Utc};
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{IdxError, Result};
use crate::model::{Earnings, Financials, Fundamentals, History, Profile, Quote, Technical};
use crate::providers::Sourced;

/// Stable wrapper. `schema` names the payload shape and is versioned
/// independently of the binary (`idx.quotes.v1`, `idx.history.v1`, ...).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Envelope<T> {
    /// Payload identifier, e.g. `idx.quotes.v1`.
    pub schema: String,
    /// Provider that produced the data (`yahoo`, `msn`, ...).
    pub provider: String,
    /// When the underlying response was fetched from the network (UTC).
    pub fetched_at: DateTime<Utc>,
    /// `true` when served from the local cache.
    pub cached: bool,
    /// `true` when the cached copy is older than its TTL (offline or network failure).
    pub stale: bool,
    /// Non-fatal notes, e.g. symbols the source did not know.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    pub data: T,
}

impl<T> Envelope<T> {
    pub fn from_sourced(schema: &str, s: Sourced<T>) -> Self {
        Envelope {
            schema: schema.to_string(),
            provider: s.provider.to_string(),
            fetched_at: s.fetched_at.trunc_subsecs(0),
            cached: s.cached,
            stale: s.stale,
            warnings: s.warnings,
            data: s.data,
        }
    }
}

pub const SCHEMA_QUOTES: &str = "idx.quotes.v1";
pub const SCHEMA_HISTORY: &str = "idx.history.v1";
pub const SCHEMA_PROFILE: &str = "idx.profile.v1";
pub const SCHEMA_FUNDAMENTALS: &str = "idx.fundamentals.v1";
pub const SCHEMA_COMPARE: &str = "idx.compare.v1";
pub const SCHEMA_FINANCIALS: &str = "idx.financials.v1";
pub const SCHEMA_EARNINGS: &str = "idx.earnings.v1";
pub const SCHEMA_TECHNICAL: &str = "idx.technical.v1";

pub type QuotesEnvelope = Envelope<Vec<Quote>>;
pub type HistoryEnvelope = Envelope<History>;
pub type ProfileEnvelope = Envelope<Profile>;
pub type FundamentalsEnvelope = Envelope<Fundamentals>;
pub type CompareEnvelope = Envelope<Vec<Fundamentals>>;
pub type FinancialsEnvelope = Envelope<Financials>;
pub type EarningsEnvelope = Envelope<Earnings>;
pub type TechnicalEnvelope = Envelope<Technical>;

/// Error shape written to stderr in JSON mode.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ErrorBody {
    /// `usage` | `config` | `network` | `provider` | `not_found` | `offline_miss` | `io` | `internal`
    pub code: String,
    pub message: String,
}

impl From<&IdxError> for ErrorEnvelope {
    fn from(e: &IdxError) -> Self {
        ErrorEnvelope {
            error: ErrorBody {
                code: e.code().to_string(),
                message: e.to_string(),
            },
            exit_code: e.exit_code(),
        }
    }
}

pub fn to_pretty<T: Serialize>(v: &T) -> Result<String> {
    let mut s = serde_json::to_string_pretty(v)?;
    s.push('\n');
    Ok(s)
}

/// Names accepted by `idx schema <name>` and their JSON Schema documents.
pub fn schema_names() -> &'static [&'static str] {
    &[
        "quotes",
        "history",
        "profile",
        "fundamentals",
        "compare",
        "financials",
        "earnings",
        "technical",
        "error",
        "quote",
        "candle",
    ]
}

pub fn schema_for(name: &str) -> Result<serde_json::Value> {
    let root = match name {
        "quotes" => schemars::schema_for!(QuotesEnvelope),
        "history" => schemars::schema_for!(HistoryEnvelope),
        "profile" => schemars::schema_for!(ProfileEnvelope),
        "fundamentals" => schemars::schema_for!(FundamentalsEnvelope),
        "compare" => schemars::schema_for!(CompareEnvelope),
        "financials" => schemars::schema_for!(FinancialsEnvelope),
        "earnings" => schemars::schema_for!(EarningsEnvelope),
        "technical" => schemars::schema_for!(TechnicalEnvelope),
        "error" => schemars::schema_for!(ErrorEnvelope),
        "quote" => schemars::schema_for!(Quote),
        "candle" => schemars::schema_for!(crate::model::Candle),
        _ => {
            return Err(IdxError::Usage(format!(
                "unknown schema {name:?}; available: {}",
                schema_names().join(", ")
            )))
        }
    };
    Ok(serde_json::to_value(root)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Symbol;

    #[test]
    fn envelope_shape() {
        let s = Sourced::fresh(
            vec![Quote::empty(Symbol::parse("BBCA").unwrap(), "t")],
            "yahoo",
        );
        let env = Envelope::from_sourced(SCHEMA_QUOTES, s);
        let v: serde_json::Value = serde_json::from_str(&to_pretty(&env).unwrap()).unwrap();
        assert_eq!(v["schema"], "idx.quotes.v1");
        assert_eq!(v["provider"], "yahoo");
        assert_eq!(v["cached"], false);
        assert!(v.get("warnings").is_none(), "empty warnings are omitted");
        assert_eq!(v["data"][0]["symbol"], "BBCA");
        assert!(v["data"][0]["price"].is_null());
    }

    #[test]
    fn schemas_exist_for_every_name() {
        for n in schema_names() {
            let s = schema_for(n).unwrap();
            assert!(s.get("$schema").is_some(), "{n}");
        }
        assert!(schema_for("nope").is_err());
    }

    #[test]
    fn error_envelope() {
        let e = IdxError::NotFound("ZZZZ".into());
        let env = ErrorEnvelope::from(&e);
        assert_eq!(env.exit_code, 4);
        assert_eq!(env.error.code, "not_found");
    }
}
