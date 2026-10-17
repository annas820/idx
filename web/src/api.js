// JSON API behind the web terminal. Every response is the CLI envelope:
// { schema, provider, fetched_at, cached, stale, warnings?, data } and errors
// are { error: { code, message }, exit_code } with a matching HTTP status.

import * as y from "./yahoo.js";
import * as an from "./analysis.js";
import UNIVERSE from "./universe.json" with { type: "json" };

const NAME_BY_SYMBOL = new Map(UNIVERSE.map(([s, n]) => [s, n]));
const SYMBOLS = UNIVERSE.map(([s]) => s);

export class ApiError extends Error {
  constructor(code, message, status = 400, exit = 2) { super(message); this.code = code; this.status = status; this.exit = exit; }
}
const notFound = (m) => new ApiError("not_found", m, 404, 4);
const usage = (m) => new ApiError("usage", m, 400, 2);
const provider = (m) => new ApiError("provider", m, 502, 3);

export function envelope(schema, data, { provider: p = "yahoo", fetched_at = new Date(), cached = false, stale = false, warnings = [] } = {}) {
  const at = fetched_at instanceof Date ? fetched_at : new Date(fetched_at * 1000);
  const env = { schema, provider: p, fetched_at: at.toISOString().replace(/\.\d{3}Z$/, "Z"), cached, stale };
  if (warnings.length) env.warnings = warnings;
  env.data = data;
  return env;
}

export function parseSymbol(raw) {
  const s = String(raw || "").trim().toUpperCase().replace(/\.JK$/, "");
  if (!s) throw usage("empty symbol");
  if (s.length > 12 || !/^[A-Z0-9-]+$/.test(s)) throw usage(`invalid symbol "${raw}": expected an IDX ticker like BBCA`);
  return s;
}
export function parseSymbols(raw) {
  const out = [];
  for (const p of String(raw || "").split(/[,\s]+/)) if (p) { const s = parseSymbol(p); if (!out.includes(s)) out.push(s); }
  if (!out.length) throw usage("no symbols given");
  return out;
}

// ---- D1 cache ----------------------------------------------------------

async function cached(env, key, ttlSecs, producer) {
  const now = Math.floor(Date.now() / 1000);
  const row = await env.DB.prepare("SELECT v, fetched_at, expires_at FROM cache WHERE k = ?1").bind(key).first();
  if (row && row.expires_at > now) return { value: JSON.parse(row.v), cached: true, stale: false, fetched_at: row.fetched_at };
  try {
    const value = await producer();
    await env.DB.prepare("INSERT OR REPLACE INTO cache (k, v, expires_at, fetched_at) VALUES (?1, ?2, ?3, ?4)")
      .bind(key, JSON.stringify(value), now + ttlSecs, now).run();
    return { value, cached: false, stale: false, fetched_at: now };
  } catch (e) {
    if (row) return { value: JSON.parse(row.v), cached: true, stale: true, fetched_at: row.fetched_at, warning: `live fetch failed (${e.message}); showing cached copy` };
    throw e;
  }
}
async function kvGet(env, k) {
  const row = await env.DB.prepare("SELECT v, updated_at FROM kv WHERE k = ?1").bind(k).first();
  return row ? { value: JSON.parse(row.v), updated_at: row.updated_at } : null;
}
export async function kvPut(env, k, value) {
  await env.DB.prepare("INSERT OR REPLACE INTO kv (k, v, updated_at) VALUES (?1, ?2, ?3)")
    .bind(k, JSON.stringify(value), Math.floor(Date.now() / 1000)).run();
}

// ---- snapshot (cron) ---------------------------------------------------

const WIB_OFFSET_MIN = 7 * 60;
export function marketIsOpen(now = new Date()) {
  const wib = new Date(now.getTime() + WIB_OFFSET_MIN * 60000);
  const day = wib.getUTCDay();
  if (day === 0 || day === 6) return false;
  const mins = wib.getUTCHours() * 60 + wib.getUTCMinutes();
  return mins >= 8 * 60 + 45 && mins <= 16 * 60 + 30;
}

