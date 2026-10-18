// node --test web/test/*.mjs (no network). The corporate-action parser in
// src/ipot.js must read the recorded IPOT pages as src/providers/ipot/actions.rs
// does, and the bundled holidays must match data/holidays.csv.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import * as ipot from "../src/ipot.js";
import * as hol from "../src/holidays.js";
import { actions, holidays } from "../src/api.js";
import HOLIDAYS from "../src/holidays.json" with { type: "json" };

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const fixture = (p) => readFileSync(join(root, "tests", "fixtures", "ipot", p), "utf8");
const BBCA = fixture("corporateaction_bbca_20200101.html");
const ENRG = fixture("corporateaction_enrg_20250101.html");
const MEGA = fixture("corporateaction_mega_20250101.html");
const EMPTY = fixture("corporateaction_bbca_since_today_empty.html");
const INVALID = fixture("corporateaction_zzzz_invalid.html");

test("cash dividends and a split, repeats dropped, newest first", () => {
  const p = ipot.parseActions(BBCA);
  const cum = p.actions.map((a) => a.cum_date);
  assert.deepEqual(cum, [...new Set(cum)], "no repeated rows");
  assert.deepEqual(cum, [...cum].sort().reverse(), "newest cum date first");
  const split = p.actions.find((a) => a.kind === "stock_split");
  assert.equal(split.label, "Stock Split");
  assert.deepEqual([split.ratio_old, split.ratio_new, split.amount], [1, 5, null]);
  assert.deepEqual([split.cum_date, split.ex_date, split.end_date], ["2021-10-12", "2021-10-13", "2021-10-15"]);
  // The schedule sits in a table nested inside the announcement cell.
  assert.match(split.details, /Nilai Nominal Baru/);
  assert.match(split.details, /13 Oktober 2021/);
  const div = p.actions.find((a) => a.cum_date === "2021-04-07");
  assert.equal(div.kind, "cash_dividend");
  assert.equal(div.label, "Cash Deviden");
  assert.equal(div.amount, 432);
  assert.deepEqual([div.ratio_old, div.ratio_new], [null, null]);
  assert.deepEqual([div.recording_date, div.distribution_date, div.end_date, div.status], ["2021-04-09", "2021-04-28", null, "Final"]);
  assert.match(div.details, /Rp\. 432/);
  assert.ok(div.details.includes("\n") && !div.details.includes("<"));
  const rups = p.meetings[0];
  assert.deepEqual([rups.date, rups.time], ["2026-03-12", "14:00:00"]);
  assert.match(rups.venue, /Menara BCA/);
  assert.match(rups.agenda, /Agenda RUPS Tahunan/);
  assert.equal(p.warrants_rights.length, 0);
  assert.ok(p.actions.every((a) => a.cum_date !== "2015-09-09"), "nothing from the template block");
});

test("rights issue, listed right and bonus shares", () => {
  const e = ipot.parseActions(ENRG);
  assert.deepEqual(
    { kind: e.actions[0].kind, amount: e.actions[0].amount, ratio: [e.actions[0].ratio_old, e.actions[0].ratio_new], end: e.actions[0].end_date },
    { kind: "rights_issue", amount: 310, ratio: [2, 1], end: "2026-10-21" },
  );
  const w = e.warrants_rights[0];
  assert.deepEqual([w.code, w.kind, w.exercise_price, w.shares, w.listing_date, w.maturity_date], ["ENRG-R", "Right", 310, 13_282_271_875, "2026-10-08", "2026-10-21"]);
  assert.ok(e.meetings.some((m) => m.status === "Pending"));
  const b = ipot.parseActions(MEGA).actions.find((a) => a.kind === "bonus");
  assert.deepEqual([b.ratio_old, b.ratio_new, b.amount, b.cum_date, b.distribution_date], [1, 1, null, "2026-04-09", "2026-04-30"]);
});

