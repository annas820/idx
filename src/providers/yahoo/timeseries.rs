//! Parser for `ws/fundamentals-timeseries/v1/finance/timeseries/{SYM}.JK`.
//! The response is one series per requested type; each series is a list of
//! `{asOfDate, periodType, currencyCode, reportedValue: {raw}}`. We pivot it
//! into one `FinancialPeriod` per (period kind, end date).
//!
//! Fixtures: `tests/fixtures/yahoo/timeseries_*.json`.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde_json::Value;

use crate::error::{IdxError, Result};
use crate::model::{FinancialPeriod, Financials, StatementPeriod, Symbol};

use super::NAME;

pub const SOURCE: &str = "yahoo:timeseries";

/// Statement items requested for annual and quarterly series.
pub const ITEMS: &[&str] = &[
    "TotalRevenue",
    "GrossProfit",
    "OperatingIncome",
    "EBITDA",
    "EBIT",
    "InterestIncome",
    "InterestExpense",
    "NetInterestIncome",
    "PretaxIncome",
    "TaxProvision",
    "NetIncome",
    "NetIncomeCommonStockholders",
    "BasicEPS",
    "DilutedEPS",
    "TotalAssets",
    "TotalLiabilitiesNetMinorityInterest",
    "StockholdersEquity",
    "CashAndCashEquivalents",
    "TotalDebt",
    "NetDebt",
    "OrdinarySharesNumber",
    "OperatingCashFlow",
    "InvestingCashFlow",
    "FinancingCashFlow",
    "CapitalExpenditure",
    "FreeCashFlow",
    "CashDividendsPaid",
];

/// Items that Yahoo also publishes as trailing twelve months.
pub const TTM_ITEMS: &[&str] = &[
    "TotalRevenue",
    "GrossProfit",
    "OperatingIncome",
    "EBITDA",
    "EBIT",
    "PretaxIncome",
    "TaxProvision",
    "NetIncome",
    "NetIncomeCommonStockholders",
    "BasicEPS",
    "DilutedEPS",
    "OperatingCashFlow",
    "InvestingCashFlow",
    "FinancingCashFlow",
    "CapitalExpenditure",
    "FreeCashFlow",
    "CashDividendsPaid",
];

