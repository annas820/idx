// The web terminal: parses CLI-shaped commands, calls /api, prints statements.
import { api, fmt, table, wrapScroll, el, sparkline, signClass } from "/app.js";

const out = document.getElementById("out");
const form = document.getElementById("prompt");
const input = document.getElementById("cmd");
const dateline = document.getElementById("dateline");

// ---- command table ---------------------------------------------------------

const COMMANDS = [
  { name: "quote", usage: "quote <SYMBOLS>", help: "delayed quotes, e.g. quote BBCA,BBRI" },
  { name: "history", usage: "history <SYMBOL> [period] [interval]", help: "OHLCV candles; periods 1d 5d 1mo 3mo 6mo 1y 2y 5y 10y ytd max, intervals 5m 15m 30m 1h 1d 1wk 1mo" },
  { name: "profile", usage: "profile <SYMBOL>", help: "sector, industry, description, size" },
  { name: "financials", usage: "financials <SYMBOL> [annual|quarterly|ttm]", help: "income statement, balance sheet, cash flow" },
  { name: "earnings", usage: "earnings <SYMBOL>", help: "quarterly EPS vs estimates, next date" },
  { name: "fundamental", usage: "fundamental <SYMBOL>", help: "ratios with a plain-language verdict" },
  { name: "technical", usage: "technical <SYMBOL> [period]", help: "RSI, MACD, averages, levels, signal" },
  { name: "compare", usage: "compare <SYMBOLS>", help: "side-by-side fundamentals" },
  { name: "screen", usage: "screen <FILTER> [--sort KEY] [--limit N]", help: 'e.g. screen pe<10,div>5,mcap>5T --sort div' },
  { name: "news", usage: "news [SYMBOL] [--limit N]", help: "Kontan and CNBC Indonesia headlines" },
  { name: "sentiment", usage: "sentiment <SYMBOL>", help: "lexicon sentiment over matching headlines" },
  { name: "market", usage: "market", help: "IHSG, breadth, gainers, losers, most traded" },
  { name: "ownership", usage: "ownership ticker <SYMBOL> | entity <NAME> | changes <FROM> <TO> [SYMBOL] | releases", help: "KSEI holders above 1 % (when a register is loaded)" },
  { name: "json", usage: "json <command…>", help: "print the raw JSON envelope of any command" },
  { name: "help", usage: "help [command]", help: "this list" },
  { name: "clear", usage: "clear", help: "clear the sheet" },
];
const ALIASES = { q: "quote", h: "history", f: "fundamental", t: "technical", n: "news", s: "screen", c: "compare", "?": "help", cls: "clear", own: "ownership" };

function tokenize(line) {
  const toks = [];
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g;
  let m;
  while ((m = re.exec(line))) toks.push(m[1] ?? m[2] ?? m[3]);
  return toks;
}
function flags(toks) {
  const pos = [], opt = {};
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    if (t.startsWith("--")) {
      const [k, v] = t.slice(2).split("=");
      if (v != null) opt[k] = v;
      else if (i + 1 < toks.length && !toks[i + 1].startsWith("--")) opt[k] = toks[++i];
      else opt[k] = true;
    } else pos.push(t);
  }
  return { pos, opt };
}
const qs = (o) => Object.entries(o).filter(([, v]) => v != null && v !== "").map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join("&");

// ---- rendering --------------------------------------------------------------

function kv(pairs) {
  const dl = el("dl", { class: "kv" });
  for (const [k, v] of pairs) if (v !== undefined) dl.append(el("dt", { text: k }), el("dd", { text: v == null ? "—" : String(v) }));
  return dl;
}
const pct = (v) => fmt.pctPlain(v);
const x = (v) => (v == null ? "—" : `${fmt.num(v, 2)}x`);

