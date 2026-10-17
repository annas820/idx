//! Operations shared by the CLI and the MCP server: anything that combines a
//! provider call with local analysis lives here so both front ends behave
//! identically.

use futures::future::join_all;

use crate::analysis;
use crate::error::{IdxError, Result};
use crate::model::{Fundamentals, Interval, Period, Symbol, Technical};
use crate::providers::{Provider, Sourced};

/// Ratios plus the rule-based verdict.
pub async fn fundamentals(p: &dyn Provider, symbol: &Symbol) -> Result<Sourced<Fundamentals>> {
    let mut result = p.fundamentals(symbol).await?;
    result.data.verdict = Some(analysis::fundamental::verdict(&result.data));
    Ok(result)
}

/// Daily candles over `period`, run through the indicator set.
pub async fn technical(
    p: &dyn Provider,
    symbol: &Symbol,
    period: Period,
) -> Result<Sourced<Technical>> {
    let hist = p.history(symbol, period, Interval::Day1).await?;
    let Some(report) = analysis::technical::analyse(&hist.data) else {
        return Err(IdxError::NotFound(format!("no daily candles for {symbol}")));
    };
    Ok(Sourced {
        data: report,
        provider: hist.provider,
        cached: hist.cached,
        stale: hist.stale,
        fetched_at: hist.fetched_at,
        warnings: hist.warnings,
    })
}

/// Fundamentals for several symbols, fetched concurrently. Symbols that fail
/// become warnings; the call fails only when none succeed.
pub async fn compare(p: &dyn Provider, symbols: &[Symbol]) -> Result<Sourced<Vec<Fundamentals>>> {
    if symbols.len() < 2 {
        return Err(IdxError::Usage(
            "compare needs at least two different symbols".into(),
        ));
    }
    let results = join_all(symbols.iter().map(|s| fundamentals(p, s))).await;

    let mut out: Option<Sourced<Vec<Fundamentals>>> = None;
    let mut pending_warnings: Vec<String> = Vec::new();
    let mut first_err: Option<IdxError> = None;
    for (sym, r) in symbols.iter().zip(results) {
        match r {
            Ok(s) => match &mut out {
                None => out = Some(s.map(|d| vec![d])),
                Some(acc) => {
                    acc.absorb(&s);
                    acc.data.push(s.data);
                }
            },
            Err(e) => {
                let msg = match &e {
                    IdxError::NotFound(_) => format!("no data for {sym}"),
                    other => format!("{sym}: {other}"),
                };
                pending_warnings.push(msg);
                first_err.get_or_insert(e);
            }
        }
    }
    let Some(mut result) = out else {
        return Err(first_err.unwrap_or_else(|| IdxError::NotFound("no data".into())));
    };
    result.warnings.extend(pending_warnings);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{History, Quote};
    use async_trait::async_trait;

    struct Fake;

    #[async_trait]
    impl Provider for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn quotes(&self, _: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
            unimplemented!()
        }
        async fn history(&self, _: &Symbol, _: Period, _: Interval) -> Result<Sourced<History>> {
            unimplemented!()
        }
        async fn fundamentals(&self, s: &Symbol) -> Result<Sourced<Fundamentals>> {
            if s.as_str().starts_with("ZZZZ") {
                return Err(IdxError::NotFound(s.to_string()));
            }
            let mut f = Fundamentals::empty(s.clone(), "fake");
            f.valuation.pe_trailing = Some(8.0);
            f.valuation.price_to_book = Some(0.9);
            f.profitability.roe_pct = Some(20.0);
            Ok(Sourced::fresh(f, "fake"))
        }
    }

    fn syms(list: &[&str]) -> Vec<Symbol> {
        list.iter().map(|s| Symbol::parse(s).unwrap()).collect()
    }

    #[tokio::test]
    async fn fundamentals_carry_a_verdict() {
        let f = fundamentals(&Fake, &Symbol::parse("BBCA").unwrap())
            .await
            .unwrap();
        assert!(f.data.verdict.is_some());
    }

    #[tokio::test]
    async fn compare_collects_warnings_and_keeps_order() {
        let c = compare(&Fake, &syms(&["ZZZZ", "BBCA", "BBRI"]))
            .await
            .unwrap();
        let got: Vec<&str> = c.data.iter().map(|f| f.symbol.as_str()).collect();
        assert_eq!(got, ["BBCA", "BBRI"]);
        assert_eq!(c.warnings, vec!["no data for ZZZZ".to_string()]);
        assert!(c.data.iter().all(|f| f.verdict.is_some()));

        let err = compare(&Fake, &syms(&["ZZZZ", "ZZZZ2"])).await.unwrap_err();
        assert_eq!(err.exit_code(), 4);
        let err = compare(&Fake, &syms(&["BBCA"])).await.unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}
