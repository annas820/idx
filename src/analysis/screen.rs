//! Screener filter language and evaluation.
//!
//! `--filter "pe<15,roe>15,sector=Banking,mcap>10T"`: comma-separated
//! clauses, each `key op value`. Numeric values accept `K`, `M`, `B`, `T`
//! suffixes and a trailing `%`. String keys (`sector`, `industry`) match
//! case-insensitively as a substring with `=` / `!=`.

use std::cmp::Ordering;

use crate::error::{IdxError, Result};
use crate::model::{Fundamentals, Quote, ScreenRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub key: Key,
    pub op: Op,
    pub value: Value,
    pub raw: String,
}

/// Every key the screener understands. `needs_fundamentals` marks the ones
/// that require a per-symbol fundamentals call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Pe,
    Fpe,
    Pb,
    Mcap,
    Price,
    Div,
    Eps,
    Volume,
    Chg,
    Roe,
    Roa,
    Margin,
    OpMargin,
    De,
    Growth,
    EpsGrowth,
    Peg,
    Ps,
    Sector,
    Industry,
}

impl Key {
    pub fn parse(s: &str) -> Option<Key> {
        Some(match s.to_ascii_lowercase().as_str() {
            "pe" | "p/e" | "per" => Key::Pe,
            "fpe" | "forwardpe" | "fwdpe" => Key::Fpe,
            "pb" | "pbv" | "p/b" => Key::Pb,
            "mcap" | "marketcap" | "cap" => Key::Mcap,
            "price" | "close" => Key::Price,
            "div" | "yield" | "dividend" => Key::Div,
            "eps" => Key::Eps,
            "volume" | "vol" | "avgvolume" => Key::Volume,
            "chg" | "change" => Key::Chg,
            "roe" => Key::Roe,
            "roa" => Key::Roa,
            "margin" | "npm" | "netmargin" => Key::Margin,
            "opmargin" | "opm" => Key::OpMargin,
            "de" | "der" | "debt" => Key::De,
            "growth" | "revgrowth" | "revenuegrowth" => Key::Growth,
            "epsgrowth" | "earningsgrowth" => Key::EpsGrowth,
            "peg" => Key::Peg,
            "ps" | "p/s" => Key::Ps,
            "sector" => Key::Sector,
            "industry" => Key::Industry,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Key::Pe => "pe",
            Key::Fpe => "fpe",
            Key::Pb => "pb",
            Key::Mcap => "mcap",
            Key::Price => "price",
            Key::Div => "div",
            Key::Eps => "eps",
            Key::Volume => "volume",
            Key::Chg => "chg",
            Key::Roe => "roe",
            Key::Roa => "roa",
            Key::Margin => "margin",
            Key::OpMargin => "opmargin",
            Key::De => "de",
            Key::Growth => "growth",
            Key::EpsGrowth => "epsgrowth",
            Key::Peg => "peg",
            Key::Ps => "ps",
            Key::Sector => "sector",
            Key::Industry => "industry",
        }
    }

    pub fn needs_fundamentals(&self) -> bool {
        matches!(
            self,
            Key::Roe
                | Key::Roa
                | Key::Margin
                | Key::OpMargin
                | Key::De
                | Key::Growth
                | Key::EpsGrowth
                | Key::Peg
                | Key::Ps
                | Key::Sector
                | Key::Industry
        )
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Key::Sector | Key::Industry)
    }

    pub const ALL: &'static str = "pe, fpe, pb, ps, peg, mcap, price, div, eps, volume, chg, roe, roa, margin, opmargin, de, growth, epsgrowth, sector, industry";
}

/// `10T`, `2.5B`, `500M`, `15%`, `0.8`.
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s
        .trim()
        .trim_end_matches('%')
        .trim()
        .replace(['_', ','], "");
    if t.is_empty() {
        return None;
    }
    let (num, mult) = match t.chars().last()?.to_ascii_uppercase() {
        'K' => (&t[..t.len() - 1], 1e3),
        'M' => (&t[..t.len() - 1], 1e6),
        'B' => (&t[..t.len() - 1], 1e9),
        'T' => (&t[..t.len() - 1], 1e12),
        _ => (t.as_str(), 1.0),
    };
    num.trim().parse::<f64>().ok().map(|v| v * mult)
}

/// Splits one clause into its key, operator and value text: `"pe<=15"` ->
/// `("pe", Le, "15")`. The leftmost operator wins, the longer one on a tie.
pub fn split_clause(clause: &str) -> Result<(&str, Op, &str)> {
    let ops = [
        ("<=", Op::Le),
        (">=", Op::Ge),
        ("!=", Op::Ne),
        ("<", Op::Lt),
        (">", Op::Gt),
        ("=", Op::Eq),
    ];
    let Some((pos, (sym, op))) = ops
        .iter()
        .filter_map(|(s, o)| clause.find(s).map(|p| (p, (*s, *o))))
        .min_by_key(|(p, (s, _))| (*p, std::cmp::Reverse(s.len())))
    else {
        return Err(IdxError::Usage(format!(
            "filter {clause:?} needs an operator (<, <=, >, >=, =, !=)"
        )));
    };
    Ok((clause[..pos].trim(), op, clause[pos + sym.len()..].trim()))
}