/** Refreshes the quote board (one JSON row) and the index tape. */
export async function refreshSnapshot(env, { force = false } = {}) {
  const now = new Date();
  const minute = Math.floor(now.getTime() / 60000);
  if (!force && !marketIsOpen(now) && minute % 15 !== 0) return { skipped: "market closed" };
  const quotes = [];
  const warnings = [];
  for (let i = 0; i < SYMBOLS.length; i += 190) {
    const batch = SYMBOLS.slice(i, i + 190);
    try {
      for (const q of await y.fetchQuotes(env, batch)) quotes.push(y.parseV7Quote(q));
    } catch (e) {
      warnings.push(`batch ${i / 190 + 1}: ${e.message}`);
    }
  }
  let index = null;
  try {
    const r = await y.fetchQuotes(env, ["^JKSE"]);
    if (r[0]) index = y.parseV7Quote(r[0]);
  } catch (e) {
    warnings.push(`index: ${e.message}`);
  }
  if (!quotes.length) throw provider(`snapshot refresh failed: ${warnings.join("; ")}`);
  const ts = Math.floor(now.getTime() / 1000);
  await kvPut(env, "board", { ts, quotes, index, warnings });
  if (index?.price != null) {
    await env.DB.prepare("INSERT OR REPLACE INTO ticks (symbol, ts, price) VALUES ('^JKSE', ?1, ?2)").bind(minute * 60, index.price).run();
    await env.DB.prepare("DELETE FROM ticks WHERE ts < ?1").bind(ts - 10 * 86400).run();
  }
  return { quotes: quotes.length, index: !!index, warnings };
}

async function board(env) {
  const b = await kvGet(env, "board");
  if (!b) throw provider("no market snapshot yet; the minute refresh has not run");
  return b;
}

// ---- endpoints ---------------------------------------------------------

export async function market(env) {
  const b = await board(env);
  const quotes = b.value.quotes.filter((q) => q.price != null && q.change_pct != null);
  const byChg = [...quotes].sort((a, c) => c.change_pct - a.change_pct);
  const liquid = quotes.filter((q) => (q.volume || 0) > 0 && (q.market_cap || 0) > 1e12);
  const liquidByChg = [...liquid].sort((a, c) => c.change_pct - a.change_pct);
  const byValue = [...quotes].sort((a, c) => ((c.volume || 0) * (c.price || 0)) - ((a.volume || 0) * (a.price || 0)));
  const ticks = await env.DB.prepare("SELECT ts, price FROM ticks WHERE symbol = '^JKSE' AND ts >= ?1 ORDER BY ts")
    .bind(Math.floor(Date.now() / 1000) - 2 * 86400).all();
  const trim = (q) => ({ symbol: q.symbol, name: q.name, price: q.price, change: q.change, change_pct: q.change_pct, volume: q.volume, market_cap: q.market_cap });
  const data = {
    as_of: b.value.quotes.map((q) => q.market_time).filter(Boolean).sort().pop() || null,
    refreshed_at: new Date(b.value.ts * 1000).toISOString().replace(/\.\d{3}Z$/, "Z"),
    market_open: marketIsOpen(),
    index: b.value.index ? trim(b.value.index) : null,
    index_tape: (ticks.results || []).map((t) => [t.ts, t.price]),
    quoted: quotes.length,
    advancing: quotes.filter((q) => q.change_pct > 0).length,
    declining: quotes.filter((q) => q.change_pct < 0).length,
    unchanged: quotes.filter((q) => q.change_pct === 0).length,
    gainers: liquidByChg.slice(0, 10).map(trim),
    losers: liquidByChg.slice(-10).reverse().map(trim),
    most_active: byValue.slice(0, 10).map(trim),
    heavyweights: [...quotes].sort((a, c) => (c.market_cap || 0) - (a.market_cap || 0)).slice(0, 12).map(trim),
    universe: SYMBOLS.length,
  };
  return envelope("idx.market.v1", data, { fetched_at: b.value.ts, cached: true, warnings: b.value.warnings || [] });
}

