//! Report-style tables: profile, fundamentals, compare, financials, earnings
//! and technicals. Same conventions as `table.rs`: pure functions, dashes for
//! missing values, colour only when asked.

use comfy_table::{Attribute, Cell, Color, Table};

use crate::analysis::fundamental::summary_line;
use crate::model::{
    Earnings, FinancialPeriod, Financials, Fundamentals, Profile, SignalLabel, StatementPeriod,
    Technical, Trend, VerdictLabel,
};
use crate::output::format as f;
use crate::output::table::{base_table, header, right, signed};

fn pct_opt(v: Option<f64>) -> String {
    v.map(|x| format!("{}%", f::num(x, 2)))
        .unwrap_or_else(|| f::DASH.into())
}

fn ratio_x(v: Option<f64>) -> String {
    v.map(|x| format!("{}x", f::num(x, 2)))
        .unwrap_or_else(|| f::DASH.into())
}

fn date_opt(d: Option<chrono::NaiveDate>) -> String {
    d.map(|d| d.to_string()).unwrap_or_else(|| f::DASH.into())
}

fn title_case(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Greedy word wrap for the summary paragraph.
pub fn wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn kv_table(color: bool, rows: Vec<(String, Cell)>) -> Table {
    let mut t = base_table(color);
    t.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);
    for (k, v) in rows {
        t.add_row(vec![Cell::new(k).add_attribute(Attribute::Bold), v]);
    }
    t
}

fn section(color: bool, name: &str) -> Cell {
    let c = Cell::new(name.to_uppercase()).add_attribute(Attribute::Bold);
    if color {
        c.fg(Color::Cyan)
    } else {
        c
    }
}

pub fn profile(p: &Profile, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} · {}\n",
        p.symbol,
        p.name.clone().unwrap_or_else(|| f::DASH.into())
    ));
    let rows = vec![
        ("Sector".into(), Cell::new(f::text(&p.sector))),
        ("Industry".into(), Cell::new(f::text(&p.industry))),
        ("Exchange".into(), Cell::new(f::text(&p.exchange))),
        ("Currency".into(), Cell::new(f::text(&p.currency))),
        ("Market cap".into(), Cell::new(f::compact(p.market_cap))),
        (
            "Shares outstanding".into(),
            Cell::new(f::int(p.shares_outstanding)),
        ),
        ("Employees".into(), Cell::new(f::int(p.employees))),
        (
            "Founded".into(),
            Cell::new(
                p.founded_year
                    .map(|y| y.to_string())
                    .unwrap_or_else(|| f::DASH.into()),
            ),
        ),
        (
            "Headquarters".into(),
            Cell::new(match (&p.city, &p.country) {
                (Some(c), Some(k)) => format!("{c}, {k}"),
                (Some(c), None) => c.clone(),
                (None, Some(k)) => k.clone(),
                _ => f::DASH.into(),
            }),
        ),
        ("Website".into(), Cell::new(f::text(&p.website))),
    ];
    out.push_str(&kv_table(color, rows).to_string());
    out.push('\n');
    if let Some(s) = &p.summary {
        out.push('\n');
        out.push_str(&wrap(s, 88));
    }
    if !p.directors.is_empty() {
        out.push('\n');
        out.push_str("Board and management\n");
        for d in p.directors.iter().take(10) {
            out.push_str(&format!(
                "  {}{}\n",
                d.name,
                d.title
                    .as_ref()
                    .map(|t| format!(" · {t}"))
                    .unwrap_or_default()
            ));
        }
        if p.directors.len() > 10 {
            out.push_str(&format!(
                "  … and {} more (see -o json)\n",
                p.directors.len() - 10
            ));
        }
    }
    out.push_str(&format!("Source: {}\n", p.source));
    out
}

fn verdict_cell(f: &Fundamentals, color: bool) -> Cell {
    let Some(v) = &f.verdict else {
        return Cell::new(f::DASH);
    };
    let c = Cell::new(v.label.as_str());
    if !color {
        return c;
    }
    match v.label {
        VerdictLabel::Undervalued => c.fg(Color::Green),
        VerdictLabel::Overvalued => c.fg(Color::Red),
        VerdictLabel::Fair => c.fg(Color::Yellow),
        VerdictLabel::InsufficientData => c,
    }
}