const R = {
  quotes(d) {
    return wrapScroll(table([
      { h: "Symbol", k: "symbol", l: true, cls: "sym" }, { h: "Name", k: "name", l: true, cls: "name" },
      { h: "Price", f: (r) => fmt.price(r.price), cls: "num" }, { h: "Chg", f: (r) => fmt.change(r.change), cls: (r) => `num ${signClass(r.change)}` },
      { h: "Chg %", f: (r) => fmt.pct(r.change_pct), cls: (r) => `num ${signClass(r.change_pct)}` }, { h: "Open", f: (r) => fmt.price(r.open) },
      { h: "High", f: (r) => fmt.price(r.day_high) }, { h: "Low", f: (r) => fmt.price(r.day_low) }, { h: "Volume", f: (r) => fmt.int(r.volume) },
      { h: "Mkt Cap", f: (r) => fmt.compact(r.market_cap) }, { h: "P/E", f: (r) => fmt.ratio(r.pe_trailing) }, { h: "Div %", f: (r) => pct(r.dividend_yield_pct) },
    ], d));
  },
  history(d) {
    const rows = d.candles.slice(-40);
    const frag = document.createDocumentFragment();
    if (d.summary) {
      const s = d.summary;
      frag.append(el("p", { class: "prose", text: `${d.symbol} · ${d.name || ""} · ${d.period} / ${d.interval} · ${s.candles} candles · close ${fmt.price(s.first_close)} → ${fmt.price(s.last_close)} (${fmt.pct(s.change_pct)}) · high ${fmt.price(s.high)} · low ${fmt.price(s.low)}` }));
      frag.append(sparkline(d.candles.map((c, i) => [i, c.close]), { cls: "spark" }));
    }
    let prev = d.candles.length > 40 ? d.candles[d.candles.length - 41].close : null;
    const withChg = rows.map((c) => { const chg = prev ? ((c.close - prev) / prev) * 100 : null; prev = c.close; return { ...c, chg }; });
    frag.append(wrapScroll(table([
      { h: d.interval.endsWith("m") || d.interval === "1h" ? "Time (WIB)" : "Date", f: (c) => (d.interval.endsWith("m") || d.interval === "1h" ? fmt.wib(c.time).replace(" WIB", "") : fmt.date(c.time)), l: true },
      { h: "Open", f: (c) => fmt.price(c.open) }, { h: "High", f: (c) => fmt.price(c.high) }, { h: "Low", f: (c) => fmt.price(c.low) }, { h: "Close", f: (c) => fmt.price(c.close) },
      { h: "Chg %", f: (c) => fmt.pct(c.chg), cls: (c) => `num ${signClass(c.chg)}` }, { h: "Volume", f: (c) => fmt.int(c.volume) },
    ], withChg)));
    if (d.candles.length > 40) frag.append(el("p", { class: "warn", text: `Showing the last 40 of ${d.candles.length} candles; use json history … for all.` }));
    return frag;
  },
  profile(p) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", html: `<b>${p.name || p.symbol}</b>` }));
    frag.append(kv([["Sector", p.sector], ["Industry", p.industry], ["Exchange", p.exchange], ["Market cap", fmt.compact(p.market_cap)], ["Shares outstanding", fmt.int(p.shares_outstanding)], ["Employees", fmt.int(p.employees)], ["Headquarters", [p.city, p.country].filter(Boolean).join(", ") || null], ["Website", p.website]]));
    if (p.summary) frag.append(el("p", { class: "prose", text: p.summary }));
    if (p.directors?.length) frag.append(el("p", { class: "prose", text: `Board and management: ${p.directors.slice(0, 8).map((d) => d.title ? `${d.name} (${d.title})` : d.name).join("; ")}${p.directors.length > 8 ? "; …" : ""}` }));
    return frag;
  },
  fundamentals(f) {
    const frag = document.createDocumentFragment();
    const v = f.valuation, p = f.profitability, h = f.health, g = f.growth, d = f.dividend, a = f.analyst;
    frag.append(el("p", { class: "prose", html: `<b>${f.name || f.symbol}</b> · ${f.sector || "—"}${f.industry ? " / " + f.industry : ""} · price ${fmt.price(f.price)} · 52w ${fmt.price(f.week52_low)} – ${fmt.price(f.week52_high)} · beta ${fmt.ratio(f.beta)}` }));
    const grid = el("div", { class: "three" });
    const stmt = (title, pairs) => el("div", { class: "stmt" }, [el("h3", { class: "stmt-title", text: title }), kv(pairs)]);
    grid.append(
      stmt("Valuation", [["Market cap", fmt.compact(v.market_cap)], ["Enterprise value", fmt.compact(v.enterprise_value)], ["P/E trailing", fmt.ratio(v.pe_trailing)], ["P/E forward", fmt.ratio(v.pe_forward)], ["PEG", fmt.ratio(v.peg)], ["P/B", fmt.ratio(v.price_to_book)], ["P/S", fmt.ratio(v.price_to_sales)], ["EV/EBITDA", fmt.ratio(v.ev_to_ebitda)]]),
      stmt("Profitability & health", [["ROE", pct(p.roe_pct)], ["ROA", pct(p.roa_pct)], ["Gross margin", pct(p.gross_margin_pct)], ["Operating margin", pct(p.operating_margin_pct)], ["Net margin", pct(p.net_margin_pct)], ["Debt / equity", x(h.debt_to_equity)], ["Current ratio", fmt.ratio(h.current_ratio)], ["Total cash", fmt.compact(h.total_cash)]]),
      stmt("Growth, dividend, analysts", [["Revenue growth", fmt.pct(g.revenue_growth_pct)], ["Earnings growth", fmt.pct(g.earnings_growth_pct)], ["Dividend yield", pct(d.yield_pct)], ["Payout", pct(d.payout_ratio_pct)], ["Ex-dividend", d.ex_dividend_date], ["EPS (ttm)", fmt.price(f.per_share.eps_ttm)], ["Rating", a.recommendation ? a.recommendation.replace("_", " ") : null], ["Target mean", fmt.price(a.target_mean)]]),
    );
    frag.append(grid);
    if (f.verdict) {
      const vv = f.verdict;
      const cls = vv.label === "undervalued" ? "up" : vv.label === "overvalued" ? "dn" : "";
      frag.append(el("p", { class: "prose", style: "margin-top:.75rem" }, [el("span", { class: `pill ${cls}`, text: vv.label.replace("_", " ") }), ` score ${vv.score > 0 ? "+" : ""}${vv.score}, ${vv.inputs_available} inputs · heuristic from public ratios, not investment advice`]));
      frag.append(el("ul", { class: "reasons" }, vv.reasons.map((r) => el("li", { text: r }))));
    }
    frag.append(el("p", { class: "warn", text: `Sources: ${f.sources.join(", ")}` }));
    return frag;
  },
  technical(t) {
    const frag = document.createDocumentFragment();
    const ma = t.moving_averages;
    const vs = (v) => (v == null ? "—" : `${fmt.price(v)} (${fmt.pct(v ? ((t.price - v) / v) * 100 : null)})`);
    const cls = t.signal.label === "buy" ? "up" : t.signal.label === "sell" ? "dn" : "";
    frag.append(el("p", { class: "prose", html: `<b>${t.name || t.symbol}</b> · ${t.candles_used} candles (${t.period}) · as of ${fmt.date(t.as_of)} · price ${fmt.price(t.price)} · 1d ${fmt.pct(t.change_pct_1d)} · 1m ${fmt.pct(t.change_pct_1m)} · 3m ${fmt.pct(t.change_pct_3m)} · 52w ${fmt.price(t.low_52w)} – ${fmt.price(t.high_52w)}` }));
    frag.append(el("p", { class: "prose" }, [el("span", { class: `pill ${cls}`, text: `${t.signal.label} ${t.signal.score > 0 ? "+" : ""}${t.signal.score}` }), ` · trend ${t.trend}`]));
    const grid = el("div", { class: "three" });
    const stmt = (title, pairs) => el("div", { class: "stmt" }, [el("h3", { class: "stmt-title", text: title }), kv(pairs)]);
    grid.append(
      stmt("Momentum", [["RSI (14)", fmt.ratio(t.rsi14)], ["MACD line", t.macd ? fmt.num(t.macd.line, 2) : null], ["MACD signal", t.macd ? fmt.num(t.macd.signal, 2) : null], ["Histogram", t.macd ? fmt.num(t.macd.histogram, 2) : null], ["ATR (14)", fmt.price(t.atr14)], ["Volume / 20d", t.volume.ratio == null ? null : `${fmt.num(t.volume.ratio, 2)}x`]]),
      stmt("Averages (price vs)", [["SMA 20", vs(ma.sma20)], ["SMA 50", vs(ma.sma50)], ["SMA 200", vs(ma.sma200)], ["EMA 20", vs(ma.ema20)], ["EMA 50", vs(ma.ema50)], ["EMA 200", vs(ma.ema200)]]),
      stmt("Bands & levels", [["Bollinger lower", t.bollinger ? fmt.price(t.bollinger.lower) : null], ["Bollinger upper", t.bollinger ? fmt.price(t.bollinger.upper) : null], ["%B", t.bollinger ? fmt.num(t.bollinger.percent_b, 2) : null], ["Support", t.levels.support.map(fmt.price).join(" · ") || null], ["Resistance", t.levels.resistance.map(fmt.price).join(" · ") || null]]),
    );
    frag.append(grid);
    frag.append(el("ul", { class: "reasons" }, t.signal.reasons.map((r) => el("li", { text: r }))));
    frag.append(el("p", { class: "warn", text: "Rule-based read of daily candles, not investment advice." }));
    return frag;
  },
  financials(f) {
    const cols = [{ h: "", k: "label", l: true }, ...f.periods.map((p, i) => ({ h: f.period === "annual" ? `FY${p.end_date.slice(0, 4)}` : p.end_date, f: (r) => r.vals[i], cls: "num" }))];
    const lines = [
      ["Total revenue", "income", "total_revenue"], ["Gross profit", "income", "gross_profit"], ["Operating income", "income", "operating_income"], ["EBITDA", "income", "ebitda"],
      ["Net interest income", "income", "net_interest_income"], ["Pretax income", "income", "pretax_income"], ["Net income", "income", "net_income"], ["EPS (diluted)", "income", "eps_diluted", "price"],
      ["Total assets", "balance", "total_assets"], ["Total liabilities", "balance", "total_liabilities"], ["Shareholders' equity", "balance", "stockholders_equity"], ["Cash & equivalents", "balance", "cash_and_equivalents"], ["Total debt", "balance", "total_debt"],
      ["Operating cash flow", "cash_flow", "operating"], ["Capital expenditure", "cash_flow", "capital_expenditure"], ["Free cash flow", "cash_flow", "free_cash_flow"], ["Dividends paid", "cash_flow", "dividends_paid"],
    ];
    const rows = lines.map(([label, s, k, kind]) => ({ label, vals: f.periods.map((p) => (kind === "price" ? fmt.price(p[s][k]) : fmt.compact(p[s][k]))) })).filter((r) => r.vals.some((v) => v !== "—"));
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", text: `${f.symbol} · ${f.period} statements · ${f.periods.length} periods · ${f.currency || ""} · T = trillion, B = billion` }));
    frag.append(wrapScroll(table(cols, rows)));
    return frag;
  },
  earnings(e) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", html: `<b>${e.name || e.symbol}</b> · EPS in ${e.currency || "IDR"}` }));
    if (e.quarters.length) frag.append(wrapScroll(table([{ h: "Quarter", k: "period", l: true, cls: "sym" }, { h: "Period end", f: (q) => fmt.date(q.end_date), l: true }, { h: "Reported", f: (q) => fmt.date(q.reported_date), l: true }, { h: "EPS actual", f: (q) => fmt.price(q.eps_actual) }, { h: "EPS est.", f: (q) => fmt.price(q.eps_estimate) }, { h: "Surprise", f: (q) => fmt.pct(q.surprise_pct), cls: (q) => `num ${signClass(q.surprise_pct)}` }], e.quarters)));
    const re = [...e.yearly, ...e.quarterly];
    if (re.length) frag.append(wrapScroll(table([{ h: "Period", k: "period", l: true, cls: "sym" }, { h: "Revenue", f: (r) => fmt.compact(r.revenue) }, { h: "Earnings", f: (r) => fmt.compact(r.earnings) }, { h: "Margin", f: (r) => pct(r.profit_margin_pct) }], re)));
    if (e.next) frag.append(el("p", { class: "prose", text: `Next earnings ${e.next.date || "—"}${e.next.date_is_estimate ? " (estimated)" : ""} · EPS consensus ${fmt.price(e.next.eps_avg)} (${fmt.price(e.next.eps_low)} – ${fmt.price(e.next.eps_high)}, ${e.next.analysts ?? "—"} analysts) · revenue consensus ${fmt.compact(e.next.revenue_avg)}` }));
    if (e.ex_dividend_date) frag.append(el("p", { class: "prose", text: `Ex-dividend ${e.ex_dividend_date} · payment ${e.dividend_date || "—"}` }));
    return frag;
  },
  compare(list) {
    const cols = [{ h: "Metric", k: "label", l: true }, ...list.map((f, i) => ({ h: f.symbol, f: (r) => r.vals[i], cls: "num" }))];
    const rowsDef = [
      ["Price", (f) => fmt.price(f.price)], ["Market cap", (f) => fmt.compact(f.valuation.market_cap)], ["P/E (ttm)", (f) => fmt.ratio(f.valuation.pe_trailing)], ["P/E (fwd)", (f) => fmt.ratio(f.valuation.pe_forward)],
      ["P/B", (f) => fmt.ratio(f.valuation.price_to_book)], ["EV/EBITDA", (f) => fmt.ratio(f.valuation.ev_to_ebitda)], ["ROE", (f) => pct(f.profitability.roe_pct)], ["Net margin", (f) => pct(f.profitability.net_margin_pct)],
      ["Debt/equity", (f) => x(f.health.debt_to_equity)], ["Rev growth", (f) => fmt.pct(f.growth.revenue_growth_pct)], ["Div yield", (f) => pct(f.dividend.yield_pct)], ["Target (mean)", (f) => fmt.price(f.analyst.target_mean)],
      ["Verdict", (f) => f.verdict?.label.replace("_", " ") || "—"],
    ];
    return wrapScroll(table(cols, rowsDef.map(([label, fn]) => ({ label, vals: list.map(fn) }))));
  },
  screen(s) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", text: `${s.filters.join(", ") || "no filters"} · ${s.quoted} of ${s.universe} quoted · ${s.matched_quote_level} passed quote filters${s.fundamentals_checked ? ` · fundamentals checked for ${s.fundamentals_checked}${s.fundamentals_capped ? " (capped)" : ""}` : ""} · ${s.total_matches} match${s.total_matches === 1 ? "" : "es"} · sorted by ${s.sort}` }));
    const withF = s.rows.some((r) => r.fundamentals_checked);
    const cols = [
      { h: "Symbol", k: "symbol", l: true, cls: "sym" }, { h: "Name", k: "name", l: true, cls: "name" }, { h: "Price", f: (r) => fmt.price(r.price) },
      { h: "Chg %", f: (r) => fmt.pct(r.change_pct), cls: (r) => `num ${signClass(r.change_pct)}` }, { h: "Mkt Cap", f: (r) => fmt.compact(r.market_cap) },
      { h: "P/E", f: (r) => fmt.ratio(r.pe_trailing) }, { h: "P/B", f: (r) => fmt.ratio(r.price_to_book) }, { h: "Div %", f: (r) => pct(r.dividend_yield_pct) },
    ];
    if (withF) cols.push({ h: "ROE", f: (r) => pct(r.roe_pct) }, { h: "Net mgn", f: (r) => pct(r.net_margin_pct) }, { h: "D/E", f: (r) => x(r.debt_to_equity) }, { h: "Sector", k: "sector", l: true });
    frag.append(wrapScroll(table(cols, s.rows)));
    if (s.rows.length < s.total_matches) frag.append(el("p", { class: "warn", text: `Showing ${s.rows.length} of ${s.total_matches}; add --limit 0 for all.` }));
    return frag;
  },
  news(n) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", text: n.symbol ? `${n.items.length} of ${n.scanned} scanned headlines mention ${n.symbol}${n.matched_name ? ` or ${n.matched_name}` : ""} · ${n.sources.join(", ")}` : `${n.items.length} of ${n.scanned} headlines · ${n.sources.join(", ")}` }));
    frag.append(wrapScroll(table([
      { h: "WIB", f: (i) => (i.published ? fmt.wib(i.published).slice(5, 16) : "—"), l: true }, { h: "Source", k: "source", l: true },
      { h: "±", f: (i) => (i.sentiment > 0 ? "+" : i.sentiment < 0 ? "−" : "·"), cls: (i) => signClass(i.sentiment) },
      { h: "Headline", f: (i) => el("a", { href: i.url, target: "_blank", rel: "noopener", text: i.title }), l: true },
    ], n.items)));
    frag.append(el("p", { class: "warn", text: "± is a lexicon read of the headline, not a recommendation." }));
    return frag;
  },
  sentiment(s) {
    const frag = document.createDocumentFragment();
    const cls = s.label === "positive" ? "up" : s.label === "negative" ? "dn" : "";
    frag.append(el("p", { class: "prose" }, [el("span", { class: `pill ${cls}`, text: s.label.replace("_", " ") }), ` score ${s.score > 0 ? "+" : ""}${fmt.num(s.score, 2)} · ${s.items} headlines: ${s.positive} positive, ${s.negative} negative, ${s.neutral} neutral${s.oldest ? ` · ${fmt.date(s.oldest)} → ${fmt.date(s.newest)}` : ""}`]));
    const terms = (l) => (l.length ? l.map(([t, n]) => `${t} ×${n}`).join(", ") : "—");
    frag.append(el("p", { class: "prose", text: `Terms · positive: ${terms(s.top_positive_terms)} · negative: ${terms(s.top_negative_terms)}` }));
    if (s.headlines.length) frag.append(R.news({ symbol: s.symbol, matched_name: s.matched_name, items: s.headlines, scanned: s.items, sources: [] }).lastChild.previousSibling);
    frag.append(el("p", { class: "warn", text: "Lexicon count over Indonesian and English finance terms. Indicative only." }));
    return frag;
  },
  market(m) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", html: `<b>IHSG ${fmt.price(m.index?.price)}</b> ${fmt.change(m.index?.change)} (${fmt.pct(m.index?.change_pct)}) · advancing ${m.advancing} · declining ${m.declining} · unchanged ${m.unchanged} · ${m.quoted} quoted · snapshot ${fmt.wib(m.refreshed_at)} · ${m.market_open ? "market open" : "market closed"}` }));
    if (m.index_tape.length > 1) frag.append(sparkline(m.index_tape, { cls: "spark" }));
    const grid = el("div", { class: "three" });
    const mk = (title, rows, valueCol) => el("div", { class: "stmt" }, [el("h3", { class: "stmt-title", text: title }), wrapScroll(table([{ h: "Symbol", k: "symbol", l: true, cls: "sym" }, { h: "Price", f: (r) => fmt.price(r.price) }, valueCol], rows))]);
    const chg = { h: "Chg %", f: (r) => fmt.pct(r.change_pct), cls: (r) => `num ${signClass(r.change_pct)}` };
    grid.append(mk("Gainers", m.gainers, chg), mk("Losers", m.losers, chg), mk("Most traded by value", m.most_active, { h: "Value", f: (r) => fmt.compact((r.volume || 0) * (r.price || 0)) }));
    frag.append(grid);
    return frag;
  },
  holders(h) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", html: `<b>${h.symbol}</b> · ${h.issuer_name || ""} · holders above 1 % as of ${h.as_of} · ${h.holders.length} listed = ${pct(h.listed_pct)}` }));
    frag.append(wrapScroll(table([{ h: "#", f: (r, i) => "", l: true }, { h: "Holder", k: "holder", l: true, cls: "sym" }, { h: "Type", k: "investor_type", l: true }, { h: "L/A", k: "local_foreign", l: true }, { h: "Shares", f: (r) => fmt.int(r.shares_total) }, { h: "%", f: (r) => pct(r.pct) }], h.holders.map((r, i) => ({ ...r, n: i + 1 })))));
    return frag;
  },
  entity(e) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", text: `"${e.query}" · ${e.entities.length} matching holder${e.entities.length === 1 ? "" : "s"} · as of ${e.as_of}: ${e.entities.join("; ")}` }));
    frag.append(wrapScroll(table([{ h: "Symbol", k: "symbol", l: true, cls: "sym" }, { h: "Issuer", k: "issuer_name", l: true }, { h: "Holder (as printed)", k: "holder_raw", l: true }, { h: "Type", k: "investor_type", l: true }, { h: "Shares", f: (r) => fmt.int(r.shares_total) }, { h: "%", f: (r) => pct(r.pct) }], e.holdings)));
    return frag;
  },
  changes(c) {
    const frag = document.createDocumentFragment();
    frag.append(el("p", { class: "prose", text: `Changes ${c.from} → ${c.to}${c.symbol ? ` · ${c.symbol}` : ""} · ${c.total_matches} match${c.total_matches === 1 ? "" : "es"}` }));
    frag.append(wrapScroll(table([{ h: "Symbol", k: "symbol", l: true, cls: "sym" }, { h: "Holder", k: "holder", l: true }, { h: "Kind", k: "kind", l: true }, { h: "% from", f: (r) => pct(r.pct_from) }, { h: "% to", f: (r) => pct(r.pct_to) }, { h: "Δ pp", f: (r) => fmt.change(r.pct_change), cls: (r) => `num ${signClass(r.pct_change)}` }, { h: "Δ shares", f: (r) => fmt.change(r.shares_change), cls: (r) => `num ${signClass(r.shares_change)}` }], c.changes)));
    return frag;
  },
  releases(list) {
    return wrapScroll(table([{ h: "As of", k: "as_of", l: true, cls: "sym" }, { h: "Format", k: "format", l: true }, { h: "Rows", f: (r) => fmt.int(r.rows) }, { h: "Source", k: "source", l: true }], list));
  },
};