export async function quote(env, symbolsRaw) {
  const symbols = parseSymbols(symbolsRaw);
  const b = await board(env);
  const bySym = new Map(b.value.quotes.map((q) => [q.symbol, q]));
  const found = symbols.map((s) => bySym.get(s)).filter(Boolean);
  const warnings = symbols.filter((s) => !bySym.has(s)).map((s) => `no data for ${s}`);
  if (!found.length) {
    // Not in the universe: try live once, cached briefly.
    const live = await cached(env, `quote:${symbols.join(",")}`, 60, async () => (await y.fetchQuotes(env, symbols)).map(y.parseV7Quote));
    if (!live.value.length) throw notFound(`no quotes for ${symbols.join(", ")}`);
    return envelope("idx.quotes.v1", live.value, { cached: live.cached, stale: live.stale, fetched_at: live.fetched_at, warnings: symbols.filter((s) => !live.value.some((q) => q.symbol === s)).map((s) => `no data for ${s}`) });
  }
  return envelope("idx.quotes.v1", found, { cached: true, fetched_at: b.value.ts, warnings });
}

const PERIODS = ["1d","5d","1mo","3mo","6mo","1y","2y","5y","10y","ytd","max"];
const INTERVALS = ["5m","15m","30m","1h","1d","1wk","1mo"];

export async function history(env, symbolRaw, period = "1y", interval = "1d", limit = 0) {
  const symbol = parseSymbol(symbolRaw);
  if (!PERIODS.includes(period)) throw usage(`invalid period "${period}"; use ${PERIODS.join(",")}`);
  if (!INTERVALS.includes(interval)) throw usage(`invalid interval "${interval}"; use ${INTERVALS.join(",")}`);
  const intraday = ["5m","15m","30m","1h"].includes(interval);
  const r = await cached(env, `history:${symbol}:${period}:${interval}`, intraday ? 120 : 3600, async () => {
    const res = await y.fetchChart(symbol, period, interval);
    if (res.notFound) return { notFound: res.message };
    return y.parseChart(res.body, period, interval);
  });
  if (r.value.notFound) throw notFound(`${symbol}: ${r.value.notFound}`);
  const data = { ...r.value };
  const warnings = r.warning ? [r.warning] : [];
  if (limit > 0 && data.candles.length > limit) {
    data.candles = data.candles.slice(-limit);
    warnings.push(`candles truncated to the last ${limit}; summary covers the full period`);
  }
  return envelope("idx.history.v1", data, { cached: r.cached, stale: r.stale, fetched_at: r.fetched_at, warnings });
}

async function summary(env, symbol, modules, ttl = 6 * 3600) {
  const r = await cached(env, `summary:${symbol}:${modules}`, ttl, async () => {
    const res = await y.fetchSummary(env, symbol, modules);
    if (res.notFound) return { notFound: res.message };
    return res.body;
  });
  if (r.value?.notFound) throw notFound(`${symbol}: ${r.value.notFound}`);
  return r;
}

export async function profile(env, symbolRaw) {
  const symbol = parseSymbol(symbolRaw);
  const r = await summary(env, symbol, y.FUNDAMENTAL_MODULES);
  const data = y.parseProfile(r.value, symbol);
  if (!data.name) data.name = NAME_BY_SYMBOL.get(symbol) || null;
  return envelope("idx.profile.v1", data, { cached: r.cached, stale: r.stale, fetched_at: r.fetched_at, warnings: r.warning ? [r.warning] : [] });
}