pub fn fundamentals(fd: &Fundamentals, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} · {} · {}{}\n",
        fd.symbol,
        fd.name.clone().unwrap_or_else(|| f::DASH.into()),
        f::text(&fd.sector),
        fd.industry
            .as_ref()
            .map(|i| format!(" / {i}"))
            .unwrap_or_default()
    ));
    let price_line = format!(
        "Price {} {} · 52w {} – {} · beta {}",
        f::price(fd.price),
        fd.currency.clone().unwrap_or_default(),
        f::price(fd.week52_low),
        f::price(fd.week52_high),
        f::ratio(fd.beta)
    );
    out.push_str(&price_line);
    out.push('\n');

    let v = &fd.valuation;
    let p = &fd.profitability;
    let h = &fd.health;
    let g = &fd.growth;
    let d = &fd.dividend;
    let ps = &fd.per_share;
    let a = &fd.analyst;
    let rows: Vec<(String, Cell)> = vec![
        ("".into(), section(color, "valuation")),
        ("Market cap".into(), right(f::compact(v.market_cap))),
        (
            "Enterprise value".into(),
            right(f::compact(v.enterprise_value)),
        ),
        ("P/E (trailing)".into(), right(f::ratio(v.pe_trailing))),
        ("P/E (forward)".into(), right(f::ratio(v.pe_forward))),
        ("PEG".into(), right(f::ratio(v.peg))),
        ("P/B".into(), right(f::ratio(v.price_to_book))),
        ("P/S".into(), right(f::ratio(v.price_to_sales))),
        ("EV/EBITDA".into(), right(f::ratio(v.ev_to_ebitda))),
        ("EV/Revenue".into(), right(f::ratio(v.ev_to_revenue))),
        ("".into(), section(color, "profitability")),
        ("ROE".into(), right(pct_opt(p.roe_pct))),
        ("ROA".into(), right(pct_opt(p.roa_pct))),
        ("Gross margin".into(), right(pct_opt(p.gross_margin_pct))),
        (
            "Operating margin".into(),
            right(pct_opt(p.operating_margin_pct)),
        ),
        ("Net margin".into(), right(pct_opt(p.net_margin_pct))),
        ("".into(), section(color, "balance sheet")),
        ("Debt / equity".into(), right(ratio_x(h.debt_to_equity))),
        ("Current ratio".into(), right(f::ratio(h.current_ratio))),
        ("Quick ratio".into(), right(f::ratio(h.quick_ratio))),
        ("Total debt".into(), right(f::compact(h.total_debt))),
        ("Total cash".into(), right(f::compact(h.total_cash))),
        ("".into(), section(color, "growth")),
        (
            "Revenue growth (yoy)".into(),
            signed(f::pct(g.revenue_growth_pct), g.revenue_growth_pct, color),
        ),
        (
            "Earnings growth (yoy)".into(),
            signed(f::pct(g.earnings_growth_pct), g.earnings_growth_pct, color),
        ),
        (
            "Revenue CAGR 5y".into(),
            signed(f::pct(g.revenue_cagr_5y_pct), g.revenue_cagr_5y_pct, color),
        ),
        (
            "EPS growth 3y".into(),
            signed(f::pct(g.eps_growth_3y_pct), g.eps_growth_3y_pct, color),
        ),
        ("".into(), section(color, "dividend")),
        ("Yield".into(), right(pct_opt(d.yield_pct))),
        ("Payout ratio".into(), right(pct_opt(d.payout_ratio_pct))),
        ("Ex-dividend".into(), right(date_opt(d.ex_dividend_date))),
        ("".into(), section(color, "per share")),
        ("EPS (ttm)".into(), right(f::price(ps.eps_ttm))),
        ("EPS (forward)".into(), right(f::price(ps.eps_forward))),
        ("Book value".into(), right(f::price(ps.book_value))),
        ("Revenue".into(), right(f::price(ps.revenue))),
        ("".into(), section(color, "analysts")),
        (
            "Recommendation".into(),
            right(
                a.recommendation
                    .as_deref()
                    .map(title_case)
                    .unwrap_or_else(|| f::DASH.into()),
            ),
        ),
        (
            "Target mean / low / high".into(),
            right(format!(
                "{} / {} / {}",
                f::price(a.target_mean),
                f::price(a.target_low),
                f::price(a.target_high)
            )),
        ),
        (
            "Analysts".into(),
            right(
                a.analysts
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| f::DASH.into()),
            ),
        ),
        ("".into(), section(color, "verdict")),
        ("Reading".into(), verdict_cell(fd, color)),
    ];
    out.push_str(&kv_table(color, rows).to_string());
    out.push('\n');
    if let Some(v) = &fd.verdict {
        out.push_str(&summary_line(fd));
        out.push('\n');
        for r in &v.reasons {
            out.push_str(&format!("  • {r}\n"));
        }
    }
    out.push_str(&format!(
        "Fiscal year end {} · latest quarter {} · sources: {}\n",
        date_opt(fd.fiscal_year_end),
        date_opt(fd.most_recent_quarter),
        fd.sources.join(", ")
    ));
    out
}