// ---- command execution -----------------------------------------------------

function need(pos, n, usage) { if (pos.length < n) throw Object.assign(new Error(`usage: ${usage}`), { code: "usage" }); }

async function run(line) {
  const toks = tokenize(line);
  if (!toks.length) return null;
  let name = toks[0].toLowerCase();
  name = ALIASES[name] || name;
  let rawJson = false;
  if (name === "json") { rawJson = true; toks.shift(); name = (toks[0] || "").toLowerCase(); name = ALIASES[name] || name; }
  const { pos, opt } = flags(toks.slice(1));
  const cmd = COMMANDS.find((c) => c.name === name);
  if (!cmd) throw Object.assign(new Error(`unknown command "${toks[0]}"; type help`), { code: "usage" });
  const sym = (i = 0) => pos[i];
  let env, render;
  switch (name) {
    case "help": return { help: pos[0] };
    case "clear": return { clear: true };
    case "quote": need(pos, 1, cmd.usage); env = await api(`quote?symbols=${encodeURIComponent(pos.join(","))}`); render = R.quotes; break;
    case "history": need(pos, 1, cmd.usage); env = await api(`history?${qs({ symbol: sym(), period: pos[1] || opt.period || "1y", interval: pos[2] || opt.interval || "1d", limit: rawJson ? opt.limit || 0 : 0 })}`); render = R.history; break;
    case "profile": need(pos, 1, cmd.usage); env = await api(`profile?symbol=${encodeURIComponent(sym())}`); render = R.profile; break;
    case "financials": need(pos, 1, cmd.usage); env = await api(`financials?${qs({ symbol: sym(), period: pos[1] || opt.period || "annual" })}`); render = R.financials; break;
    case "earnings": need(pos, 1, cmd.usage); env = await api(`earnings?symbol=${encodeURIComponent(sym())}`); render = R.earnings; break;
    case "fundamental": need(pos, 1, cmd.usage); env = await api(`fundamental?symbol=${encodeURIComponent(sym())}`); render = R.fundamentals; break;
    case "technical": need(pos, 1, cmd.usage); env = await api(`technical?${qs({ symbol: sym(), period: pos[1] || opt.period || "1y" })}`); render = R.technical; break;
    case "compare": need(pos, 1, cmd.usage); env = await api(`compare?symbols=${encodeURIComponent(pos.join(","))}`); render = R.compare; break;
    case "screen": need(pos, 1, cmd.usage); env = await api(`screen?${qs({ filter: pos.join(","), sort: opt.sort, limit: opt.limit, max_fundamentals: opt["max-fundamentals"], asc: opt.asc ? "1" : null })}`); render = R.screen; break;
    case "news": env = await api(`news?${qs({ symbol: sym(), limit: opt.limit || 20 })}`); render = R.news; break;
    case "sentiment": need(pos, 1, cmd.usage); env = await api(`sentiment?symbol=${encodeURIComponent(sym())}`); render = R.sentiment; break;
    case "market": env = await api("market"); render = R.market; break;
    case "ownership": {
      const sub = (pos[0] || "").toLowerCase();
      if (sub === "ticker") { need(pos, 2, cmd.usage); env = await api(`ownership/ticker?${qs({ symbol: pos[1], as_of: opt["as-of"] })}`); render = R.holders; }
      else if (sub === "entity") { need(pos, 2, cmd.usage); env = await api(`ownership/entity?${qs({ name: pos.slice(1).join(" "), as_of: opt["as-of"] })}`); render = R.entity; }
      else if (sub === "changes") { need(pos, 3, cmd.usage); env = await api(`ownership/changes?${qs({ from: pos[1], to: pos[2], symbol: pos[3], min_change: opt["min-change"], limit: opt.limit })}`); render = R.changes; }
      else if (sub === "releases") { env = await api("ownership/releases"); render = R.releases; }
      else throw Object.assign(new Error(`usage: ${cmd.usage}`), { code: "usage" });
      break;
    }
  }
  return { env, render: rawJson ? (d, e) => el("pre", { class: "json", text: JSON.stringify(e, null, 2) }) : render };
}

