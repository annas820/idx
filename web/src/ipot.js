// Indo Premier (IPOT) broker summary: the web twin of src/providers/ipot.
// Same endpoint and parsing rules, tested on the same tests/fixtures/ipot pages.
// Figures of a million and more arrive abbreviated to one decimal (1.6 M);
// they keep that precision and set `rounded`. A spurious nginx 404 is retried.

import { fetchWithin } from "./net.js";

export const NAME = "ipot";
const ENDPOINT = "https://www.indopremier.com/module/saham/include/data-brokersummary.php";
const FD = { all: "all", foreign: "F", domestic: "D" };
const BOARD = { all: "all", regular: "RG", cash: "TN", negotiated: "NG" };
const KIND = { local: "local", foreign: "foreign", bumn: "state_owned" };
export const INVESTORS = Object.keys(FD);
export const BOARDS = Object.keys(BOARD);

const mdy = (iso) => { const [y, m, d] = iso.split("-"); return `${m}/${d}/${y}`; };
const iso = (m, d, y) => `${y}-${m}-${d}`;

export function url(symbol, from, to, investor = "all", board = "all") {
  return `${ENDPOINT}?code=${symbol}&start=${mdy(from)}&end=${mdy(to)}&fd=${FD[investor]}&board=${BOARD[board]}`;
}

/** "1.6 M" -> { v: 1600000, rounded: true }; "687,067" -> { v: 687067, rounded: false }; "" -> null. */
export function number(text) {
  const t = String(text ?? "").trim().replace(/,/g, "");
  if (!t) return null;
  const mult = { K: 1e3, M: 1e6, B: 1e9, T: 1e12 }[t.slice(-1).toUpperCase()];
  const v = Number((mult ? t.slice(0, -1) : t).trim());
  if (!Number.isFinite(v)) return null;
  return { v: Math.round(v * (mult || 1)), rounded: !!mult };
}

const strip = (s) => s.replace(/<[^>]*>/g, "").replace(/&nbsp;/g, " ").trim();

/** Parses the fragment: { invalid: true } or the summary fields (names left null). */
export function parse(html) {
  if (/invalid stock code/i.test(String(html).trim())) return { invalid: true };
  const a = html.indexOf("<tbody>"), b = html.indexOf("</tbody>");
  if (a < 0 || b < 0) throw new Error("unexpected broker summary page (no table)");
  let rounded = false;
  const num = (text) => { const n = number(text); if (!n) return null; rounded ||= n.rounded; return n.v; };
  const date = (id) => { const m = html.match(new RegExp(`id="${id}"[^>]*?value="(\\d{2})/(\\d{2})/(\\d{4})"`)); return m ? iso(m[1], m[2], m[3]) : null; };
  const buyers = [], sellers = [];
  html.slice(a, b).split("</tr>").forEach((row, i) => {
    const cells = [...row.matchAll(/<td[^>]*>([\s\S]*?)<\/td>/g)].map((m) => ({ cls: (m[1].match(/class="([^"]*)"/) || [])[1] || null, text: strip(m[1]) }));
    if (cells.length < 9) return;
    const rank = Number.parseInt(cells[4].text, 10) || i + 1;
    for (const [side, at] of [[buyers, 0], [sellers, 5]]) {
      const code = cells[at].text.toUpperCase();
      if (!code) continue;
      side.push({ rank, code, name: null, kind: KIND[(cells[at].cls || "").replace(/^text-/, "")] || null, lots: num(cells[at + 1].text), value: num(cells[at + 2].text), average_price: num(cells[at + 3].text) });
    }
  });
  const footer = (label) => { const i = html.indexOf(label); if (i < 0) return null; const rest = html.slice(i + label.length); return num(rest.slice(0, rest.indexOf("<"))); };
  const total_value = footer("T. Val :"), foreign_net_value = footer("F. NVal :"), total_lots = footer("T.Lot :"), avg = footer("Avg :");
  return {
    from: date("startDateBrokerSummary"), to: date("endDateBrokerSummary"), buyers, sellers,
    total_value, total_lots, foreign_net_value,
    // The page prints "Avg : 0" when nothing traded; there is no average then.
    average_price: total_lots > 0 ? avg : null,
    rounded,
  };
}

/** GET with the source's spurious 404 retried (up to 5 tries, doubling pauses); each try has a deadline. */
export async function fetchPage(u, { attempts = 5, pauseMs = 250, fetchFn = fetch, ms } = {}) {
  let r;
  for (let i = 0; i < attempts; i++) {
    if (i) await new Promise((res) => setTimeout(res, pauseMs * 2 ** (i - 1)));
    r = await fetchWithin(u, { headers: { "User-Agent": "Mozilla/5.0 (idx web terminal)", Accept: "text/html" } }, { fetchFn, ms });
    if (r.status !== 404) break;
  }
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.text();
}
