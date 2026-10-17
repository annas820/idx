//! Pure parsers for IPOT's broker summary fragment. Tested against the
//! recorded pages in `tests/fixtures/ipot/`.
//!
//! The fragment is a 9-column table: buyer code, lots, value, average,
//! rank, seller code, lots, value, average. Each code sits in a
//! `<span class="text-local|text-foreign|text-bumn">`; the footer holds
//! `T. Val`, `F. NVal`, `T.Lot` and `Avg`.

use chrono::NaiveDate;

use crate::error::{IdxError, Result};
use crate::model::{BrokerFlow, BrokerKind};

use super::NAME;

/// What one page said.
#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    /// `Invalid stock code!`: the source does not know the ticker.
    Invalid,
    Summary(Parsed),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    /// Range echoed back in the page's date inputs.
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub buyers: Vec<BrokerFlow>,
    pub sellers: Vec<BrokerFlow>,
    pub total_value: Option<f64>,
    pub total_lots: Option<f64>,
    pub foreign_net_value: Option<f64>,
    pub average_price: Option<f64>,
    /// At least one figure was abbreviated (`1.6 M`).
    pub rounded: bool,
}

/// `1.6 M` -> `(1_600_000, true)`, `687,067` -> `(687_067, false)`,
/// `-2.3 T` -> `(-2.3e12, true)`; empty or unparsable -> `None`.
pub fn number(text: &str) -> Option<(f64, bool)> {
    let t = text.trim().replace(',', "");
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
    let v = num.trim().parse::<f64>().ok()?;
    // Round away float noise from the scaling (1.6 * 1e6).
    Some(((v * mult).round(), mult != 1.0))
}

#[derive(Debug)]
struct Cell {
    class: Option<String>,
    text: String,
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").trim().to_string()
}

fn cells(row: &str) -> Vec<Cell> {
    let mut out = Vec::new();
    let mut rest = row;
    while let Some(start) = rest.find("<td") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let inner_start = open_end + 1;
        let Some(close) = after[inner_start..].find("</td>") else {
            break;
        };
        let inner = &after[inner_start..inner_start + close];
        let class = inner.find("class=\"").and_then(|i| {
            let v = &inner[i + 7..];
            v.find('"').map(|j| v[..j].to_string())
        });
        out.push(Cell {
            class,
            text: strip_tags(inner),
        });
        rest = &after[inner_start + close + 5..];
    }
    out
}

fn kind(class: Option<&str>) -> Option<BrokerKind> {
    match class?.strip_prefix("text-")? {
        "local" => Some(BrokerKind::Local),
        "foreign" => Some(BrokerKind::Foreign),
        "bumn" => Some(BrokerKind::StateOwned),
        _ => None,
    }
}

/// `MM/DD/YYYY` from the value of the input with this id.
fn input_date(html: &str, id: &str) -> Option<NaiveDate> {
    let at = html.find(&format!("id=\"{id}\""))?;
    let tag_end = at + html[at..].find('>')?;
    let tag = &html[..tag_end];
    let tag = &tag[tag.rfind('<')?..];
    let v = &tag[tag.find("value=\"")? + 7..];
    NaiveDate::parse_from_str(&v[..v.find('"')?], "%m/%d/%Y").ok()
}

/// Text after `label` up to the next tag, e.g. `T. Val : 6.6 T`.
fn footer(html: &str, label: &str) -> Option<String> {
    let at = html.find(label)? + label.len();
    let rest = &html[at..];
    Some(rest[..rest.find('<')?].trim().to_string())
}