export async function fundamentals(env, symbolRaw) {
  const symbol = parseSymbol(symbolRaw);
  const r = await summary(env, symbol, y.FUNDAMENTAL_MODULES);
  const data = y.parseFundamentals(r.value, symbol);
  data.verdict = an.verdict(data);
  return envelope("idx.fundamentals.v1", data, { cached: r.cached, stale: r.stale, fetched_at: r.fetched_at, warnings: r.warning ? [r.warning] : [] });
}

export async function earnings(env, symbolRaw) {
  const symbol = parseSymbol(symbolRaw);
  const r = await summary(env, symbol, y.EARNINGS_MODULES);
  return envelope("idx.earnings.v1", y.parseEarnings(r.value, symbol), { cached: r.cached, stale: r.stale, fetched_at: r.fetched_at, warnings: r.warning ? [r.warning] : [] });
}

export async function financials(env, symbolRaw, period = "annual") {
  const symbol = parseSymbol(symbolRaw);
  if (!["annual","quarterly","ttm"].includes(period)) throw usage(`invalid period "${period}"; use annual, quarterly or ttm`);
  const r = await cached(env, `financials:${symbol}:${period}`, 24 * 3600, async () => y.parseTimeseries(await y.fetchTimeseries(symbol, period), symbol, period));
  if (r.value.notFound) throw notFound(r.value.message);
  return envelope("idx.financials.v1", r.value, { cached: r.cached, stale: r.stale, fetched_at: r.fetched_at, warnings: r.warning ? [r.warning] : [] });
}

export async function technical(env, symbolRaw, period = "1y") {
  const h = await history(env, symbolRaw, period, "1d", 0);
  const t = an.technical(h.data);
  if (!t) throw notFound(`no daily candles for ${parseSymbol(symbolRaw)}`);
  return envelope("idx.technical.v1", t, { cached: h.cached, stale: h.stale, fetched_at: Date.parse(h.fetched_at) / 1000, warnings: h.warnings || [] });
}

export async function compare(env, symbolsRaw) {
  const symbols = parseSymbols(symbolsRaw);
  if (symbols.length < 2) throw usage("compare needs at least two different symbols");
  const results = await Promise.all(symbols.map((s) => fundamentals(env, s).then((e) => ({ ok: e })).catch((e) => ({ err: e, s }))));
  const data = [], warnings = [];
  let cachedAll = true, stale = false, fetched = Math.floor(Date.now() / 1000);
  for (const r of results) {
    if (r.ok) { data.push(r.ok.data); cachedAll &&= r.ok.cached; stale ||= r.ok.stale; fetched = Math.min(fetched, Date.parse(r.ok.fetched_at) / 1000); }
    else warnings.push(r.err.code === "not_found" ? `no data for ${r.s}` : `${r.s}: ${r.err.message}`);
  }
  if (!data.length) throw results[0].err;
  return envelope("idx.compare.v1", data, { cached: cachedAll, stale, fetched_at: fetched, warnings });
}

