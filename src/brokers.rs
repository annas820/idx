//! IDX exchange members (brokers) by two-letter code, bundled from
//! `data/brokers.csv` (the member list idx.co.id publishes). Used to put a
//! name next to the codes a broker summary reports.

use std::collections::BTreeMap;
use std::sync::OnceLock;

const BUNDLED: &str = include_str!("../data/brokers.csv");

fn parse_csv(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.eq_ignore_ascii_case("code,name"))
        .filter_map(|l| l.split_once(','))
        .map(|(c, n)| (c.trim().to_ascii_uppercase(), n.trim().to_string()))
        .filter(|(c, n)| !c.is_empty() && !n.is_empty())
        .collect()
}

fn directory() -> &'static BTreeMap<String, String> {
    static DIR: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    DIR.get_or_init(|| parse_csv(BUNDLED))
}

/// Member name for a broker code such as `CC`; `None` when the code is not
/// on the bundled list.
pub fn name(code: &str) -> Option<String> {
    directory().get(&code.trim().to_ascii_uppercase()).cloned()
}

/// Every member on the bundled list as `(code, name)`, sorted by code.
pub fn all() -> Vec<(String, String)> {
    directory()
        .iter()
        .map(|(c, n)| (c.clone(), n.clone()))
        .collect()
}

/// Number of members on the bundled list.
pub fn count() -> usize {
    directory().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_directory_names_codes() {
        assert_eq!(count(), 89);
        assert_eq!(name("CC").as_deref(), Some("MANDIRI SEKURITAS"));
        assert_eq!(
            name(" yp ").as_deref(),
            Some("MIRAE ASSET SEKURITAS INDONESIA")
        );
        assert_eq!(name("ZZ"), None);
    }

    #[test]
    fn parser_skips_comments_and_header() {
        let m = parse_csv("# note\ncode,name\nAB, Some Broker \n,orphan\nCD,\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m["AB"], "Some Broker");
    }
}