pub fn compare(list: &[Fundamentals], color: bool) -> String {
    let mut t = base_table(color);
    let mut cols: Vec<&str> = vec!["Metric"];
    for fd in list {
        cols.push(fd.symbol.as_str());
    }
    t.set_header(header(&cols));

    macro_rules! row {
        ($label:expr, $f:expr) => {{
            let mut cells = vec![Cell::new($label).add_attribute(Attribute::Bold)];
            for fd in list {
                cells.push($f(fd));
            }
            t.add_row(cells);
        }};
    }
    row!("Price", |fd: &Fundamentals| right(f::price(fd.price)));
    row!("Market cap", |fd: &Fundamentals| right(f::compact(
        fd.valuation.market_cap
    )));
    row!("P/E (ttm)", |fd: &Fundamentals| right(f::ratio(
        fd.valuation.pe_trailing
    )));
    row!("P/E (fwd)", |fd: &Fundamentals| right(f::ratio(
        fd.valuation.pe_forward
    )));
    row!("PEG", |fd: &Fundamentals| right(f::ratio(fd.valuation.peg)));
    row!("P/B", |fd: &Fundamentals| right(f::ratio(
        fd.valuation.price_to_book
    )));
    row!("P/S", |fd: &Fundamentals| right(f::ratio(
        fd.valuation.price_to_sales
    )));
    row!("EV/EBITDA", |fd: &Fundamentals| right(f::ratio(
        fd.valuation.ev_to_ebitda
    )));
    row!("ROE", |fd: &Fundamentals| right(pct_opt(
        fd.profitability.roe_pct
    )));
    row!("ROA", |fd: &Fundamentals| right(pct_opt(
        fd.profitability.roa_pct
    )));
    row!("Op margin", |fd: &Fundamentals| right(pct_opt(
        fd.profitability.operating_margin_pct
    )));
    row!("Net margin", |fd: &Fundamentals| right(pct_opt(
        fd.profitability.net_margin_pct
    )));
    row!("Debt/equity", |fd: &Fundamentals| right(ratio_x(
        fd.health.debt_to_equity
    )));
    row!("Rev growth", |fd: &Fundamentals| signed(
        f::pct(fd.growth.revenue_growth_pct),
        fd.growth.revenue_growth_pct,
        color
    ));
    row!("EPS growth", |fd: &Fundamentals| signed(
        f::pct(fd.growth.earnings_growth_pct),
        fd.growth.earnings_growth_pct,
        color
    ));
    row!("Div yield", |fd: &Fundamentals| right(pct_opt(
        fd.dividend.yield_pct
    )));
    row!("Payout", |fd: &Fundamentals| right(pct_opt(
        fd.dividend.payout_ratio_pct
    )));
    row!("Beta", |fd: &Fundamentals| right(f::ratio(fd.beta)));
    row!("Target (mean)", |fd: &Fundamentals| right(f::price(
        fd.analyst.target_mean
    )));
    row!("Upside", |fd: &Fundamentals| {
        let up = match (fd.price, fd.analyst.target_mean) {
            (Some(p), Some(t)) if p > 0.0 => Some((t - p) / p * 100.0),
            _ => None,
        };
        signed(f::pct(up), up, color)
    });
    row!("Analysts", |fd: &Fundamentals| right(
        fd.analyst
            .recommendation
            .as_deref()
            .map(title_case)
            .unwrap_or_else(|| f::DASH.into())
    ));
    row!("Verdict", |fd: &Fundamentals| verdict_cell(fd, color));

    let mut out = t.to_string();
    out.push('\n');
    out.push_str("Verdicts are heuristics from public ratios, not investment advice.\n");
    out
}

fn money(v: Option<f64>) -> Cell {
    right(f::compact(v))
}

pub struct StatementFlags {
    pub income: bool,
    pub balance: bool,
    pub cash_flow: bool,
}