export async function screen(env, filterSpec, sort = "mcap", limit = 25, maxFundamentals = 20, descending = true) {
  const filters = an.parseFilters(filterSpec || "");
  const sortField = an.sortField(sort);
  if (!sortField) throw usage(`unknown sort key "${sort}"; use one of ${an.KEY_NAMES}`);
  const b = await board(env);
  let rows = b.value.quotes.map((q) => ({
    symbol: q.symbol, name: q.name || NAME_BY_SYMBOL.get(q.symbol) || null, price: q.price, change_pct: q.change_pct, market_cap: q.market_cap,
    pe_trailing: q.pe_trailing, pe_forward: q.pe_forward, price_to_book: q.price_to_book, dividend_yield_pct: q.dividend_yield_pct,
    eps_ttm: q.eps_ttm, avg_volume_3m: q.avg_volume_3m, week52_high: q.week52_high, week52_low: q.week52_low, fundamentals_checked: false,
    sector: null, industry: null, roe_pct: null, roa_pct: null, net_margin_pct: null, operating_margin_pct: null, debt_to_equity: null,
    revenue_growth_pct: null, earnings_growth_pct: null, peg: null, price_to_sales: null,
  }));
  const quoted = rows.length;
  const quoteFilters = filters.filter((f) => !an.FUNDAMENTAL_FIELDS.has(f.field));
  const fundFilters = filters.filter((f) => an.FUNDAMENTAL_FIELDS.has(f.field));
  rows = rows.filter((r) => an.matches(r, quoteFilters));
  const matchedQuote = rows.length;
  let checked = 0, capped = false;
  const warnings = [];
  if (fundFilters.length || an.FUNDAMENTAL_FIELDS.has(sortField)) {
    an.sortRows(rows, "market_cap", true);
    const cap = Math.max(1, Math.min(maxFundamentals, 40));
    if (rows.length > cap) { capped = true; rows = rows.slice(0, cap); }
    const results = await Promise.all(rows.map((r) => fundamentals(env, r.symbol).then((e) => e.data).catch(() => null)));
    rows.forEach((r, i) => {
      const f = results[i];
      if (!f) return;
      checked++;
      Object.assign(r, {
        fundamentals_checked: true, sector: f.sector, industry: f.industry, roe_pct: f.profitability.roe_pct, roa_pct: f.profitability.roa_pct,
        net_margin_pct: f.profitability.net_margin_pct, operating_margin_pct: f.profitability.operating_margin_pct, debt_to_equity: f.health.debt_to_equity,
        revenue_growth_pct: f.growth.revenue_growth_pct, earnings_growth_pct: f.growth.earnings_growth_pct, peg: f.valuation.peg, price_to_sales: f.valuation.price_to_sales,
      });
      r.pe_trailing ??= f.valuation.pe_trailing; r.price_to_book ??= f.valuation.price_to_book; r.dividend_yield_pct ??= f.dividend.yield_pct;
    });
    rows = rows.filter((r) => an.matches(r, fundFilters));
    if (capped) warnings.push(`fundamentals were fetched for the ${cap} largest companies that passed the quote filters; the web terminal caps this at 40`);
  }
  an.sortRows(rows, sortField, descending);
  const total = rows.length;
  if (limit > 0) rows = rows.slice(0, limit);
  const data = { filters: filters.map((f) => f.raw), sort: sort.toLowerCase(), descending, universe: SYMBOLS.length, quoted, matched_quote_level: matchedQuote, fundamentals_checked: checked, fundamentals_capped: capped, total_matches: total, rows };
  return envelope("idx.screen.v1", data, { cached: true, fetched_at: b.value.ts, warnings });
}

// ---- news ---------------------------------------------------------------

