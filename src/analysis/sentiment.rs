//! Lexicon sentiment for Indonesian and English finance headlines. Deliberately
//! simple and inspectable: every matched term is reported with the score.

use std::collections::HashMap;

use crate::model::{NewsItem, SentimentLabel, SentimentReport, Symbol};

pub const METHOD: &str = "lexicon: (positive - negative) / (positive + negative) per headline over title + summary, Indonesian and English finance terms; mean over matched items";

const POSITIVE: &[&str] = &[
    // Indonesian
    "naik",
    "menguat",
    "melonjak",
    "melesat",
    "meroket",
    "menanjak",
    "rebound",
    "untung",
    "laba",
    "cuan",
    "tumbuh",
    "pertumbuhan",
    "ekspansi",
    "rekor",
    "positif",
    "optimis",
    "optimistis",
    "dividen",
    "akuisisi",
    "buyback",
    "surplus",
    "kenaikan",
    "membaik",
    "pulih",
    "pemulihan",
    "tertinggi",
    "prospektif",
    "kinerja solid",
    "solid",
    "moncer",
    "cemerlang",
    "menebar",
    "beli",
    "akumulasi",
    "upgrade",
    "apresiasi",
    "bullish",
    // English
    "gain",
    "gains",
    "rally",
    "rallies",
    "rise",
    "rises",
    "soar",
    "soars",
    "jump",
    "jumps",
    "profit",
    "profits",
    "growth",
    "record",
    "beat",
    "beats",
    "outperform",
    "strong",
    "raised",
    "upgrade",
    "upgrades",
    "buy",
    "surge",
    "surges",
    "recover",
    "recovery",
    "high",
];

const NEGATIVE: &[&str] = &[
    // Indonesian
    "turun",
    "melemah",
    "anjlok",
    "ambles",
    "merosot",
    "tergelincir",
    "rugi",
    "kerugian",
    "koreksi",
    "terkoreksi",
    "jatuh",
    "longsor",
    "tertekan",
    "negatif",
    "pesimis",
    "pesimistis",
    "gagal bayar",
    "default",
    "suspensi",
    "suspend",
    "disuspensi",
    "delisting",
    "denda",
    "sanksi",
    "korupsi",
    "phk",
    "penurunan",
    "terendah",
    "lesu",
    "melambat",
    "perlambatan",
    "jual",
    "tekanan jual",
    "downgrade",
    "rontok",
    "bearish",
    "defisit",
    "penipuan",
    // English
    "loss",
    "losses",
    "fall",
    "falls",
    "drop",
    "drops",
    "plunge",
    "plunges",
    "slump",
    "slumps",
    "decline",
    "declines",
    "weak",
    "cut",
    "cuts",
    "downgrade",
    "downgrades",
    "fraud",
    "lawsuit",
    "miss",
    "misses",
    "sell",
    "selloff",
    "low",
    "probe",
    "investigation",
];