pub fn financials(fin: &Financials, flags: &StatementFlags, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} · {} statements · {} periods · {}\n",
        fin.symbol,
        fin.period,
        fin.periods.len(),
        fin.currency.clone().unwrap_or_default()
    ));
    let periods: &[FinancialPeriod] = &fin.periods;
    let mut cols: Vec<String> = vec!["".into()];
    for p in periods {
        cols.push(match fin.period {
            StatementPeriod::Annual => format!("FY{}", p.end_date.format("%Y")),
            _ => p.end_date.format("%Y-%m-%d").to_string(),
        });
    }
    let col_refs: Vec<&str> = cols.iter().map(String::as_str).collect();

    let mut t = base_table(color);
    t.set_header(header(&col_refs));
    macro_rules! line {
        ($label:expr, $get:expr) => {{
            let vals: Vec<Option<f64>> = periods.iter().map($get).collect();
            if vals.iter().any(Option::is_some) {
                let mut cells = vec![Cell::new($label)];
                cells.extend(vals.into_iter().map(money));
                t.add_row(cells);
            }
        }};
    }
    macro_rules! sect {
        ($name:expr) => {{
            let mut cells = vec![section(color, $name)];
            cells.extend(periods.iter().map(|_| Cell::new("")));
            t.add_row(cells);
        }};
    }
    if flags.income {
        sect!("income statement");
        line!("Total revenue", |p: &FinancialPeriod| p
            .income
            .total_revenue);
        line!("Gross profit", |p: &FinancialPeriod| p.income.gross_profit);
        line!("Operating income", |p: &FinancialPeriod| p
            .income
            .operating_income);
        line!("EBITDA", |p: &FinancialPeriod| p.income.ebitda);
        line!("EBIT", |p: &FinancialPeriod| p.income.ebit);
        line!("Interest income", |p: &FinancialPeriod| p
            .income
            .interest_income);
        line!("Interest expense", |p: &FinancialPeriod| p
            .income
            .interest_expense);
        line!("Net interest income", |p: &FinancialPeriod| p
            .income
            .net_interest_income);
        line!("Pretax income", |p: &FinancialPeriod| p
            .income
            .pretax_income);
        line!("Tax", |p: &FinancialPeriod| p.income.tax_provision);
        line!("Net income", |p: &FinancialPeriod| p.income.net_income);
        {
            let vals: Vec<Option<f64>> = periods.iter().map(|p| p.income.eps_diluted).collect();
            if vals.iter().any(Option::is_some) {
                let mut cells = vec![Cell::new("EPS (diluted)")];
                cells.extend(vals.into_iter().map(|v| right(f::price(v))));
                t.add_row(cells);
            }
        }
    }
    if flags.balance && fin.period != StatementPeriod::Ttm {
        sect!("balance sheet");
        line!("Total assets", |p: &FinancialPeriod| p.balance.total_assets);
        line!("Total liabilities", |p: &FinancialPeriod| p
            .balance
            .total_liabilities);
        line!("Shareholders' equity", |p: &FinancialPeriod| p
            .balance
            .stockholders_equity);
        line!("Cash & equivalents", |p: &FinancialPeriod| p
            .balance
            .cash_and_equivalents);
        line!("Total debt", |p: &FinancialPeriod| p.balance.total_debt);
        line!("Net debt", |p: &FinancialPeriod| p.balance.net_debt);
        line!("Shares outstanding", |p: &FinancialPeriod| p
            .balance
            .shares_outstanding);
    }
    if flags.cash_flow {
        sect!("cash flow");
        line!("Operating", |p: &FinancialPeriod| p.cash_flow.operating);
        line!("Investing", |p: &FinancialPeriod| p.cash_flow.investing);
        line!("Financing", |p: &FinancialPeriod| p.cash_flow.financing);
        line!("Capital expenditure", |p: &FinancialPeriod| p
            .cash_flow
            .capital_expenditure);
        line!("Free cash flow", |p: &FinancialPeriod| p
            .cash_flow
            .free_cash_flow);
        line!("Dividends paid", |p: &FinancialPeriod| p
            .cash_flow
            .dividends_paid);
    }
    out.push_str(&t.to_string());
    out.push('\n');
    out.push_str(&format!(
        "Values in {} as reported (T = trillion, B = billion). Source: {}\n",
        fin.currency
            .clone()
            .unwrap_or_else(|| "source currency".into()),
        fin.source
    ));
    out
}

