//! Indonesian finance news from public RSS feeds.
//!
//! Feeds verified 2026-10-17 (see `tests/fixtures/news/`):
//! - Kontan: `https://{investasi,keuangan,industri,insight}.kontan.co.id/rss`
//!   (RSS 2.0, CDATA fields, `pubDate` in RFC 2822 with +0700, 25 items)
//! - CNBC Indonesia: `https://www.cnbcindonesia.com/{market,research,news}/rss`
//!   (RSS 2.0, 100 items, description starts with an `<img>` tag)
//! - Bisnis.com answers 403 to non-browser clients and has no reachable feed;
//!   it is listed so `--source bisnis` can be tried, and failures are warnings.
//!
//! Items are fetched through the normal cache (15 min TTL) and parsed with
//! the `rss` crate. Matching a ticker is done by the caller (`service::news`).

use std::sync::Arc;
use std::time::Duration;

use chrono::DateTime;
use futures::future::join_all;

use crate::error::{IdxError, Result};
use crate::http::Fetcher;
use crate::model::{wib, NewsItem};

pub const NAME: &str = "news";
pub const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feed {
    /// `publisher/section`, used as the item source label.
    pub id: &'static str,
    pub url: &'static str,
    /// Fetched by default; others only when asked for by publisher name.
    pub default: bool,
}

pub const FEEDS: &[Feed] = &[
    Feed {
        id: "kontan/investasi",
        url: "https://investasi.kontan.co.id/rss",
        default: true,
    },
    Feed {
        id: "kontan/keuangan",
        url: "https://keuangan.kontan.co.id/rss",
        default: true,
    },
    Feed {
        id: "kontan/industri",
        url: "https://industri.kontan.co.id/rss",
        default: true,
    },
    Feed {
        id: "kontan/insight",
        url: "https://insight.kontan.co.id/rss",
        default: false,
    },
    Feed {
        id: "cnbcindonesia/market",
        url: "https://www.cnbcindonesia.com/market/rss",
        default: true,
    },
    Feed {
        id: "cnbcindonesia/research",
        url: "https://www.cnbcindonesia.com/research/rss",
        default: true,
    },
    Feed {
        id: "cnbcindonesia/news",
        url: "https://www.cnbcindonesia.com/news/rss",
        default: false,
    },
    Feed {
        id: "bisnis/market",
        url: "https://market.bisnis.com/rss",
        default: false,
    },
];

pub fn publishers() -> Vec<&'static str> {
    let mut v: Vec<&str> = FEEDS
        .iter()
        .map(|f| f.id.split('/').next().unwrap_or(f.id))
        .collect();
    v.dedup();
    v
}

