//! Tables for `idx ownership ...`.

use std::path::Path;

use chrono::NaiveDate;
use comfy_table::{Attribute, Cell, Color};

use crate::model::{
    ChangeKind, ChangesReport, DiscoveredFile, EntityReport, HoldersReport, ImportReport, Release,
};
use crate::output::format as f;
use crate::output::table::{base_table, header, right, signed};

fn pct(v: f64) -> String {
    format!("{}%", f::num(v, 2))
}

fn pct_opt(v: Option<f64>) -> String {
    v.map(pct).unwrap_or_else(|| f::DASH.into())
}

fn shares(v: i64) -> String {
    f::num(v as f64, 0)
}

pub fn holders(r: &HoldersReport, color: bool) -> String {
    let mut out = format!(
        "{} · {} · holders above 1% as of {} · {} listed = {}\n",
        r.symbol,
        r.issuer_name.clone().unwrap_or_else(|| f::DASH.into()),
        r.as_of,
        r.holders.len(),
        pct(r.listed_pct)
    );
    let mut t = base_table(color);
    t.set_header(header(&["#", "Holder", "Type", "L/A", "Shares", "%"]));
    for (i, h) in r.holders.iter().enumerate() {
        t.add_row(vec![
            right((i + 1).to_string()),
            Cell::new(f::clip(&h.holder, 48)).add_attribute(Attribute::Bold),
            Cell::new(f::clip(&f::text(&h.investor_type), 22)),
            Cell::new(f::text(&h.local_foreign)),
            right(shares(h.shares_total)),
            right(pct(h.pct)),
        ]);
    }
    out.push_str(&t.to_string());
    out.push_str("\nSource: KSEI via IDX monthly register, imported locally. Holders below 1% are not published.\n");
    out
}

