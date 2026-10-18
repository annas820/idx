//! Tables for exchange-wide data: holidays, instruments, members, indices,
//! the corporate-action calendar, corporate actions, foreign flow and broker
//! activity. Same conventions as `table.rs`: pure functions, dashes for
//! missing values, colour only when asked.

use comfy_table::Cell;

use crate::model::Holidays;
use crate::output::format as f;
use crate::output::table::{base_table, header};

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