pub fn earnings(e: &Earnings, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} · {} · EPS in {}\n",
        e.symbol,
        e.name.clone().unwrap_or_else(|| f::DASH.into()),
        e.currency.clone().unwrap_or_default()
    ));
    if !e.quarters.is_empty() {
        let mut t = base_table(color);
        t.set_header(header(&[
            "Quarter",
            "Period end",
            "Reported",
            "EPS actual",
            "EPS est.",
            "Surprise",
        ]));
        for q in &e.quarters {
            t.add_row(vec![
                Cell::new(&q.period).add_attribute(Attribute::Bold),
                Cell::new(date_opt(q.end_date)),
                Cell::new(date_opt(q.reported_date)),
                right(f::price(q.eps_actual)),
                right(f::price(q.eps_estimate)),
                signed(f::pct(q.surprise_pct), q.surprise_pct, color),
            ]);
        }
        out.push_str(&t.to_string());
        out.push('\n');
    }
    if !e.yearly.is_empty() || !e.quarterly.is_empty() {
        let mut t = base_table(color);
        t.set_header(header(&["Period", "Revenue", "Earnings", "Margin"]));
        for r in e.yearly.iter().chain(e.quarterly.iter()) {
            let margin = r
                .profit_margin_pct
                .or_else(|| match (r.revenue, r.earnings) {
                    (Some(rev), Some(earn)) if rev != 0.0 => Some(earn / rev * 100.0),
                    _ => None,
                });
            t.add_row(vec![
                Cell::new(&r.period).add_attribute(Attribute::Bold),
                money(r.revenue),
                money(r.earnings),
                right(pct_opt(margin)),
            ]);
        }
        out.push_str(&t.to_string());
        out.push('\n');
    }
    if let Some(n) = &e.next {
        out.push_str(&format!(
            "Next earnings {}{} · EPS consensus {} (range {} – {}, {} analysts) · revenue consensus {}\n",
            date_opt(n.date),
            if n.date_is_estimate { " (estimated)" } else { "" },
            f::price(n.eps_avg),
            f::price(n.eps_low),
            f::price(n.eps_high),
            n.analysts
                .map(|a| a.to_string())
                .unwrap_or_else(|| f::DASH.into()),
            f::compact(n.revenue_avg),
        ));
    }
    if e.ex_dividend_date.is_some() || e.dividend_date.is_some() {
        out.push_str(&format!(
            "Ex-dividend {} · payment {}\n",
            date_opt(e.ex_dividend_date),
            date_opt(e.dividend_date)
        ));
    }
    out.push_str(&format!("Source: {}\n", e.source));
    out
}

fn signal_cell(t: &Technical, color: bool) -> Cell {
    let c = Cell::new(format!(
        "{} (score {:+})",
        t.signal.label.as_str().to_uppercase(),
        t.signal.score
    ))
    .add_attribute(Attribute::Bold);
    if !color {
        return c;
    }
    match t.signal.label {
        SignalLabel::Buy => c.fg(Color::Green),
        SignalLabel::Sell => c.fg(Color::Red),
        SignalLabel::Neutral => c.fg(Color::Yellow),
    }
}