/// Feeds selected by publisher names (`kontan`, `cnbcindonesia`, `bisnis`,
/// `all`); empty selects the defaults.
pub fn select(sources: &[String]) -> Result<Vec<Feed>> {
    if sources.is_empty() {
        return Ok(FEEDS.iter().copied().filter(|f| f.default).collect());
    }
    if sources.iter().any(|s| s.eq_ignore_ascii_case("all")) {
        return Ok(FEEDS.to_vec());
    }
    let mut out = Vec::new();
    for s in sources {
        let s = s.to_ascii_lowercase();
        let hits: Vec<Feed> = FEEDS
            .iter()
            .copied()
            .filter(|f| f.id.starts_with(&format!("{s}/")) || f.id == s)
            .collect();
        if hits.is_empty() {
            return Err(IdxError::Usage(format!(
                "unknown news source {s:?}; use {} or all",
                publishers().join(", ")
            )));
        }
        out.extend(hits);
    }
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

/// Removes tags and collapses whitespace.
pub fn strip_html(s: &str) -> String {
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
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses one RSS 2.0 document into items labelled with `source`.
pub fn parse_feed(xml: &str, source: &str) -> Result<Vec<NewsItem>> {
    let channel = rss::Channel::read_from(xml.as_bytes())
        .map_err(|e| IdxError::provider(NAME, format!("{source}: invalid RSS: {e}")))?;
    Ok(channel
        .items()
        .iter()
        .filter_map(|it| {
            let title = strip_html(it.title()?);
            let url = it.link().map(str::trim).unwrap_or("").to_string();
            if title.is_empty() || url.is_empty() {
                return None;
            }
            let published = it
                .pub_date()
                .and_then(|d| DateTime::parse_from_rfc2822(d.trim()).ok())
                .map(|d| d.with_timezone(&wib()));
            Some(NewsItem {
                title,
                url,
                source: source.to_string(),
                published,
                summary: it.description().map(strip_html).unwrap_or_default(),
                matched_by: "all".into(),
                scope: "market".into(),
                sentiment: 0.0,
                positive_terms: vec![],
                negative_terms: vec![],
            })
        })
        .collect())
}

/// Fetches the given feeds concurrently. Feeds that fail become warnings;
/// the call fails only when none succeed.
pub async fn fetch(
    fetcher: &Arc<Fetcher>,
    feeds: &[Feed],
) -> Result<(Vec<NewsItem>, Vec<String>, bool, bool)> {
    let futs = feeds.iter().map(|f| async move {
        let r = fetcher
            .get(
                f.url,
                &[(
                    "Accept".into(),
                    "application/rss+xml, application/xml;q=0.9, */*;q=0.8".into(),
                )],
                TTL,
                None,
            )
            .await;
        (f, r)
    });
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut cached = true;
    let mut stale = false;
    let mut ok = 0;
    for (feed, r) in join_all(futs).await {
        match r {
            Ok(f) if f.is_success() => match parse_feed(&f.body, feed.id) {
                Ok(list) => {
                    ok += 1;
                    cached &= f.cached;
                    stale |= f.stale;
                    items.extend(list);
                }
                Err(e) => warnings.push(e.to_string()),
            },
            Ok(f) => warnings.push(format!("{}: HTTP {}", feed.id, f.status)),
            Err(IdxError::OfflineMiss(_)) => {
                warnings.push(format!("{}: not cached (offline)", feed.id))
            }
            Err(e) => warnings.push(format!("{}: {e}", feed.id)),
        }
    }
    if ok == 0 {
        return Err(IdxError::provider(
            NAME,
            format!("no news feed could be read: {}", warnings.join("; ")),
        ));
    }
    items.sort_by_key(|a| std::cmp::Reverse(a.published));
    items.dedup_by(|a, b| a.url == b.url);
    Ok((items, warnings, cached, stale))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KONTAN: &str = include_str!("../../../tests/fixtures/news/kontan_investasi.xml");
    const CNBC: &str = include_str!("../../../tests/fixtures/news/cnbc_market_excerpt.xml");

    #[test]
    fn parses_kontan() {
        let items = parse_feed(KONTAN, "kontan/investasi").unwrap();
        assert_eq!(items.len(), 25);
        let first = &items[0];
        assert!(first.title.starts_with("Yield SBN"), "{}", first.title);
        assert!(first
            .url
            .starts_with("https://investasi.kontan.co.id/news/"));
        assert_eq!(
            first.published.unwrap().to_rfc3339(),
            "2026-10-17T09:35:05+07:00"
        );
        assert!(first.summary.contains("BI-Rate"));
        assert!(items.iter().any(|i| i.summary.contains("(ANTM)")));
    }

    #[test]
    fn parses_cnbc_and_strips_images() {
        let items = parse_feed(CNBC, "cnbcindonesia/market").unwrap();
        assert_eq!(items.len(), 20);
        assert!(!items[0].summary.contains('<'));
        assert!(items[0].summary.contains("MDLA"), "{}", items[0].summary);
        assert_eq!(items[0].source, "cnbcindonesia/market");
    }

    #[test]
    fn feed_selection() {
        assert_eq!(select(&[]).unwrap().len(), 5);
        assert_eq!(select(&["kontan".into()]).unwrap().len(), 4);
        assert_eq!(select(&["all".into()]).unwrap().len(), FEEDS.len());
        assert!(select(&["detik".into()]).is_err());
        assert_eq!(strip_html("<p>a &amp; b</p>  <img src=x/> c"), "a & b c");
    }
}