const FEEDS = [
  { id: "kontan/investasi", url: "https://investasi.kontan.co.id/rss" },
  { id: "kontan/keuangan", url: "https://keuangan.kontan.co.id/rss" },
  { id: "kontan/industri", url: "https://industri.kontan.co.id/rss" },
  { id: "cnbcindonesia/market", url: "https://www.cnbcindonesia.com/market/rss" },
  { id: "cnbcindonesia/research", url: "https://www.cnbcindonesia.com/research/rss" },
];
function stripHtml(s) {
  return String(s || "").replace(/<[^>]*>/g, " ").replace(/&nbsp;/g, " ").replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/\s+/g, " ").trim();
}
function cdata(s) { const m = String(s || "").match(/^\s*<!\[CDATA\[([\s\S]*?)\]\]>\s*$/); return m ? m[1] : s; }
export function parseRss(xml, source) {
  const items = [];
  const re = /<item>([\s\S]*?)<\/item>/g;
  let m;
  while ((m = re.exec(xml))) {
    const it = m[1];
    const tag = (n) => { const t = it.match(new RegExp(`<${n}(?:\\s[^>]*)?>([\\s\\S]*?)</${n}>`)); return t ? cdata(t[1]).trim() : ""; };
    const title = stripHtml(tag("title")), url = tag("link").trim();
    if (!title || !url) continue;
    const pub = tag("pubDate");
    const d = pub ? new Date(pub) : null;
    items.push({ title, url, source, published: d && !isNaN(d) ? y.wibIso(Math.floor(d.getTime() / 1000)) : null, summary: stripHtml(tag("description")), matched_by: "all", sentiment: 0, positive_terms: [], negative_terms: [] });
  }
  return items;
}
export function matchName(raw) {
  return String(raw || "").split(/\s+/).map((t) => t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "")).filter((t) => t && !["PT","TBK","PERSERO","LTD","INC"].includes(t.toUpperCase())).join(" ");
}
function itemMatches(item, ticker, name) {
  const text = `${item.title} ${item.summary}`.toUpperCase();
  if (text.split(/[^A-Z0-9]+/).includes(ticker)) return "ticker";
  if (name && name.length >= 4 && text.includes(name.toUpperCase())) return "name";
  return null;
}
export async function news(env, symbolRaw, limit = 20) {
  const symbol = symbolRaw ? parseSymbol(symbolRaw) : null;
  const results = await Promise.all(FEEDS.map((f) => cached(env, `feed:${f.id}`, 15 * 60, async () => {
    const r = await fetch(f.url, { headers: { "User-Agent": "Mozilla/5.0 (idx web terminal)", Accept: "application/rss+xml, application/xml;q=0.9, */*;q=0.8" } });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    return parseRss(await r.text(), f.id);
  }).then((r) => ({ f, r })).catch((e) => ({ f, e }))));
  let items = [], cachedAll = true, stale = false;
  const warnings = [];
  let okCount = 0, fetched = Math.floor(Date.now() / 1000);
  for (const x of results) {
    if (x.e) { warnings.push(`${x.f.id}: ${x.e.message}`); continue; }
    okCount++; cachedAll &&= x.r.cached; stale ||= x.r.stale; fetched = Math.min(fetched, x.r.fetched_at);
    items.push(...x.r.value);
  }
  if (!okCount) throw provider(`no news feed could be read: ${warnings.join("; ")}`);
  items.sort((a, b) => String(b.published).localeCompare(String(a.published)));
  const seen = new Set(); items = items.filter((i) => !seen.has(i.url) && seen.add(i.url));
  const scanned = items.length;
  const matchedName = symbol ? matchName(NAME_BY_SYMBOL.get(symbol) || "") || null : null;
  if (symbol) items = items.filter((it) => { const how = itemMatches(it, symbol, matchedName); if (how) it.matched_by = how; return !!how; });
  for (const it of items) { const s = an.scoreText(`${it.title} ${it.summary}`); it.sentiment = s.score; it.positive_terms = s.pos; it.negative_terms = s.neg; }
  const all = items;
  if (limit > 0) items = items.slice(0, limit);
  const data = { symbol, matched_name: matchedName, sources: FEEDS.map((f) => f.id), scanned, items };
  return { env: envelope("idx.news.v1", data, { provider: "news", cached: cachedAll, stale, fetched_at: fetched, warnings }), all, matchedName };
}
export async function sentiment(env, symbolRaw) {
  const symbol = parseSymbol(symbolRaw);
  const n = await news(env, symbol, 0);
  const rep = an.aggregateSentiment(symbol, n.matchedName, n.all, 10);
  return envelope("idx.sentiment.v1", rep, { provider: "news", cached: n.env.cached, stale: n.env.stale, fetched_at: Date.parse(n.env.fetched_at) / 1000, warnings: n.env.warnings || [] });
}

// ---- ownership (synced register) --------------------------------------

