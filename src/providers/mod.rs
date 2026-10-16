//! The `Provider` abstraction. Every data source implements this trait and is
//! selected with `--provider`, `IDX_PROVIDER`, or the config file. `auto`
//! builds a [`Chain`] that tries providers in order so one broken source does
//! not take the tool down.
//!
//! Adding a provider: see `AGENTS.md`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{IdxError, Result};
use crate::http::Fetcher;
use crate::model::{History, Interval, Period, Quote, Symbol};

pub mod yahoo;

/// Data plus provenance. Renderers put this in the JSON envelope.
#[derive(Debug, Clone)]
pub struct Sourced<T> {
    pub data: T,
    pub provider: &'static str,
    pub cached: bool,
    pub stale: bool,
    pub fetched_at: DateTime<Utc>,
    pub warnings: Vec<String>,
}

impl<T> Sourced<T> {
    pub fn fresh(data: T, provider: &'static str) -> Self {
        Sourced {
            data,
            provider,
            cached: false,
            stale: false,
            fetched_at: Utc::now(),
            warnings: vec![],
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Sourced<U> {
        Sourced {
            data: f(self.data),
            provider: self.provider,
            cached: self.cached,
            stale: self.stale,
            fetched_at: self.fetched_at,
            warnings: self.warnings,
        }
    }
}

/// Per-endpoint cache lifetimes.
#[derive(Debug, Clone, Copy)]
pub struct Ttls {
    pub quote: Duration,
    pub history: Duration,
}

impl Default for Ttls {
    fn default() -> Self {
        Ttls {
            quote: Duration::from_secs(60),
            history: Duration::from_secs(3600),
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Quotes for one or more symbols. Symbols the source does not know are
    /// reported in `warnings`; if none are known the result is `NotFound`.
    async fn quotes(&self, symbols: &[Symbol]) -> Result<Sourced<Vec<Quote>>>;

    /// OHLCV bars, oldest first.
    async fn history(
        &self,
        symbol: &Symbol,
        period: Period,
        interval: Interval,
    ) -> Result<Sourced<History>>;
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// Try each provider in order; fall back when one fails.
    Auto,
    /// Yahoo Finance (chart + quote endpoints).
    Yahoo,
    /// MSN Money (arrives in a later release).
    Msn,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Auto => "auto",
            ProviderKind::Yahoo => "yahoo",
            ProviderKind::Msn => "msn",
        }
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = IdxError;
    fn from_str(s: &str) -> Result<Self> {
        <ProviderKind as clap::ValueEnum>::from_str(s, true).map_err(|_| {
            IdxError::Config(format!("unknown provider {s:?}; use auto, yahoo or msn"))
        })
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tries providers in order. Returns the first success. If every provider
/// fails, surfaces a `NotFound` if any provider said so (the symbol is probably
/// wrong), otherwise the first error.
pub struct Chain {
    providers: Vec<Box<dyn Provider>>,
}

impl Chain {
    pub fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        Chain { providers }
    }

    fn pick(errors: Vec<IdxError>) -> IdxError {
        let mut errors = errors;
        if let Some(pos) = errors
            .iter()
            .position(|e| matches!(e, IdxError::NotFound(_)))
        {
            return errors.swap_remove(pos);
        }
        errors
            .into_iter()
            .next()
            .unwrap_or_else(|| IdxError::Other("no providers configured".into()))
    }
}

#[async_trait]
impl Provider for Chain {
    fn name(&self) -> &'static str {
        "auto"
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
        let mut errors = Vec::new();
        for p in &self.providers {
            match p.quotes(symbols).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    tracing::info!(provider = p.name(), error = %e, "provider failed, trying next");
                    errors.push(e);
                }
            }
        }
        Err(Self::pick(errors))
    }

    async fn history(
        &self,
        symbol: &Symbol,
        period: Period,
        interval: Interval,
    ) -> Result<Sourced<History>> {
        let mut errors = Vec::new();
        for p in &self.providers {
            match p.history(symbol, period, interval).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    tracing::info!(provider = p.name(), error = %e, "provider failed, trying next");
                    errors.push(e);
                }
            }
        }
        Err(Self::pick(errors))
    }
}

/// Builds the provider for `kind`.
pub fn build(kind: ProviderKind, fetcher: Arc<Fetcher>, ttls: Ttls) -> Result<Box<dyn Provider>> {
    match kind {
        ProviderKind::Yahoo => Ok(Box::new(yahoo::Yahoo::new(fetcher, ttls))),
        ProviderKind::Msn => Err(IdxError::Config(
            "provider \"msn\" is not available in this release yet; use --provider yahoo or auto"
                .into(),
        )),
        ProviderKind::Auto => Ok(Box::new(Chain::new(vec![Box::new(yahoo::Yahoo::new(
            fetcher, ttls,
        ))]))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Failing(&'static str, fn() -> IdxError);
    struct Ok1;

    #[async_trait]
    impl Provider for Failing {
        fn name(&self) -> &'static str {
            self.0
        }
        async fn quotes(&self, _: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
            Err((self.1)())
        }
        async fn history(&self, _: &Symbol, _: Period, _: Interval) -> Result<Sourced<History>> {
            Err((self.1)())
        }
    }

    #[async_trait]
    impl Provider for Ok1 {
        fn name(&self) -> &'static str {
            "ok"
        }
        async fn quotes(&self, s: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
            Ok(Sourced::fresh(vec![Quote::empty(s[0].clone(), "ok")], "ok"))
        }
        async fn history(&self, _: &Symbol, _: Period, _: Interval) -> Result<Sourced<History>> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn chain_falls_through_to_working_provider() {
        let chain = Chain::new(vec![
            Box::new(Failing("a", || IdxError::Network("down".into()))),
            Box::new(Ok1),
        ]);
        let got = chain
            .quotes(&[Symbol::parse("BBCA").unwrap()])
            .await
            .unwrap();
        assert_eq!(got.provider, "ok");
    }

    #[tokio::test]
    async fn chain_prefers_not_found_over_network_errors() {
        let chain = Chain::new(vec![
            Box::new(Failing("a", || IdxError::Network("down".into()))),
            Box::new(Failing("b", || IdxError::NotFound("ZZZZ".into()))),
        ]);
        let err = chain
            .quotes(&[Symbol::parse("ZZZZ").unwrap()])
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn provider_kind_parses() {
        assert_eq!(
            "Yahoo".parse::<ProviderKind>().unwrap(),
            ProviderKind::Yahoo
        );
        assert_eq!("auto".parse::<ProviderKind>().unwrap(), ProviderKind::Auto);
        assert!("x".parse::<ProviderKind>().is_err());
    }
}
