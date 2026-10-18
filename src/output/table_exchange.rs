//! Tables for exchange-wide data: holidays, instruments, members, indices,
//! the corporate-action calendar, corporate actions, foreign flow and broker
//! activity. Same conventions as `table.rs`: pure functions, dashes for
//! missing values, colour only when asked.

use comfy_table::{Attribute, Cell};

use crate::model::{Holidays, IndexBoard, InstrumentList, MemberList};
use crate::output::format as f;
use crate::output::table::{base_table, header, right, signed};

/// Holiday list plus today's trading status.
pub fn holidays(h: &Holidays, _color: bool) -> String {
    let mut out = String::new();
    let scope = match h.year {
        Some(y) => format!("IDX trading holidays {y}"),
        None => "IDX trading holidays".to_string(),
    };
    out.push_str(&scope);
    if let Some(n) = h.trading_days {
        out.push_str(&format!(" · {n} trading days"));
    }
    out.push('\n');
    out.push_str(&format!(
        "Today {} is {} · next trading day {}\n",
        h.today,
        if h.today_is_trading_day {
            "a trading day"
        } else {
            "not a trading day"
        },
        h.next_trading_day
    ));
    if h.holidays.is_empty() {
        out.push_str("No holidays listed.\n");
        return out;
    }
    let mut t = base_table(false);
    t.set_header(header(&["Date", "Day", "Holiday", "English"]));
    for d in &h.holidays {
        t.add_row(vec![
            Cell::new(d.date.to_string()),
            Cell::new(&d.weekday),
            Cell::new(&d.name),
            Cell::new(d.name_en.as_deref().unwrap_or(f::DASH)),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out.push_str(&format!(
        "Covers {}. Weekend holidays are not listed; IDX can add a holiday later.\n",
        h.years_covered
            .iter()
            .map(|y| y.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out
}

/// Listed stocks.
pub fn instruments(l: &InstrumentList, _color: bool) -> String {
    let mut out = String::new();
    let mut scope = format!("{} of {} listed stocks", l.instruments.len(), l.total);
    if let Some(b) = &l.board {
        scope.push_str(&format!(" · board {b}"));
    }
    if let Some(q) = &l.search {
        scope.push_str(&format!(" · matching {q:?}"));
    }
    out.push_str(&scope);
    out.push('\n');
    if l.instruments.is_empty() {
        return out;
    }
    let mut t = base_table(false);
    t.set_header(header(&["Symbol", "Name", "Board", "Listed", "Shares"]));
    for i in &l.instruments {
        t.add_row(vec![
            Cell::new(i.symbol.as_str()).add_attribute(Attribute::Bold),
            Cell::new(f::clip(i.name.as_deref().unwrap_or(f::DASH), 34)),
            Cell::new(i.board.as_deref().unwrap_or(f::DASH)),
            Cell::new(
                i.listing_date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| f::DASH.to_string()),
            ),
            right(f::compact(i.shares)),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

/// Exchange members.
pub fn members(l: &MemberList, _color: bool) -> String {
    let mut out = format!("{} of {} exchange members", l.members.len(), l.total);
    if let Some(q) = &l.search {
        out.push_str(&format!(" · matching {q:?}"));
    }
    out.push('\n');
    if l.members.is_empty() {
        return out;
    }
    let mut t = base_table(false);
    t.set_header(header(&["Code", "Member", "Licences"]));
    for m in &l.members {
        t.add_row(vec![
            Cell::new(&m.code).add_attribute(Attribute::Bold),
            Cell::new(m.name.as_deref().unwrap_or(f::DASH)),
            Cell::new(
                m.licenses
                    .as_ref()
                    .map(|l| l.join(", "))
                    .unwrap_or_else(|| f::DASH.to_string()),
            ),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

/// The day's index board.
pub fn indices(b: &IndexBoard, color: bool) -> String {
    let mut out = format!("IDX indices · {}\n", b.date);
    let mut t = base_table(color);
    t.set_header(header(&[
        "Index",
        "Close",
        "Change",
        "%",
        "High",
        "Low",
        "Stocks",
        "Value",
        "Market cap",
    ]));
    for r in &b.indices {
        t.add_row(vec![
            Cell::new(&r.code).add_attribute(Attribute::Bold),
            right(f::ratio(r.close)),
            signed(
                f::change(r.change.map(|c| (c * 100.0).round() / 100.0)),
                r.change,
                color,
            ),
            signed(f::pct(r.change_pct), r.change_pct, color),
            right(f::ratio(r.high)),
            right(f::ratio(r.low)),
            right(
                r.constituents
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| f::DASH.to_string()),
            ),
            right(f::compact(r.value)),
            right(f::compact(r.market_cap)),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out.push_str(
        "COMPOSITE is IHSG (^JKSE). Value and market cap in IDR, as published by idx.co.id.\n",
    );
    out
}