fn normalise(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn hits(text: &str, lexicon: &[&str]) -> Vec<String> {
    let padded = format!(" {text} ");
    let mut out = Vec::new();
    for term in lexicon {
        if padded.contains(&format!(" {term} ")) {
            out.push((*term).to_string());
        }
    }
    out
}

/// Scores one headline: -1 to +1, plus the terms that fired.
pub fn score_text(text: &str) -> (f64, Vec<String>, Vec<String>) {
    let norm = normalise(text);
    let pos = hits(&norm, POSITIVE);
    let neg = hits(&norm, NEGATIVE);
    let total = pos.len() + neg.len();
    let score = if total == 0 {
        0.0
    } else {
        (pos.len() as f64 - neg.len() as f64) / total as f64
    };
    (score, pos, neg)
}

/// Fills `sentiment`, `positive_terms`, `negative_terms` on an item.
pub fn score_item(item: &mut NewsItem) {
    let (s, p, n) = score_text(&format!("{} {}", item.title, item.summary));
    item.sentiment = s;
    item.positive_terms = p;
    item.negative_terms = n;
}

fn top_terms(items: &[NewsItem], positive: bool, n: usize) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for it in items {
        let terms = if positive {
            &it.positive_terms
        } else {
            &it.negative_terms
        };
        for t in terms {
            *counts.entry(t.as_str()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(k, c)| (k.to_string(), c))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

/// Aggregates scored items into the report. `items` should already be
/// filtered to the symbol and scored.
pub fn aggregate(
    symbol: &Symbol,
    matched_name: Option<String>,
    items: &[NewsItem],
    headline_limit: usize,
) -> SentimentReport {
    let n = items.len();
    let positive = items.iter().filter(|i| i.sentiment > 0.0).count();
    let negative = items.iter().filter(|i| i.sentiment < 0.0).count();
    let neutral = n - positive - negative;
    let score = if n == 0 {
        0.0
    } else {
        items.iter().map(|i| i.sentiment).sum::<f64>() / n as f64
    };
    let label = if n == 0 {
        SentimentLabel::NoCoverage
    } else if score >= 0.15 {
        SentimentLabel::Positive
    } else if score <= -0.15 {
        SentimentLabel::Negative
    } else {
        SentimentLabel::Neutral
    };
    let mut headlines: Vec<NewsItem> = items.to_vec();
    headlines.sort_by(|a, b| {
        b.sentiment
            .abs()
            .partial_cmp(&a.sentiment.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.published.cmp(&a.published))
    });
    headlines.truncate(headline_limit);
    SentimentReport {
        symbol: symbol.clone(),
        matched_name,
        label,
        score: (score * 1000.0).round() / 1000.0,
        items: n,
        positive,
        negative,
        neutral,
        oldest: items.iter().filter_map(|i| i.published).min(),
        newest: items.iter().filter_map(|i| i.published).max(),
        top_positive_terms: top_terms(items, true, 5),
        top_negative_terms: top_terms(items, false, 5),
        headlines,
        method: METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> NewsItem {
        let mut i = NewsItem {
            title: title.into(),
            url: String::new(),
            source: "t".into(),
            published: None,
            summary: String::new(),
            matched_by: "ticker".into(),
            scope: "company".into(),
            sentiment: 0.0,
            positive_terms: vec![],
            negative_terms: vec![],
        };
        score_item(&mut i);
        i
    }

    #[test]
    fn scores_indonesian_and_english() {
        let (s, p, n) = score_text("Saham BBCA melonjak, laba naik 12%");
        assert!(s > 0.0, "{p:?} {n:?}");
        assert!(p.contains(&"melonjak".to_string()));
        let (s, _, n) = score_text("IHSG anjlok, investor asing jual bersih");
        assert!(s < 0.0);
        assert!(n.contains(&"anjlok".to_string()));
        let (s, _, _) = score_text("Rapat umum pemegang saham digelar Rabu");
        assert_eq!(s, 0.0);
        // Word boundaries: "sell" must not fire inside "Russell".
        let (_, _, n) = score_text("Russell index rebalancing");
        assert!(n.is_empty(), "{n:?}");
    }

    #[test]
    fn aggregates() {
        let items = vec![
            item("Laba BBCA naik"),
            item("BBCA menguat ke rekor"),
            item("Saham BBCA turun tipis"),
        ];
        let r = aggregate(&Symbol::parse("BBCA").unwrap(), None, &items, 2);
        assert_eq!(r.items, 3);
        assert_eq!(r.positive, 2);
        assert_eq!(r.negative, 1);
        assert_eq!(r.label, SentimentLabel::Positive);
        assert_eq!(r.headlines.len(), 2);
        assert_eq!(r.top_positive_terms[0].1, 1);
        let empty = aggregate(&Symbol::parse("BBCA").unwrap(), None, &[], 5);
        assert_eq!(empty.label, SentimentLabel::NoCoverage);
    }
}
