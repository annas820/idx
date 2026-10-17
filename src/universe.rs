//! The list of IDX tickers the screener walks. Seeded from `data/universe.csv`
//! (bundled, from the public KSEI register), extended with any issuer seen in
//! the local ownership database, and replaceable with `IDX_UNIVERSE=<csv>`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{IdxError, Result};
use crate::model::Symbol;

const BUNDLED: &str = include_str!("../data/universe.csv");

#[derive(Debug, Clone, PartialEq)]
pub struct Issuer {
    pub symbol: Symbol,
    pub name: Option<String>,
}

fn parse_csv(text: &str) -> Result<Vec<Issuer>> {
    let mut out = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.eq_ignore_ascii_case("symbol,name") {
            continue;
        }
        let (sym, name) = match line.split_once(',') {
            Some((s, n)) => (s.trim(), n.trim()),
            None => (line, ""),
        };
        let symbol = Symbol::parse(sym)
            .map_err(|e| IdxError::Usage(format!("universe line {}: {e}", i + 1)))?;
        let name = if name.is_empty() {
            None
        } else {
            Some(name.trim_matches('"').to_string())
        };
        out.entry(symbol.clone()).or_insert(Issuer { symbol, name });
    }
    Ok(out.into_values().collect())
}

/// Bundled list, or the file at `IDX_UNIVERSE` / `override_path` when given.
pub fn load(override_path: Option<&Path>) -> Result<Vec<Issuer>> {
    let path = override_path
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("IDX_UNIVERSE").map(Into::into));
    match path {
        Some(p) => {
            let text = std::fs::read_to_string(&p)
                .map_err(|e| IdxError::Usage(format!("universe {}: {e}", p.display())))?;
            let list = parse_csv(&text)?;
            if list.is_empty() {
                return Err(IdxError::Usage(format!(
                    "universe {} has no symbols",
                    p.display()
                )));
            }
            Ok(list)
        }
        None => parse_csv(BUNDLED),
    }
}

/// Adds issuers from the ownership database when it exists; never fails.
pub fn extend_from_ownership(list: &mut Vec<Issuer>, db_path: &Path) {
    if !db_path.exists() {
        return;
    }
    let Ok(db) = crate::ownership::OwnershipDb::open(db_path) else {
        return;
    };
    let Ok(extra) = db.issuers() else {
        return;
    };
    for (code, name) in extra {
        if let Ok(symbol) = Symbol::parse(&code) {
            if !list.iter().any(|i| i.symbol == symbol) {
                list.push(Issuer { symbol, name });
            }
        }
    }
    list.sort_by(|a, b| a.symbol.cmp(&b.symbol));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_universe_is_large_and_clean() {
        let u = load(None).unwrap();
        assert!(u.len() > 900, "{}", u.len());
        assert!(u.iter().any(|i| i.symbol.as_str() == "BBCA"));
        let bbca = u.iter().find(|i| i.symbol.as_str() == "BBCA").unwrap();
        assert!(bbca.name.as_deref().unwrap().contains("BANK CENTRAL ASIA"));
        assert!(
            u.windows(2).all(|w| w[0].symbol < w[1].symbol),
            "sorted, unique"
        );
    }

    #[test]
    fn custom_csv() {
        let list = parse_csv("# comment\nsymbol,name\nbbri,Bank Rakyat\nTLKM\nBBRI,dup\n").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].symbol.as_str(), "BBRI");
        assert_eq!(list[0].name.as_deref(), Some("Bank Rakyat"));
        assert_eq!(list[1].name, None);
        assert!(parse_csv("BB$CA\n").is_err());
    }
}
