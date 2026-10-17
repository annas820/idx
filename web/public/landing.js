import { api, fmt, table, wrapScroll, el, sparkline, reink, signClass, copyButton } from "/app.js";

const $ = (id) => document.getElementById(id);

const BOARD_COLS = [
  { h: "Symbol", k: "symbol", l: true, cls: "sym", name: "sym", title: (r) => r.name || "" },
  { h: "Name", k: "name", l: true, cls: "name", name: "name" },
  { h: "Price", f: (r) => fmt.price(r.price), cls: "num" },
  { h: "Chg", f: (r) => fmt.change(r.change), cls: (r) => `num ${signClass(r.change)}` },
  { h: "Chg %", f: (r) => fmt.pct(r.change_pct), cls: (r) => `num ${signClass(r.change_pct)}` },
  { h: "Volume", f: (r) => fmt.int(r.volume), cls: "num" },
  { h: "Mkt Cap", f: (r) => fmt.compact(r.market_cap), cls: "num" },
];
const MOVER_COLS = [
  { h: "Symbol", k: "symbol", l: true, cls: "sym" },
  { h: "Price", f: (r) => fmt.price(r.price), cls: "num" },
  { h: "Chg %", f: (r) => fmt.pct(r.change_pct), cls: (r) => `num ${signClass(r.change_pct)}` },
];
const VALUE_COLS = [
  { h: "Symbol", k: "symbol", l: true, cls: "sym" },
  { h: "Price", f: (r) => fmt.price(r.price), cls: "num" },
  { h: "Value", f: (r) => fmt.compact((r.volume || 0) * (r.price || 0)), cls: "num" },
];

let boardTable = null;

function renderMarket(env) {
  const m = env.data;
  $("dateline").textContent = m.as_of ? `${fmt.wib(m.as_of)} · ${m.market_open ? "market open" : "market closed"}` : `refreshed ${fmt.wib(m.refreshed_at)}`;
  if (m.index) {
    $("ix-px").textContent = fmt.price(m.index.price);
    const chg = $("ix-chg");
    chg.textContent = `${fmt.change(m.index.change)} (${fmt.pct(m.index.change_pct)})`;
    chg.className = `chg ${signClass(m.index.change)}`;
  }
  $("breadth").textContent = `advancing ${m.advancing} · declining ${m.declining} · unchanged ${m.unchanged} · ${m.quoted} quoted`;
  const slot = $("tape-slot");
  const distinct = new Set(m.index_tape.map((p) => p[1])).size;
  slot.replaceChildren(...(distinct > 1 ? [sparkline(m.index_tape)] : []));

  const t = table(BOARD_COLS, m.heavyweights, { key: "symbol" });
  t.setAttribute("aria-label", "Live board: heavyweights by market cap");
  reink(boardTable, t);
  boardTable = t;
  $("board").replaceChildren(wrapScroll(t));
  $("board-foot").replaceChildren(
    el("span", { text: `Snapshot ${fmt.wib(m.refreshed_at)} · ${m.quoted} of ${m.universe} tickers quoted · Yahoo Finance via idx · see Catatan 2 and 3.` }),
  );

  const mk = (title, cols, rows) => el("div", { class: "stmt" }, [el("h3", { class: "stmt-title", text: title }), wrapScroll(table(cols, rows))]);
  $("movers").replaceChildren(
    mk("Gainers", MOVER_COLS, m.gainers),
    mk("Losers", MOVER_COLS, m.losers),
    mk("Most traded by value", VALUE_COLS, m.most_active),
  );
}

async function refresh() {
  try {
    renderMarket(await api("market"));
  } catch (e) {
    $("dateline").textContent = "board unavailable";
    $("board-foot").replaceChildren(el("span", { class: "dn", text: `Live board unavailable: ${e.message}` }));
  }
}

$("copy-install").replaceWith(copyButton($("install-line").textContent));
$("copy-mcp").replaceWith(copyButton($("mcp-line").textContent));
$("today").textContent = new Date().toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "Asia/Jakarta" });

refresh();
setInterval(refresh, 60_000);
document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