/// `v op target` for numbers; equality within 1e-9.
pub fn compare(v: f64, op: Op, target: f64) -> bool {
    match op {
        Op::Lt => v < target,
        Op::Le => v <= target,
        Op::Gt => v > target,
        Op::Ge => v >= target,
        Op::Eq => (v - target).abs() < 1e-9,
        Op::Ne => (v - target).abs() >= 1e-9,
    }
}

/// Parses `"pe<15, roe>=15, sector=bank"` into filters.
pub fn parse_filters(spec: &str) -> Result<Vec<Filter>> {
    let mut out = Vec::new();
    for clause in spec.split([',', ';']).map(str::trim) {
        if clause.is_empty() {
            continue;
        }
        let (key_s, op, val_s) = split_clause(clause)?;
        let Some(key) = Key::parse(key_s) else {
            return Err(IdxError::Usage(format!(
                "unknown filter key {key_s:?}; use one of {}",
                Key::ALL
            )));
        };
        let value = if key.is_text() {
            if !matches!(op, Op::Eq | Op::Ne) {
                return Err(IdxError::Usage(format!(
                    "{} only supports = and !=",
                    key.name()
                )));
            }
            if val_s.is_empty() {
                return Err(IdxError::Usage(format!("{} needs a value", key.name())));
            }
            Value::Text(val_s.trim_matches('"').to_string())
        } else {
            match parse_number(val_s) {
                Some(n) => Value::Num(n),
                None => {
                    return Err(IdxError::Usage(format!(
                        "filter {clause:?}: {val_s:?} is not a number (try 15, 2.5B, 10T, 3%)"
                    )))
                }
            }
        };
        out.push(Filter {
            key,
            op,
            value,
            raw: clause.to_string(),
        });
    }
    Ok(out)
}

pub fn numeric(row: &ScreenRow, key: Key) -> Option<f64> {
    match key {
        Key::Pe => row.pe_trailing,
        Key::Fpe => row.pe_forward,
        Key::Pb => row.price_to_book,
        Key::Mcap => row.market_cap,
        Key::Price => row.price,
        Key::Div => row.dividend_yield_pct,
        Key::Eps => row.eps_ttm,
        Key::Volume => row.avg_volume_3m.map(|v| v as f64),
        Key::Chg => row.change_pct,
        Key::Roe => row.roe_pct,
        Key::Roa => row.roa_pct,
        Key::Margin => row.net_margin_pct,
        Key::OpMargin => row.operating_margin_pct,
        Key::De => row.debt_to_equity,
        Key::Growth => row.revenue_growth_pct,
        Key::EpsGrowth => row.earnings_growth_pct,
        Key::Peg => row.peg,
        Key::Ps => row.price_to_sales,
        Key::Sector | Key::Industry => None,
    }
}

fn text(row: &ScreenRow, key: Key) -> Option<&str> {
    match key {
        Key::Sector => row.sector.as_deref(),
        Key::Industry => row.industry.as_deref(),
        _ => None,
    }
}

/// A row passes when every filter holds. Missing values never match.
pub fn matches(row: &ScreenRow, filters: &[Filter]) -> bool {
    filters.iter().all(|f| match &f.value {
        Value::Num(target) => match numeric(row, f.key) {
            None => false,
            Some(v) => compare(v, f.op, *target),
        },
        Value::Text(needle) => match text(row, f.key) {
            None => false,
            Some(hay) => {
                let hit = hay.to_lowercase().contains(&needle.to_lowercase());
                if f.op == Op::Ne {
                    !hit
                } else {
                    hit
                }
            }
        },
    })
}

/// Sorts by `key`; rows without a value go last whatever the direction.
pub fn sort_rows(rows: &mut [ScreenRow], key: Key, descending: bool) {
    rows.sort_by(|a, b| {
        let (x, y) = (numeric(a, key), numeric(b, key));
        match (x, y) {
            (None, None) => a.symbol.cmp(&b.symbol),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(x), Some(y)) => {
                let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                if descending {
                    o.reverse()
                } else {
                    o
                }
            }
        }
    });
}