// ---- sheet ---------------------------------------------------------------------

function entry(cmdLine) {
  const sec = el("section", { class: "entry" });
  const cap = el("div", { class: "caption" }, [el("span", { class: "cmd", text: `idx ${cmdLine}` }), el("span", { class: "meta", text: "…" })]);
  const body = el("div", { class: "body" });
  sec.append(cap, body);
  out.append(sec);
  return { sec, cap, body, meta: cap.lastChild };
}

function renderHelp(which) {
  const frag = document.createDocumentFragment();
  const list = which ? COMMANDS.filter((c) => c.name === (ALIASES[which] || which)) : COMMANDS;
  if (!list.length) frag.append(el("p", { class: "err", text: `no command "${which}"` }));
  frag.append(wrapScroll(table([{ h: "Command", k: "usage", l: true, cls: "sym" }, { h: "What it does", k: "help", l: true }], list.map((c) => ({ usage: c.usage, help: c.help })))));
  frag.append(el("p", { class: "warn", text: "Aliases: q h f t n s c own. Prefix any command with json for the raw envelope. Symbols accept BBCA or BBCA.JK; several symbols are comma or space separated." }));
  return frag;
}

async function execute(line) {
  const e = entry(line);
  input.value = "";
  try {
    const r = await run(line);
    if (!r) { e.sec.remove(); return; }
    if (r.clear) { out.replaceChildren(); return; }
    if ("help" in r) { e.meta.textContent = ""; e.body.append(renderHelp(r.help)); return; }
    const env = r.env;
    e.meta.textContent = `${env.provider} · ${env.cached ? "cached" : "live"}${env.stale ? " · stale" : ""} · fetched ${fmt.wib(env.fetched_at.replace("Z", "+00:00")).replace(" WIB", " UTC")}`;
    for (const w of env.warnings || []) e.body.append(el("p", { class: "warn", text: w }));
    e.body.append(r.render(env.data, env));
  } catch (err) {
    e.meta.textContent = err.code ? `error · ${err.code}` : "error";
    e.body.append(el("p", { class: "err", text: err.message }));
    if (err.code === "usage") e.body.append(el("p", { class: "warn", text: "Type help for the command list." }));
  }
  e.sec.scrollIntoView({ block: "end", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
  input.focus();
}

// history + completion
const hist = [];
let hIdx = -1;
form.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const line = input.value.trim();
  if (!line) return;
  hist.push(line); hIdx = hist.length;
  execute(line);
});
input.addEventListener("keydown", (ev) => {
  if (ev.key === "ArrowUp" && hist.length) { ev.preventDefault(); hIdx = Math.max(0, hIdx - 1); input.value = hist[hIdx]; }
  else if (ev.key === "ArrowDown" && hist.length) { ev.preventDefault(); hIdx = Math.min(hist.length, hIdx + 1); input.value = hist[hIdx] || ""; }
  else if (ev.key === "Tab" && input.value && !input.value.includes(" ")) {
    const hit = COMMANDS.map((c) => c.name).find((n) => n.startsWith(input.value.toLowerCase()));
    if (hit) { ev.preventDefault(); input.value = `${hit} `; }
  }
});

// hints + deep links (?cmd=)
const SAMPLE = ["market", "quote BBCA,BBRI,TLKM", "fundamental BBCA", "technical BBRI", "screen pe<10,div>5,mcap>5T --sort div", "news TLKM", "compare BBCA BBRI BMRI", "history ASII 6mo 1wk", "ownership ticker BBCA", "help"];
document.getElementById("hints").append(...SAMPLE.map((s) => el("button", { type: "button", text: s, onclick: () => { hist.push(s); hIdx = hist.length; execute(s); } })));

api("market").then((m) => { dateline.textContent = `${fmt.wib(m.data.as_of || m.data.refreshed_at)} · ${m.data.market_open ? "market open" : "market closed"} · ${m.data.quoted} quoted`; }).catch(() => { dateline.textContent = "snapshot unavailable"; });

const initial = new URLSearchParams(location.search).get("cmd");
if (initial) execute(initial);