pub fn technical(t: &Technical, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} · {} · {} candles ({}) · as of {}\n",
        t.symbol,
        t.name.clone().unwrap_or_else(|| f::DASH.into()),
        t.candles_used,
        t.period,
        t.as_of.format("%Y-%m-%d")
    ));
    out.push_str(&format!(
        "Price {} · 1d {} · 1w {} · 1m {} · 3m {} · 52w {} – {}\n",
        f::price(Some(t.price)),
        f::pct(t.change_pct_1d),
        f::pct(t.change_pct_1w),
        f::pct(t.change_pct_1m),
        f::pct(t.change_pct_3m),
        f::price(t.low_52w),
        f::price(t.high_52w)
    ));
    let ma = &t.moving_averages;
    let vs = |v: Option<f64>| -> Cell {
        match v {
            Some(x) => {
                let diff = if x != 0.0 {
                    Some((t.price - x) / x * 100.0)
                } else {
                    None
                };
                signed(
                    format!("{} ({})", f::price(Some(x)), f::pct(diff)),
                    diff,
                    color,
                )
            }
            None => right(f::DASH.into()),
        }
    };
    let trend_cell = {
        let c = Cell::new(t.trend.as_str());
        if color {
            match t.trend {
                Trend::Uptrend => c.fg(Color::Green),
                Trend::Downtrend => c.fg(Color::Red),
                _ => c,
            }
        } else {
            c
        }
    };
    let rows: Vec<(String, Cell)> = vec![
        ("Trend".into(), trend_cell),
        ("Signal".into(), signal_cell(t, color)),
        ("".into(), section(color, "momentum")),
        ("RSI (14)".into(), right(f::ratio(t.rsi14))),
        (
            "MACD line / signal / hist".into(),
            match &t.macd {
                Some(m) => signed(
                    format!(
                        "{} / {} / {}",
                        f::num(m.line, 2),
                        f::num(m.signal, 2),
                        f::num(m.histogram, 2)
                    ),
                    Some(m.histogram),
                    color,
                ),
                None => right(f::DASH.into()),
            },
        ),
        ("ATR (14)".into(), right(f::price(t.atr14))),
        ("".into(), section(color, "moving averages (price vs)")),
        ("SMA 20".into(), vs(ma.sma20)),
        ("SMA 50".into(), vs(ma.sma50)),
        ("SMA 200".into(), vs(ma.sma200)),
        ("EMA 20".into(), vs(ma.ema20)),
        ("EMA 50".into(), vs(ma.ema50)),
        ("EMA 200".into(), vs(ma.ema200)),
        ("".into(), section(color, "bollinger (20, 2σ)")),
        (
            "Lower / middle / upper".into(),
            right(match &t.bollinger {
                Some(b) => format!(
                    "{} / {} / {}",
                    f::price(Some(b.lower)),
                    f::price(Some(b.middle)),
                    f::price(Some(b.upper))
                ),
                None => f::DASH.into(),
            }),
        ),
        (
            "%B / bandwidth".into(),
            right(match &t.bollinger {
                Some(b) => format!(
                    "{} / {}%",
                    f::num(b.percent_b, 2),
                    f::num(b.bandwidth_pct, 1)
                ),
                None => f::DASH.into(),
            }),
        ),
        ("".into(), section(color, "volume")),
        (
            "Last / 20d avg".into(),
            right(format!(
                "{} / {} ({})",
                f::int(t.volume.last),
                f::int(t.volume.avg20),
                t.volume
                    .ratio
                    .map(|r| format!("{}x", f::num(r, 2)))
                    .unwrap_or_else(|| f::DASH.into())
            )),
        ),
        ("".into(), section(color, "levels")),
        (
            "Support".into(),
            right(if t.levels.support.is_empty() {
                f::DASH.into()
            } else {
                t.levels
                    .support
                    .iter()
                    .map(|v| f::price(Some(*v)))
                    .collect::<Vec<_>>()
                    .join(" · ")
            }),
        ),
        (
            "Resistance".into(),
            right(if t.levels.resistance.is_empty() {
                f::DASH.into()
            } else {
                t.levels
                    .resistance
                    .iter()
                    .map(|v| f::price(Some(*v)))
                    .collect::<Vec<_>>()
                    .join(" · ")
            }),
        ),
    ];
    out.push_str(&kv_table(color, rows).to_string());
    out.push('\n');
    out.push_str("Why:\n");
    for r in &t.signal.reasons {
        out.push_str(&format!("  • {r}\n"));
    }
    out.push_str("Signal is a rule-based read of daily candles, not investment advice.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fundamentals, Symbol};

    #[test]
    fn wrap_breaks_lines() {
        let w = wrap("one two three four five six", 9);
        assert_eq!(w, "one two\nthree\nfour five\nsix\n");
    }

    #[test]
    fn fundamentals_table_renders_sections_and_dashes() {
        let mut fd = Fundamentals::empty(Symbol::parse("BBCA").unwrap(), "t");
        fd.valuation.pe_trailing = Some(10.96);
        fd.profitability.roe_pct = Some(21.8);
        fd.verdict = Some(crate::analysis::fundamental::verdict(&fd));
        let s = fundamentals(&fd, false);
        assert!(s.contains("VALUATION"));
        assert!(s.contains("10.96"));
        assert!(s.contains("21.80%"));
        assert!(s.contains("insufficient data"));
        assert!(s.contains(f::DASH));
    }

    #[test]
    fn compare_table_has_one_column_per_symbol() {
        let a = Fundamentals::empty(Symbol::parse("BBCA").unwrap(), "t");
        let b = Fundamentals::empty(Symbol::parse("BBRI").unwrap(), "t");
        let s = compare(&[a, b], false);
        assert!(s.contains("BBCA"));
        assert!(s.contains("BBRI"));
        assert!(s.contains("P/E (ttm)"));
    }
}