pub fn parse(html: &str) -> Result<Page> {
    if html
        .trim()
        .to_ascii_lowercase()
        .contains("invalid stock code")
    {
        return Ok(Page::Invalid);
    }
    let (Some(body_start), Some(body_end)) = (html.find("<tbody>"), html.find("</tbody>")) else {
        return Err(IdxError::provider(
            NAME,
            "unexpected broker summary page (no table)",
        ));
    };
    let mut p = Parsed {
        from: input_date(html, "startDateBrokerSummary"),
        to: input_date(html, "endDateBrokerSummary"),
        ..Parsed::default()
    };
    let mut rounded = false;
    let mut num = |c: &Cell| {
        number(&c.text).map(|(v, r)| {
            rounded |= r;
            v
        })
    };
    for (i, row) in html[body_start..body_end].split("</tr>").enumerate() {
        let c = cells(row);
        if c.len() < 9 {
            continue;
        }
        let rank = c[4].text.parse::<u32>().unwrap_or(i as u32 + 1);
        for (side, at) in [(&mut p.buyers, 0), (&mut p.sellers, 5)] {
            let code = c[at].text.to_ascii_uppercase();
            if code.is_empty() {
                continue;
            }
            side.push(BrokerFlow {
                rank,
                code,
                name: None,
                kind: kind(c[at].class.as_deref()),
                lots: num(&c[at + 1]),
                value: num(&c[at + 2]),
                average_price: num(&c[at + 3]),
            });
        }
    }
    let mut total = |label: &str| {
        footer(html, label).and_then(|t| {
            number(&t).map(|(v, r)| {
                rounded |= r;
                v
            })
        })
    };
    p.total_value = total("T. Val :");
    p.foreign_net_value = total("F. NVal :");
    p.total_lots = total("T.Lot :");
    // The page prints `Avg : 0` when nothing traded; there is no average then.
    p.average_price = total("Avg :").filter(|_| p.total_lots.is_some_and(|l| l > 0.0));
    p.rounded = rounded;
    Ok(Page::Summary(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str = include_str!("../../../tests/fixtures/ipot/brokersummary_bbca_20261016.html");
    const RANGE: &str =
        include_str!("../../../tests/fixtures/ipot/brokersummary_bbca_20261012_20261016.html");
    const EMPTY: &str =
        include_str!("../../../tests/fixtures/ipot/brokersummary_bbca_20261017_no_trades.html");
    const INVALID: &str =
        include_str!("../../../tests/fixtures/ipot/brokersummary_zzzz_invalid.html");

    fn summary(html: &str) -> Parsed {
        match parse(html).unwrap() {
            Page::Summary(p) => p,
            Page::Invalid => panic!("expected a summary"),
        }
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn numbers_as_published() {
        assert_eq!(number("1.6 M"), Some((1_600_000.0, true)));
        assert_eq!(number("841.2 B"), Some((841_200_000_000.0, true)));
        assert_eq!(number("-2.3 T"), Some((-2_300_000_000_000.0, true)));
        assert_eq!(number("687,067 "), Some((687_067.0, false)));
        assert_eq!(number("5,191 "), Some((5_191.0, false)));
        assert_eq!(number("0"), Some((0.0, false)));
        assert_eq!(number(" "), None);
        assert_eq!(number("n/a"), None);
    }

    #[test]
    fn parses_one_day() {
        let p = summary(DAY);
        assert_eq!(
            (p.from, p.to),
            (Some(d(2026, 10, 16)), Some(d(2026, 10, 16)))
        );
        assert_eq!((p.buyers.len(), p.sellers.len()), (10, 10));

        let b = &p.buyers[0];
        assert_eq!(
            (b.rank, b.code.as_str(), b.kind),
            (1, "MG", Some(BrokerKind::Local))
        );
        assert_eq!(b.lots, Some(1_600_000.0));
        assert_eq!(b.value, Some(841_200_000_000.0));
        assert_eq!(b.average_price, Some(5_191.0));
        assert_eq!(b.name, None, "names are the service's job");

        let s = &p.sellers[0];
        assert_eq!(
            (s.code.as_str(), s.kind),
            ("CC", Some(BrokerKind::StateOwned))
        );
        assert_eq!(s.value, Some(2_200_000_000_000.0));
        assert_eq!(p.buyers[3].kind, Some(BrokerKind::Foreign));
        assert_eq!(p.sellers[3].code, "BB");
        assert_eq!(p.sellers[3].lots, Some(687_067.0), "small lots are exact");
        assert_eq!(p.sellers[9].rank, 10);

        assert_eq!(p.total_value, Some(6_600_000_000_000.0));
        assert_eq!(p.foreign_net_value, Some(-2_300_000_000_000.0));
        assert_eq!(p.total_lots, Some(12_600_000.0));
        assert_eq!(p.average_price, Some(5_228.0));
        assert!(p.rounded);
    }

    #[test]
    fn parses_a_range() {
        let p = summary(RANGE);
        assert_eq!(
            (p.from, p.to),
            (Some(d(2026, 10, 12)), Some(d(2026, 10, 16)))
        );
        assert_eq!(p.total_value, Some(9_900_000_000_000.0));
        assert_eq!(p.average_price, Some(5_471.0));
        assert_eq!(p.buyers.len(), 10);
    }

    #[test]
    fn no_trades_is_an_empty_summary() {
        let p = summary(EMPTY);
        assert!(p.buyers.is_empty() && p.sellers.is_empty());
        assert_eq!(p.total_value, Some(0.0));
        assert_eq!(p.total_lots, Some(0.0));
        assert_eq!(p.average_price, None, "Avg : 0 means no average");
        assert!(!p.rounded);
        assert_eq!(p.to, Some(d(2026, 10, 17)));
    }

    #[test]
    fn invalid_code_and_garbage() {
        assert_eq!(parse(INVALID).unwrap(), Page::Invalid);
        let err = parse("<html>maintenance</html>").unwrap_err();
        assert_eq!(err.exit_code(), 3);
    }
}
