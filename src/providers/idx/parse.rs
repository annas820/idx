//! Pure parsers for idx.co.id's JSON endpoints, tested against the recorded
//! responses in `tests/fixtures/idx/`. Field names below are the source's;
//! nothing outside this module sees them.

use chrono::{NaiveDate, NaiveDateTime};
use serde::Deserialize;

use crate::error::{IdxError, Result};
use crate::model::{CalendarCategory, CalendarEvent, IndexRow, Instrument, Member, Symbol};

use super::NAME;

fn bad(what: &str, e: serde_json::Error) -> IdxError {
    IdxError::provider(NAME, format!("unexpected {what} response: {e}"))
}

/// `2026-10-16T00:00:00` -> 2026-10-16. The source writes `1911-01-01` (and
/// sometimes `1900-01-01`) for "no date"; those become `None`.
pub fn date(raw: &str) -> Option<NaiveDate> {
    let d = NaiveDate::parse_from_str(raw.get(..10)?, "%Y-%m-%d").ok()?;
    (chrono::Datelike::year(&d) >= 1950).then_some(d)
}

fn datetime(raw: &str) -> Option<NaiveDateTime> {
    let dt = NaiveDateTime::parse_from_str(raw.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok()?;
    (chrono::Datelike::year(&dt) >= 1950).then_some(dt)
}

fn text(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
}

/// `{"draw":0,"recordsTotal":963,"recordsFiltered":963,"data":[...]}`, the
/// DataTables shape most endpoints answer with.
#[derive(Deserialize)]
struct Table<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

/// One stock's day from the stock summary (`GetStockSummary`, and the
/// per-stock history `GetTradingInfoSS`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StockDay {
    #[serde(rename = "Date")]
    raw_date: String,
    pub stock_code: String,
    pub stock_name: Option<String>,
    pub previous: Option<f64>,
    pub open_price: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub change: Option<f64>,
    pub volume: Option<f64>,
    pub value: Option<f64>,
    pub frequency: Option<f64>,
    pub listed_shares: Option<f64>,
    pub foreign_sell: Option<f64>,
    pub foreign_buy: Option<f64>,
    pub non_regular_volume: Option<f64>,
    pub non_regular_value: Option<f64>,
}

impl StockDay {
    pub fn date(&self) -> Option<NaiveDate> {
        date(&self.raw_date)
    }
}

/// `GetStockSummary?date=YYYYMMDD`: every stock's day; empty on a day
/// without trading.
pub fn stock_summary(body: &str) -> Result<Vec<StockDay>> {
    let t: Table<StockDay> = serde_json::from_str(body).map_err(|e| bad("stock summary", e))?;
    Ok(t.data)
}

#[derive(Deserialize)]
struct History {
    #[serde(default = "Vec::new")]
    replies: Vec<StockDay>,
}

/// `GetTradingInfoSS?code=BBCA&start=0&length=N`: one stock's latest N
/// days, newest first; empty for an unknown code.
pub fn trading_history(body: &str) -> Result<Vec<StockDay>> {
    let h: History = serde_json::from_str(body).map_err(|e| bad("trading history", e))?;
    Ok(h.replies)
}

/// One member's day from `GetBrokerSummary`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BrokerDay {
    #[serde(rename = "IDFirm")]
    pub code: String,
    #[serde(rename = "FirmName")]
    pub name: Option<String>,
    #[serde(rename = "Volume")]
    pub volume: Option<f64>,
    #[serde(rename = "Value")]
    pub value: Option<f64>,
    #[serde(rename = "Frequency")]
    pub frequency: Option<f64>,
}