pub fn row_from_quote(q: &Quote, name_hint: Option<&str>) -> ScreenRow {
    ScreenRow {
        symbol: q.symbol.clone(),
        name: q.name.clone().or_else(|| name_hint.map(str::to_string)),
        price: q.price,
        change_pct: q.change_pct,
        market_cap: q.market_cap,
        pe_trailing: q.pe_trailing,
        pe_forward: q.pe_forward,
        price_to_book: q.price_to_book,
        dividend_yield_pct: q.dividend_yield_pct,
        eps_ttm: q.eps_ttm,
        avg_volume_3m: q.avg_volume_3m,
        week52_high: q.week52_high,
        week52_low: q.week52_low,
        fundamentals_checked: false,
        sector: None,
        industry: None,
        roe_pct: None,
        roa_pct: None,
        net_margin_pct: None,
        operating_margin_pct: None,
        debt_to_equity: None,
        revenue_growth_pct: None,
        earnings_growth_pct: None,
        peg: None,
        price_to_sales: None,
    }
}

pub fn apply_fundamentals(row: &mut ScreenRow, f: &Fundamentals) {
    row.fundamentals_checked = true;
    row.sector = f.sector.clone();
    row.industry = f.industry.clone();
    row.roe_pct = f.profitability.roe_pct;
    row.roa_pct = f.profitability.roa_pct;
    row.net_margin_pct = f.profitability.net_margin_pct;
    row.operating_margin_pct = f.profitability.operating_margin_pct;
    row.debt_to_equity = f.health.debt_to_equity;
    row.revenue_growth_pct = f.growth.revenue_growth_pct;
    row.earnings_growth_pct = f.growth.earnings_growth_pct;
    row.peg = f.valuation.peg;
    row.price_to_sales = f.valuation.price_to_sales;
    if row.pe_trailing.is_none() {
        row.pe_trailing = f.valuation.pe_trailing;
    }
    if row.price_to_book.is_none() {
        row.price_to_book = f.valuation.price_to_book;
    }
    if row.dividend_yield_pct.is_none() {
        row.dividend_yield_pct = f.dividend.yield_pct;
    }
    if row.name.is_none() {
        row.name = f.name.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Symbol;

    fn row(sym: &str, pe: Option<f64>, mcap: Option<f64>) -> ScreenRow {
        let mut q = Quote::empty(Symbol::parse(sym).unwrap(), "t");
        q.pe_trailing = pe;
        q.market_cap = mcap;
        row_from_quote(&q, None)
    }

    #[test]
    fn parses_clauses_and_suffixes() {
        let f = parse_filters("pe<15, roe>=15,sector=Bank, mcap>10T, div>3%").unwrap();
        assert_eq!(f.len(), 5);
        assert_eq!(f[0].key, Key::Pe);
        assert_eq!(f[0].op, Op::Lt);
        assert_eq!(f[1].op, Op::Ge);
        assert_eq!(f[2].value, Value::Text("Bank".into()));
        assert_eq!(f[3].value, Value::Num(10e12));
        assert_eq!(f[4].value, Value::Num(3.0));
        assert!(f.iter().any(|x| x.key.needs_fundamentals()));
        assert_eq!(parse_number("2.5B"), Some(2.5e9));
        assert_eq!(parse_number("1,500"), Some(1500.0));
    }

    #[test]
    fn rejects_bad_clauses() {
        assert_eq!(parse_filters("pe 15").unwrap_err().exit_code(), 2);
        assert_eq!(parse_filters("nope<1").unwrap_err().exit_code(), 2);
        assert_eq!(parse_filters("pe<abc").unwrap_err().exit_code(), 2);
        assert_eq!(parse_filters("sector>1").unwrap_err().exit_code(), 2);
    }

    #[test]
    fn matching_and_sorting() {
        let f = parse_filters("pe<15,mcap>1T").unwrap();
        assert!(matches(&row("A", Some(10.0), Some(2e12)), &f));
        assert!(!matches(&row("B", Some(20.0), Some(2e12)), &f));
        assert!(
            !matches(&row("C", None, Some(2e12)), &f),
            "missing never matches"
        );
        let mut r = row("D", Some(5.0), Some(2e12));
        r.sector = Some("Financial Services".into());
        assert!(matches(&r, &parse_filters("sector=financial").unwrap()));
        assert!(!matches(&r, &parse_filters("sector!=financial").unwrap()));

        let mut rows = vec![
            row("X", Some(12.0), None),
            row("Y", Some(8.0), Some(1.0)),
            row("Z", None, Some(5.0)),
        ];
        sort_rows(&mut rows, Key::Pe, false);
        let order: Vec<&str> = rows.iter().map(|r| r.symbol.as_str()).collect();
        assert_eq!(order, ["Y", "X", "Z"]);
        sort_rows(&mut rows, Key::Mcap, true);
        let order: Vec<&str> = rows.iter().map(|r| r.symbol.as_str()).collect();
        assert_eq!(order, ["Z", "Y", "X"]);
    }
}