async function ownershipRelease(env, asOf) {
  const row = asOf
    ? await env.DB.prepare("SELECT as_of FROM own_releases WHERE as_of = ?1").bind(asOf).first()
    : await env.DB.prepare("SELECT as_of FROM own_releases ORDER BY as_of DESC LIMIT 1").first();
  if (!row) {
    const have = (await env.DB.prepare("SELECT as_of FROM own_releases ORDER BY as_of").all()).results.map((r) => r.as_of);
    throw notFound(have.length ? `no ownership release for ${asOf}; available: ${have.join(", ")}` : "no ownership data loaded on the web terminal yet; the CLI can import the monthly KSEI file locally (idx ownership import --latest)");
  }
  return row.as_of;
}
const holdingRow = (r) => ({ as_of: r.as_of, symbol: r.symbol, issuer_name: r.issuer_name, holder: r.holder, holder_raw: r.holder_raw, entity_key: r.entity_key, investor_type: r.investor_type, local_foreign: r.local_foreign, nationality: null, domicile: null, shares_scripless: null, shares_scrip: null, shares_total: r.shares_total, pct: r.pct });

export async function ownershipTicker(env, symbolRaw, asOf) {
  const symbol = parseSymbol(symbolRaw);
  const date = await ownershipRelease(env, asOf);
  const rows = (await env.DB.prepare("SELECT * FROM own_holdings WHERE as_of = ?1 AND symbol = ?2 ORDER BY pct DESC").bind(date, symbol).all()).results;
  if (!rows.length) throw notFound(`no holders above 1% recorded for ${symbol} as of ${date}`);
  const holders = rows.map(holdingRow);
  return envelope("idx.ownership.holders.v1", { symbol, issuer_name: rows[0].issuer_name, as_of: date, listed_pct: holders.reduce((s, h) => s + h.pct, 0), holders }, { provider: "local-db", cached: true });
}
export async function ownershipEntity(env, query, asOf) {
  if (!query || query.trim().length < 3) throw usage("entity name needs at least 3 characters");
  const date = await ownershipRelease(env, asOf);
  const key = query.toUpperCase().replace(/[^A-Z0-9]+/g, " ").split(" ").filter((t) => t && !["PT","TBK","PERSERO"].includes(t)).join(" ");
  const rows = (await env.DB.prepare("SELECT * FROM own_holdings WHERE as_of = ?1 AND entity_key LIKE ?2 ORDER BY pct DESC").bind(date, `%${key}%`).all()).results;
  if (!rows.length) throw notFound(`no holder matching "${query}" as of ${date}`);
  const entities = [...new Set(rows.map((r) => r.holder))].sort();
  return envelope("idx.ownership.entity.v1", { query, as_of: date, entities, holdings: rows.map(holdingRow) }, { provider: "local-db", cached: true });
}
export async function ownershipReleases(env) {
  const rows = (await env.DB.prepare("SELECT * FROM own_releases ORDER BY as_of").all()).results;
  return envelope("idx.ownership.releases.v1", rows.map((r) => ({ as_of: r.as_of, format: r.format, source: r.source, sha256: "", rows: r.rows, issuers: 0, entities: 0, imported_at: new Date(r.synced_at * 1000).toISOString() })), { provider: "local-db", cached: true, warnings: rows.length ? [] : ["no ownership data loaded on the web terminal yet"] });
}
export async function ownershipChanges(env, from, to, symbolRaw, minChange = 0, limit = 50) {
  if (!from || !to || from >= to) throw usage("--from must be earlier than --to (YYYY-MM-DD)");
  await ownershipRelease(env, from); await ownershipRelease(env, to);
  const symbol = symbolRaw ? parseSymbol(symbolRaw) : null;
  const load = async (d) => {
    const q = symbol ? env.DB.prepare("SELECT symbol, holder, entity_key, investor_type, shares_total, pct FROM own_holdings WHERE as_of = ?1 AND symbol = ?2").bind(d, symbol)
                     : env.DB.prepare("SELECT symbol, holder, entity_key, investor_type, shares_total, pct FROM own_holdings WHERE as_of = ?1").bind(d);
    const m = new Map();
    for (const r of (await q.all()).results) m.set(`${r.symbol}|${r.entity_key}`, r);
    return m;
  };
  const [before, after] = await Promise.all([load(from), load(to)]);
  const changes = [];
  const r4 = (v) => Math.round(v * 1e4) / 1e4;
  for (const [k, a] of after) {
    const b = before.get(k);
    if (b) {
      if (Math.abs(a.pct - b.pct) < 1e-9 && a.shares_total === b.shares_total) continue;
      changes.push({ symbol: a.symbol, holder: a.holder, entity_key: a.entity_key, investor_type: a.investor_type, kind: "changed", pct_from: b.pct, pct_to: a.pct, pct_change: r4(a.pct - b.pct), shares_from: b.shares_total, shares_to: a.shares_total, shares_change: a.shares_total - b.shares_total });
    } else changes.push({ symbol: a.symbol, holder: a.holder, entity_key: a.entity_key, investor_type: a.investor_type, kind: "new", pct_from: null, pct_to: a.pct, pct_change: r4(a.pct), shares_from: null, shares_to: a.shares_total, shares_change: a.shares_total });
  }
  for (const [k, b] of before) if (!after.has(k)) changes.push({ symbol: b.symbol, holder: b.holder, entity_key: b.entity_key, investor_type: b.investor_type, kind: "exited", pct_from: b.pct, pct_to: null, pct_change: r4(-b.pct), shares_from: b.shares_total, shares_to: null, shares_change: -b.shares_total });
  const kept = changes.filter((c) => Math.abs(c.pct_change) + 1e-12 >= minChange).sort((a, b) => Math.abs(b.pct_change) - Math.abs(a.pct_change) || a.symbol.localeCompare(b.symbol));
  return envelope("idx.ownership.changes.v1", { from, to, symbol, min_pct_change: minChange, total_matches: kept.length, changes: limit > 0 ? kept.slice(0, limit) : kept }, { provider: "local-db", cached: true });
}