test("empty and invalid pages, labels and the URL", () => {
  assert.deepEqual(ipot.parseActions(EMPTY), { actions: [], meetings: [], warrants_rights: [] });
  assert.deepEqual(ipot.parseActions(INVALID), { invalid: true });
  assert.equal(ipot.actionKind("Cash Deviden"), "cash_dividend");
  assert.equal(ipot.actionKind("Stock Dividend"), "stock_dividend");
  assert.equal(ipot.actionKind("Reverse Split"), "reverse_split");
  assert.equal(ipot.actionKind("Right Issue"), "rights_issue");
  assert.equal(ipot.actionKind("Something"), "other");
  assert.equal(ipot.actionsUrl("MEGA", "2025-01-01"), "https://www.indopremier.com/module/saham/include/corporateaction.php?code=MEGA&start=01/01/2025");
});

test("bundled holidays match data/holidays.csv and IDX's trading-day counts", () => {
  const csv = readFileSync(join(root, "data", "holidays.csv"), "utf8").split("\n").map((l) => l.trim())
    .filter((l) => l && !l.startsWith("#") && !l.toLowerCase().startsWith("date,"))
    .map((l) => { const [d, n, ...en] = l.split(","); return [d.trim(), n.trim(), en.join(",").trim()]; });
  assert.deepEqual(HOLIDAYS, csv);
  assert.equal(hol.tradingDays(2026), 239);
  assert.equal(hol.tradingDays(2027), 241);
  assert.equal(hol.weekday("2026-08-17"), "Monday");
  assert.equal(hol.isTradingDay("2026-08-17"), false);
  assert.equal(hol.nextTradingDay("2026-08-14"), "2026-08-18");
  assert.equal(hol.nextTradingDay("2026-03-19"), "2026-03-25");
  const up = hol.holidays(2026, true, "2026-10-18");
  assert.deepEqual(up.data.holidays.map((h) => h.date), ["2026-12-24", "2026-12-25", "2026-12-31"]);
  assert.equal(up.data.next_trading_day, "2026-10-19");
  assert.equal(up.data.today_is_trading_day, false);
  const none = hol.holidays(2030, false, "2026-10-18");
  assert.equal(none.data.trading_days, null);
  assert.match(none.warnings[0], /no holiday list for 2030/);
});

test("/api/holidays envelope", () => {
  const out = holidays("2027");
  assert.equal(out.schema, "idx.holidays.v1");
  assert.equal(out.provider, "bundled");
  assert.equal(out.data.trading_days, 241);
  assert.equal(out.data.holidays[0].name_en, "New Year 2027");
  assert.equal(holidays(null, "1").data.year, null);
  assert.throws(() => holidays("10000"), (e) => e.code === "usage");
});

// Minimal D1 stand-in: every read misses, writes succeed.
const env = { DB: { prepare: () => ({ bind: () => ({ first: async () => null, run: async () => ({}) }) }) } };

test("the /api/actions envelope", async () => {
  const realFetch = globalThis.fetch;
  const asked = [];
  globalThis.fetch = async (u) => { asked.push(String(u)); return new Response(String(u).includes("code=ZZZZ") ? INVALID : ENRG, { status: 200 }); };
  try {
    const out = await actions(env, "enrg", "2025-01-01");
    assert.equal(out.schema, "idx.actions.v1");
    assert.equal(out.provider, "ipot");
    assert.equal(out.data.symbol, "ENRG");
    assert.equal(out.data.name, "ENERGI MEGA PERSADA Tbk");
    assert.equal(out.data.since, "2025-01-01");
    assert.equal(out.data.warrants_rights[0].code, "ENRG-R");
    assert.ok(asked[0].endsWith("code=ENRG&start=01/01/2025"));
    await assert.rejects(actions(env, "ZZZZ", "2025-01-01"), (e) => e.code === "not_found");
    await assert.rejects(actions(env, "^JKSE"), (e) => e.code === "usage");
    await assert.rejects(actions(env, "BBCA", "2025/01/01"), (e) => e.code === "usage");
  } finally {
    globalThis.fetch = realFetch;
  }
});