pub fn entity(r: &EntityReport, color: bool) -> String {
    let mut out = format!(
        "\"{}\" · {} matching holder{} · as of {}\n",
        r.query,
        r.entities.len(),
        if r.entities.len() == 1 { "" } else { "s" },
        r.as_of
    );
    for e in &r.entities {
        out.push_str(&format!("  {e}\n"));
    }
    let mut t = base_table(color);
    t.set_header(header(&[
        "Symbol",
        "Issuer",
        "Holder (as printed)",
        "Type",
        "Shares",
        "%",
    ]));
    for h in &r.holdings {
        t.add_row(vec![
            Cell::new(&h.symbol).add_attribute(Attribute::Bold),
            Cell::new(f::clip(&f::text(&h.issuer_name), 30)),
            Cell::new(f::clip(&h.holder_raw, 40)),
            Cell::new(f::clip(&f::text(&h.investor_type), 18)),
            right(shares(h.shares_total)),
            right(pct(h.pct)),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

pub fn changes(r: &ChangesReport, color: bool) -> String {
    let s = &r.summary;
    let pp = |v: f64| format!("{}{} pp", if v > 0.0 { "+" } else { "" }, f::num(v, 2));
    let mut out = format!(
        "Changes {} → {}{} · {} match{} · {} issuer{} · {} holder{}\n",
        r.from,
        r.to,
        r.symbol
            .as_ref()
            .map(|s| format!(" · {s}"))
            .unwrap_or_default(),
        r.total_matches,
        if r.total_matches == 1 { "" } else { "es" },
        s.issuers,
        if s.issuers == 1 { "" } else { "s" },
        s.holders,
        if s.holders == 1 { "" } else { "s" },
    );
    out.push_str(&format!(
        "Summary · {} new · {} exited · {} increased · {} decreased · net {} (foreign {}, local {})\n",
        s.new_holders,
        s.exits,
        s.increased,
        s.decreased,
        pp(s.net_pp),
        pp(s.foreign_net_pp),
        pp(s.local_net_pp)
    ));
    if !s.by_investor_type.is_empty() {
        out.push_str(&format!(
            "By type · {}\n",
            s.by_investor_type
                .iter()
                .take(6)
                .map(|t| format!("{} {} ({})", t.investor_type, pp(t.net_pp), t.rows))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    if r.symbol.is_none() && !s.issuer_flow.is_empty() {
        out.push_str(&format!(
            "Issuer flow · {}\n",
            s.issuer_flow
                .iter()
                .take(8)
                .map(|i| format!(
                    "{} {} (foreign {})",
                    i.symbol,
                    pp(i.net_pp),
                    pp(i.foreign_net_pp)
                ))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    if r.changes.len() < r.total_matches {
        let first = r.offset + 1;
        let last = r.offset + r.changes.len();
        out.push_str(&format!(
            "Rows {first}–{last} of {} · next page: --offset {} · all rows: --limit 0\n",
            r.total_matches, last
        ));
    }
    let mut t = base_table(color);
    t.set_header(header(&[
        "Symbol",
        "Holder",
        "Type",
        "L/A",
        "Kind",
        "% from",
        "% to",
        "Δ pp",
        "Δ shares",
    ]));
    for c in &r.changes {
        let kind = Cell::new(match c.kind {
            ChangeKind::Changed => "changed",
            ChangeKind::New => "new",
            ChangeKind::Exited => "exited",
        });
        let kind = if color {
            match c.kind {
                ChangeKind::New => kind.fg(Color::Green),
                ChangeKind::Exited => kind.fg(Color::Red),
                ChangeKind::Changed => kind,
            }
        } else {
            kind
        };
        t.add_row(vec![
            Cell::new(&c.symbol).add_attribute(Attribute::Bold),
            Cell::new(f::clip(&c.holder, 40)),
            Cell::new(f::clip(&f::text(&c.investor_type), 12)),
            Cell::new(f::text(&c.local_foreign)),
            kind,
            right(pct_opt(c.pct_from)),
            right(pct_opt(c.pct_to)),
            signed(f::change(Some(c.pct_change)), Some(c.pct_change), color),
            signed(
                f::change(Some(c.shares_change as f64)),
                Some(c.shares_change as f64),
                color,
            ),
        ]);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out
}

pub fn releases(list: &[Release], db: &Path, color: bool) -> String {
    let mut t = base_table(color);
    t.set_header(header(&[
        "As of", "Format", "Rows", "Issuers", "Holders", "Imported", "Source",
    ]));
    for r in list {
        t.add_row(vec![
            Cell::new(r.as_of.to_string()).add_attribute(Attribute::Bold),
            Cell::new(&r.format),
            right(f::num(r.rows as f64, 0)),
            right(f::num(r.issuers as f64, 0)),
            right(f::num(r.entities as f64, 0)),
            Cell::new(r.imported_at.get(..10).unwrap_or(&r.imported_at)),
            Cell::new(f::clip(&r.source, 48)),
        ]);
    }
    format!("{}\nDatabase: {}\n", t, db.display())
}

pub fn imports(list: &[ImportReport]) -> String {
    let mut out = String::new();
    for r in list {
        out.push_str(&format!(
            "Imported {} ({}): {} rows, {} issuers, {} holders{} → {}\n",
            r.release.as_of,
            r.release.format,
            f::num(r.release.rows as f64, 0),
            r.release.issuers,
            r.release.entities,
            if r.replaced {
                " (replaced earlier import)"
            } else {
                ""
            },
            r.database
        ));
    }
    if list.is_empty() {
        out.push_str("Nothing imported.\n");
    }
    out
}

pub fn discovered(files: &[DiscoveredFile], have: &[NaiveDate], color: bool) -> String {
    let mut t = base_table(color);
    t.set_header(header(&["As of", "Format", "Imported", "Title", "URL"]));
    for d in files {
        t.add_row(vec![
            Cell::new(d.as_of.to_string()).add_attribute(Attribute::Bold),
            Cell::new(&d.format),
            Cell::new(if have.contains(&d.as_of) { "yes" } else { "no" }),
            Cell::new(f::clip(&d.title, 48)),
            Cell::new(&d.url),
        ]);
    }
    format!("{t}\n")
}