// ---- router ----------------------------------------------------------

const num = (v, d) => { if (v == null || v === "") return d; const n = Number(v); if (!Number.isFinite(n)) throw usage(`"${v}" is not a number`); return n; };

export async function route(env, url) {
  const p = url.pathname.replace(/^\/api/, "");
  const q = url.searchParams;
  switch (p) {
    case "/market": return market(env);
    case "/quote": return quote(env, q.get("symbols") || q.get("symbol"));
    case "/history": return history(env, q.get("symbol"), q.get("period") || "1y", q.get("interval") || "1d", num(q.get("limit"), 0));
    case "/profile": return profile(env, q.get("symbol"));
    case "/fundamental": return fundamentals(env, q.get("symbol"));
    case "/earnings": return earnings(env, q.get("symbol"));
    case "/financials": return financials(env, q.get("symbol"), q.get("period") || "annual");
    case "/technical": return technical(env, q.get("symbol"), q.get("period") || "1y");
    case "/compare": return compare(env, q.get("symbols"));
    case "/screen": return screen(env, q.get("filter") || "", q.get("sort") || "mcap", num(q.get("limit"), 25), num(q.get("max_fundamentals"), 20), q.get("asc") == null);
    case "/news": return (await news(env, q.get("symbol"), num(q.get("limit"), 20))).env;
    case "/sentiment": return sentiment(env, q.get("symbol"));
    case "/ownership/ticker": return ownershipTicker(env, q.get("symbol"), q.get("as_of"));
    case "/ownership/entity": return ownershipEntity(env, q.get("name"), q.get("as_of"));
    case "/ownership/changes": return ownershipChanges(env, q.get("from"), q.get("to"), q.get("symbol"), num(q.get("min_change"), 0), num(q.get("limit"), 50));
    case "/ownership/releases": return ownershipReleases(env);
    case "/universe": return envelope("idx.universe.v1", UNIVERSE.map(([symbol, name]) => ({ symbol, name })), { provider: "bundled", cached: true });
    default: throw notFound(`no such endpoint ${p}`);
  }
}