/// `GetBrokerSummary?date=YYYYMMDD`: every member's volume, value and
/// frequency (both sides together); empty on a day without trading.
pub fn broker_summary(body: &str) -> Result<Vec<BrokerDay>> {
    let t: Table<BrokerDay> = serde_json::from_str(body).map_err(|e| bad("broker summary", e))?;
    Ok(t.data)
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawIndex {
    index_code: String,
    previous: Option<f64>,
    highest: Option<f64>,
    lowest: Option<f64>,
    close: Option<f64>,
    number_of_stock: Option<f64>,
    change: Option<f64>,
    volume: Option<f64>,
    value: Option<f64>,
    frequency: Option<f64>,
    market_capital: Option<f64>,
}

/// `GetIndexSummary?date=YYYYMMDD`: every index's day; empty on a day
/// without trading.
pub fn index_summary(body: &str) -> Result<Vec<IndexRow>> {
    let t: Table<RawIndex> = serde_json::from_str(body).map_err(|e| bad("index summary", e))?;
    Ok(t.data
        .into_iter()
        .map(|r| IndexRow {
            change_pct: match (r.change, r.previous) {
                (Some(c), Some(p)) if p != 0.0 => Some(c / p * 100.0),
                _ => None,
            },
            code: r.index_code.trim().to_string(),
            previous: r.previous,
            high: r.highest,
            low: r.lowest,
            close: r.close,
            change: r.change,
            constituents: r.number_of_stock.map(|n| n as u32),
            volume: r.volume,
            value: r.value,
            frequency: r.frequency,
            market_cap: r.market_capital,
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawSecurity {
    code: String,
    name: Option<String>,
    listing_date: Option<String>,
    shares: Option<f64>,
    listing_board: Option<String>,
}

/// `GetSecuritiesStock`: every listed stock. Rows whose code is not a
/// valid ticker are skipped.
pub fn securities(body: &str) -> Result<Vec<Instrument>> {
    let t: Table<RawSecurity> = serde_json::from_str(body).map_err(|e| bad("securities", e))?;
    Ok(t.data
        .into_iter()
        .filter_map(|r| {
            Some(Instrument {
                symbol: Symbol::parse(&r.code).ok()?,
                name: text(r.name),
                listing_date: r.listing_date.as_deref().and_then(date),
                shares: r.shares,
                board: text(r.listing_board),
            })
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawMember {
    code: String,
    name: Option<String>,
    license: Option<String>,
}

/// `GetBrokerSearch`: every exchange member with its licences.
pub fn members(body: &str) -> Result<Vec<Member>> {
    let t: Table<RawMember> = serde_json::from_str(body).map_err(|e| bad("members", e))?;
    Ok(t.data
        .into_iter()
        .filter(|r| !r.code.trim().is_empty())
        .map(|r| Member {
            code: r.code.trim().to_ascii_uppercase(),
            name: text(r.name),
            licenses: r.license.map(|l| {
                l.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }),
        })
        .collect())
}

#[derive(Deserialize)]
struct RawCalendar {
    #[serde(rename = "Results", default = "Vec::new")]
    results: Vec<RawEvent>,
}

#[derive(Deserialize)]
struct RawEvent {
    id: u64,
    title: Option<String>,
    description: Option<String>,
    location: Option<String>,
    start: String,
    #[serde(rename = "Jenis")]
    kind: Option<String>,
    #[serde(rename = "Step")]
    step: Option<String>,
    #[serde(rename = "TglWaktuRups")]
    meeting: Option<String>,
    #[serde(rename = "TglWaktuPE")]
    expose: Option<String>,
}

/// The step a `tanggal ...` entry marks (dividends, rights).
fn date_milestone(d: &str) -> Option<&'static str> {
    let rest = d.strip_prefix("tanggal ")?;
    let table: [(&str, &str); 8] = [
        ("cum", "cum"),
        ("ex", "ex"),
        ("dps", "recording"),
        ("pembayaran", "payment"),
        ("distribusi", "distribution"),
        ("pencatatan", "listing"),
        ("awal perdagangan", "trading_start"),
        ("akhir perdagangan", "trading_end"),
    ];
    table
        .iter()
        .find(|(k, _)| rest.starts_with(&format!("{k} ")))
        .map(|(_, v)| *v)
}

/// Plan, notice, result or material, from the first words of a meeting or
/// public-expose entry, or the source's step code.
fn stage(d: &str, step: &str) -> Option<&'static str> {
    if d.starts_with("hasil") || step == "hsl" {
        Some("result")
    } else if d.starts_with("panggilan") || step == "pgl" {
        Some("notice")
    } else if d.starts_with("materi") {
        Some("material")
    } else if d.starts_with("rencana")
        || d.starts_with("pemberitahuan")
        || step == "rcn"
        || step == "rencana"
    {
        Some("plan")
    } else {
        None
    }
}

/// Category and milestone of one entry, from its type field and wording.
pub fn classify(
    kind: &str,
    step: &str,
    description: &str,
) -> (CalendarCategory, Option<&'static str>) {
    use CalendarCategory as C;
    let k = kind.trim().to_lowercase();
    let s = step.trim().to_lowercase();
    let d = description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    // Bonds and sukuk first: their meetings (RUPO, RUPSU) would otherwise
    // read as shareholders' meetings.
    if k == "rupo" || k == "rupsu" {
        return (C::Bond, stage(&d, &s));
    }
    if k == "obligasijatuhtempo" || d.starts_with("jatuh tempo") {
        return (C::Bond, Some("maturity"));
    }
    if k == "pencatatanawal" || d.starts_with("pencatatan obligasi") {
        return (C::Bond, Some("listing"));
    }
    if d.starts_with("informasi pembayaran") {
        let m = if d.contains("pokok") {
            "principal"
        } else {
            "coupon"
        };
        return (C::Bond, Some(m));
    }
    // Public expose entries carry `tahunan` / `insidentil` in the type field;
    // their results read "Hasil Tahunan ..." without the words.
    if d.contains("public expose") || k == "tahunan" || k == "insidentil" {
        return (C::PublicExpose, stage(&d, &s));
    }
    if d.contains("rups") {
        return (C::Rups, stage(&d, &s));
    }
    if d.contains("pembelian kembali") || d.contains("buyback") {
        return (C::Buyback, Some("report"));
    }
    let category = if d.contains("hmetd") {
        C::Rights
    } else if d.contains("dividen") {
        C::Dividend
    } else if d.contains("bonus") {
        C::Bonus
    } else if d.contains("pemecahan") || d.contains("split") || d.contains("penggabungan nilai") {
        C::Split
    } else if d.contains("waran") {
        C::Warrant
    } else if d.contains("pencatatan") {
        C::Listing
    } else {
        C::Other
    };
    (category, date_milestone(&d))
}

/// `GetCalendar?range=d|m&date=YYYYMMDD[&code=XXXX]`: calendar entries,
/// oldest first.
pub fn calendar(body: &str) -> Result<Vec<CalendarEvent>> {
    let c: RawCalendar = serde_json::from_str(body).map_err(|e| bad("calendar", e))?;
    let mut out: Vec<CalendarEvent> = c
        .results
        .into_iter()
        .filter_map(|r| {
            let day = date(&r.start)?;
            let description = text(r.description).unwrap_or_default();
            let kind = text(r.kind);
            let step = text(r.step);
            let (category, milestone) = classify(
                kind.as_deref().unwrap_or(""),
                step.as_deref().unwrap_or(""),
                &description,
            );
            Some(CalendarEvent {
                date: day,
                code: text(r.title).unwrap_or_default().to_ascii_uppercase(),
                category,
                milestone: milestone.map(str::to_string),
                description,
                location: text(r.location),
                meeting_at: r.meeting.as_deref().and_then(datetime),
                expose_date: r.expose.as_deref().and_then(date),
                source_type: kind,
                source_step: step,
                id: r.id,
            })
        })
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STOCKS: &str =
        include_str!("../../../tests/fixtures/idx/stocksummary_20261016_excerpt.json");
    const NO_TRADING: &str =
        include_str!("../../../tests/fixtures/idx/stocksummary_20261018_no_trading.json");
    const BROKERS: &str = include_str!("../../../tests/fixtures/idx/brokersummary_20261016.json");
    const INDICES: &str = include_str!("../../../tests/fixtures/idx/indexsummary_20261016.json");
    const HISTORY: &str =
        include_str!("../../../tests/fixtures/idx/tradinginfoss_bbca_excerpt.json");
    const HISTORY_EMPTY: &str =
        include_str!("../../../tests/fixtures/idx/tradinginfoss_zzzz_empty.json");
    const SECURITIES: &str = include_str!("../../../tests/fixtures/idx/securities_excerpt.json");
    const MEMBERS: &str = include_str!("../../../tests/fixtures/idx/brokersearch.json");
    const MONTH: &str =
        include_str!("../../../tests/fixtures/idx/calendar_m_20261001_excerpt.json");
    const DAY: &str = include_str!("../../../tests/fixtures/idx/calendar_d_20261019.json");
    const EMPTY_CAL: &str = include_str!("../../../tests/fixtures/idx/calendar_m_bbca_empty.json");

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn stock_summary_rows_carry_foreign_shares() {
        let rows = stock_summary(STOCKS).unwrap();
        assert_eq!(rows.len(), 28);
        let bbca = rows.iter().find(|r| r.stock_code == "BBCA").unwrap();
        assert_eq!(bbca.date(), Some(d(2026, 10, 16)));
        assert_eq!(bbca.stock_name.as_deref(), Some("Bank Central Asia Tbk."));
        assert_eq!(bbca.previous, Some(6075.0));
        assert_eq!(bbca.close, Some(5175.0));
        assert_eq!(bbca.change, Some(-900.0));
        assert_eq!(bbca.volume, Some(1_241_098_200.0));
        assert_eq!(bbca.foreign_buy, Some(309_392_800.0));
        assert_eq!(bbca.foreign_sell, Some(757_227_900.0));
        assert_eq!(bbca.non_regular_volume, Some(16_501_628.0));
        assert!(stock_summary(NO_TRADING).unwrap().is_empty());
        assert!(stock_summary("<html>").is_err());
    }

    #[test]
    fn trading_history_is_newest_first_and_empty_for_unknown_codes() {
        let rows = trading_history(HISTORY).unwrap();
        assert_eq!(rows.len(), 30);
        assert_eq!(rows[0].date(), Some(d(2026, 10, 16)));
        assert_eq!(rows[1].date(), Some(d(2026, 10, 15)));
        assert!(rows.iter().all(|r| r.stock_code == "BBCA"));
        assert!(trading_history(HISTORY_EMPTY).unwrap().is_empty());
    }

    #[test]
    fn broker_summary_lists_every_member() {
        let rows = broker_summary(BROKERS).unwrap();
        assert_eq!(rows.len(), 88);
        assert_eq!(rows[0].code, "AD");
        assert_eq!(rows[0].name.as_deref(), Some("Sukadana Prima Sekuritas"));
        assert_eq!(rows[0].value, Some(103_161_000.0));
        let total: f64 = rows.iter().filter_map(|r| r.value).sum();
        // Both sides of every trade: twice the market's 32.0T.
        assert!((total - 64_027_626_516_936.0).abs() < 1.0, "{total}");
    }

    #[test]
    fn index_summary_maps_fields_and_computes_the_percent() {
        let rows = index_summary(INDICES).unwrap();
        assert_eq!(rows.len(), 45);
        let ihsg = &rows[0];
        assert_eq!(ihsg.code, "COMPOSITE");
        assert_eq!(ihsg.close, Some(5185.105));
        assert_eq!(ihsg.high, Some(5404.508));
        assert_eq!(ihsg.constituents, Some(918));
        let pct = ihsg.change_pct.unwrap();
        assert!((pct - (-678.675 / 5863.78 * 100.0)).abs() < 1e-9);
        assert_eq!(rows[1].code, "LQ45");
    }

    #[test]
    fn securities_and_members() {
        let s = securities(SECURITIES).unwrap();
        let bbca = s.iter().find(|i| i.symbol.as_str() == "BBCA").unwrap();
        assert_eq!(bbca.board.as_deref(), Some("Utama"));
        assert!(bbca.listing_date.is_some());
        assert!(bbca.shares.unwrap() > 1e11);
        let mglv = s.iter().find(|i| i.symbol.as_str() == "MGLV").unwrap();
        assert_eq!(mglv.listing_date, Some(d(2021, 6, 8)));
        assert_eq!(mglv.board.as_deref(), Some("Akselerasi"));

        let m = members(MEMBERS).unwrap();
        assert_eq!(m.len(), 89);
        let xc = m.iter().find(|x| x.code == "XC").unwrap();
        assert_eq!(xc.name.as_deref(), Some("AJAIB SEKURITAS ASIA"));
        assert_eq!(
            xc.licenses.as_deref().unwrap(),
            ["Penjamin Emisi Efek", "Perantara Pedagang Efek"]
        );
    }

    #[test]
    fn calendar_entries_are_classified() {
        let ev = calendar(DAY).unwrap();
        assert_eq!(ev.len(), 8);
        assert!(ev.iter().all(|e| e.date == d(2026, 10, 19)));
        let pyfa = &ev[0];
        assert_eq!(pyfa.code, "PYFA");
        assert_eq!(pyfa.category, CalendarCategory::Bond);
        assert_eq!(pyfa.milestone.as_deref(), Some("plan"));
        assert_eq!(pyfa.meeting_at, None, "1911 placeholder");
        assert!(calendar(EMPTY_CAL).unwrap().is_empty());

        let month = calendar(MONTH).unwrap();
        assert!(month.windows(2).all(|w| w[0].date <= w[1].date));
        let find = |prefix: &str| {
            month
                .iter()
                .find(|e| e.description.starts_with(prefix))
                .unwrap_or_else(|| panic!("no entry starting {prefix:?}"))
        };
        let cum = find("tanggal cum Dividen");
        assert_eq!(cum.category, CalendarCategory::Dividend);
        assert_eq!(cum.milestone.as_deref(), Some("cum"));
        let dps = find("tanggal DPS HMETD");
        assert_eq!(dps.category, CalendarCategory::Rights);
        assert_eq!(dps.milestone.as_deref(), Some("recording"));
        let pay = find("tanggal Pembayaran Dividen");
        assert_eq!(pay.milestone.as_deref(), Some("payment"));
        let end = find("tanggal akhir perdagangan hmetd");
        assert_eq!(end.milestone.as_deref(), Some("trading_end"));
        let rups = find("Pemberitahuan RUPS");
        assert_eq!(rups.category, CalendarCategory::Rups);
        assert_eq!(rups.milestone.as_deref(), Some("plan"));
        assert!(rups.meeting_at.is_some());
        let egm = find("Hasil RUPS Luar Biasa");
        assert_eq!(egm.category, CalendarCategory::Rups);
        assert_eq!(egm.milestone.as_deref(), Some("result"));
        assert!(egm
            .source_type
            .as_deref()
            .unwrap()
            .starts_with("Luar Biasa"));
        let expose = find("Hasil Tahunan");
        assert_eq!(expose.category, CalendarCategory::PublicExpose);
        assert_eq!(expose.milestone.as_deref(), Some("result"));
        assert!(expose.expose_date.is_some());
        let material = find("Materi public expose");
        assert_eq!(material.milestone.as_deref(), Some("material"));
        let buyback = find("Laporan Pembelian Kembali");
        assert_eq!(buyback.category, CalendarCategory::Buyback);
        let coupon = find("Informasi Pembayaran Kupon");
        assert_eq!(
            (coupon.category, coupon.milestone.as_deref()),
            (CalendarCategory::Bond, Some("coupon"))
        );
        let maturity = find("Jatuh Tempo");
        assert_eq!(maturity.milestone.as_deref(), Some("maturity"));
        let rupsu = month
            .iter()
            .find(|e| e.source_type.as_deref() == Some("rupsu"))
            .unwrap();
        assert_eq!(rupsu.category, CalendarCategory::Bond, "bondholder meeting");
        assert!(
            month.iter().all(|e| e.category != CalendarCategory::Other),
            "every recorded entry has a rule: {:?}",
            month
                .iter()
                .filter(|e| e.category == CalendarCategory::Other)
                .map(|e| &e.description)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn classify_rules_for_wording_not_in_the_fixtures() {
        use CalendarCategory as C;
        assert_eq!(
            classify("", "", "tanggal cum Saham Bonus PT X Tbk").0,
            C::Bonus
        );
        assert_eq!(
            classify("", "", "tanggal pencatatan Pemecahan Nilai Nominal PT X").0,
            C::Split
        );
        assert_eq!(classify("", "", "Something new").0, C::Other);
        assert_eq!(classify("", "", "Something new").1, None);
    }
}