/// The `type=` query value for one period kind.
pub fn types_param(period: StatementPeriod) -> String {
    let (prefix, items) = match period {
        StatementPeriod::Annual => ("annual", ITEMS),
        StatementPeriod::Quarterly => ("quarterly", ITEMS),
        StatementPeriod::Ttm => ("trailing", TTM_ITEMS),
    };
    items
        .iter()
        .map(|i| format!("{prefix}{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn kind_of(type_name: &str) -> Option<(StatementPeriod, &str)> {
    if let Some(rest) = type_name.strip_prefix("annual") {
        Some((StatementPeriod::Annual, rest))
    } else if let Some(rest) = type_name.strip_prefix("quarterly") {
        Some((StatementPeriod::Quarterly, rest))
    } else {
        type_name
            .strip_prefix("trailing")
            .map(|rest| (StatementPeriod::Ttm, rest))
    }
}

fn assign(p: &mut FinancialPeriod, item: &str, v: f64) {
    let i = &mut p.income;
    let b = &mut p.balance;
    let c = &mut p.cash_flow;
    match item {
        "TotalRevenue" => i.total_revenue = Some(v),
        "GrossProfit" => i.gross_profit = Some(v),
        "OperatingIncome" => i.operating_income = Some(v),
        "EBITDA" => i.ebitda = Some(v),
        "EBIT" => i.ebit = Some(v),
        "InterestIncome" => i.interest_income = Some(v),
        "InterestExpense" => i.interest_expense = Some(v),
        "NetInterestIncome" => i.net_interest_income = Some(v),
        "PretaxIncome" => i.pretax_income = Some(v),
        "TaxProvision" => i.tax_provision = Some(v),
        "NetIncome" => i.net_income = Some(v),
        "NetIncomeCommonStockholders" => i.net_income_common = Some(v),
        "BasicEPS" => i.eps_basic = Some(v),
        "DilutedEPS" => i.eps_diluted = Some(v),
        "TotalAssets" => b.total_assets = Some(v),
        "TotalLiabilitiesNetMinorityInterest" => b.total_liabilities = Some(v),
        "StockholdersEquity" => b.stockholders_equity = Some(v),
        "CashAndCashEquivalents" => b.cash_and_equivalents = Some(v),
        "TotalDebt" => b.total_debt = Some(v),
        "NetDebt" => b.net_debt = Some(v),
        "OrdinarySharesNumber" => b.shares_outstanding = Some(v),
        "OperatingCashFlow" => c.operating = Some(v),
        "InvestingCashFlow" => c.investing = Some(v),
        "FinancingCashFlow" => c.financing = Some(v),
        "CapitalExpenditure" => c.capital_expenditure = Some(v),
        "FreeCashFlow" => c.free_cash_flow = Some(v),
        "CashDividendsPaid" => c.dividends_paid = Some(v),
        _ => {}
    }
}

/// Pivots the body into periods of the requested kind, oldest first.
/// A symbol Yahoo does not know yields series with no rows, which we report
/// as `NotFound`.
pub fn parse_timeseries(
    body: &str,
    symbol: &Symbol,
    period: StatementPeriod,
) -> Result<Financials> {
    let v: Value = serde_json::from_str(body)?;
    if let Some(err) = v.pointer("/timeseries/error").filter(|e| !e.is_null()) {
        return Err(IdxError::provider(NAME, err.to_string()));
    }
    let series = v
        .pointer("/timeseries/result")
        .and_then(Value::as_array)
        .ok_or_else(|| IdxError::provider(NAME, "timeseries response has no result"))?;

    let mut periods: BTreeMap<NaiveDate, FinancialPeriod> = BTreeMap::new();
    let mut currency: Option<String> = None;
    for s in series {
        let Some(type_name) = s.pointer("/meta/type/0").and_then(Value::as_str) else {
            continue;
        };
        let Some((kind, item)) = kind_of(type_name) else {
            continue;
        };
        if kind != period {
            continue;
        }
        let Some(rows) = s.get(type_name).and_then(Value::as_array) else {
            continue;
        };
        for row in rows.iter().filter(|r| !r.is_null()) {
            let Some(d) = row
                .get("asOfDate")
                .and_then(Value::as_str)
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            else {
                continue;
            };
            let Some(val) = row.pointer("/reportedValue/raw").and_then(Value::as_f64) else {
                continue;
            };
            if currency.is_none() {
                currency = row
                    .get("currencyCode")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            let p = periods
                .entry(d)
                .or_insert_with(|| FinancialPeriod::new(d, period));
            assign(p, item, val);
        }
    }
    if periods.is_empty() {
        return Err(IdxError::NotFound(format!(
            "no {period} financial statements for {symbol}"
        )));
    }
    Ok(Financials {
        symbol: symbol.clone(),
        currency,
        period,
        periods: periods.into_values().collect(),
        source: SOURCE.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BBCA: &str = include_str!("../../../tests/fixtures/yahoo/timeseries_bbca.json");
    const TLKM: &str = include_str!("../../../tests/fixtures/yahoo/timeseries_tlkm.json");
    const NONE: &str = include_str!("../../../tests/fixtures/yahoo/timeseries_not_found.json");

    fn sym(s: &str) -> Symbol {
        Symbol::parse(s).unwrap()
    }

    #[test]
    fn annual_bbca() {
        let f = parse_timeseries(BBCA, &sym("BBCA"), StatementPeriod::Annual).unwrap();
        assert_eq!(f.currency.as_deref(), Some("IDR"));
        assert_eq!(f.periods.len(), 4);
        let last = f.periods.last().unwrap();
        assert_eq!(
            last.end_date,
            NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()
        );
        assert_eq!(last.income.total_revenue, Some(114_319_648_000_000.0));
        assert_eq!(last.income.net_income, Some(57_537_287_000_000.0));
        assert_eq!(last.income.eps_diluted, Some(467.0));
        assert_eq!(last.income.ebitda, None, "banks have no EBITDA series");
        assert_eq!(last.balance.total_assets, Some(1_586_828_536_000_000.0));
        assert_eq!(last.balance.total_debt, Some(2_395_446_000_000.0));
        assert_eq!(last.cash_flow.free_cash_flow, Some(75_057_575_000_000.0));
        assert_eq!(last.cash_flow.dividends_paid, Some(-37_595_047_000_000.0));
        assert_eq!(
            f.periods[0].end_date,
            NaiveDate::from_ymd_opt(2022, 12, 31).unwrap()
        );
    }

    #[test]
    fn quarterly_and_ttm() {
        let q = parse_timeseries(BBCA, &sym("BBCA"), StatementPeriod::Quarterly).unwrap();
        assert_eq!(q.periods.len(), 5);
        assert_eq!(
            q.periods.last().unwrap().end_date,
            NaiveDate::from_ymd_opt(2026, 6, 30).unwrap()
        );
        assert_eq!(
            q.periods.last().unwrap().income.net_income,
            Some(14_850_323_000_000.0)
        );
        let t = parse_timeseries(BBCA, &sym("BBCA"), StatementPeriod::Ttm).unwrap();
        let last = t.periods.last().unwrap();
        assert_eq!(last.income.total_revenue, Some(115_749_506_000_000.0));
        assert_eq!(last.income.eps_diluted, Some(472.0));
        assert_eq!(last.balance.total_assets, None);
    }

    #[test]
    fn tlkm_has_ebitda() {
        let f = parse_timeseries(TLKM, &sym("TLKM"), StatementPeriod::Annual).unwrap();
        let last = f.periods.last().unwrap();
        assert_eq!(last.income.ebitda, Some(73_370_000_000_000.0));
        assert_eq!(last.income.gross_profit, Some(98_490_000_000_000.0));
        assert_eq!(last.balance.net_debt, Some(16_546_000_000_000.0));
    }

    #[test]
    fn unknown_symbol_is_not_found() {
        let err = parse_timeseries(NONE, &sym("ZZZZ"), StatementPeriod::Annual).unwrap_err();
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn types_param_shape() {
        let t = types_param(StatementPeriod::Quarterly);
        assert!(t.starts_with("quarterlyTotalRevenue,"));
        assert!(t.contains("quarterlyCashDividendsPaid"));
        assert!(types_param(StatementPeriod::Ttm).starts_with("trailingTotalRevenue"));
    }
}
