// node --test web/test/*.mjs (no network). src/bandar.js must read the
// recorded IPOT pages exactly like src/analysis/bandar.rs: the same numbers
// and the same reasons text (the Rust tests pin the same strings).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import * as ipot from "../src/ipot.js";
import * as bd from "../src/bandar.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const page = (f) => readFileSync(join(root, "tests", "fixtures", "ipot", f), "utf8");
const load = (f, symbol = "BBCA") => ({ symbol, name: null, investor: "all", board: "all", ...ipot.parse(page(f)) });
const DAY = load("brokersummary_bbca_20261016.html");
const WEEK = load("brokersummary_bbca_20261012_20261016.html");
const EMPTY = load("brokersummary_bbca_20261017_no_trades.html");

test("signals and phases follow the table", () => {
  assert.equal(bd.signal(15), "big_accumulation");
  assert.equal(bd.signal(14.99), "accumulation");
  assert.equal(bd.signal(5), "accumulation");
  assert.equal(bd.signal(4.99), "neutral");
  assert.equal(bd.signal(-5), "distribution");
  assert.equal(bd.signal(-15), "big_distribution");
  assert.equal(bd.signal(null), "neutral");
  assert.equal(bd.phase("accumulation", 2), "quiet_accumulation");
  assert.equal(bd.phase("accumulation", 2.01), "markup");
  assert.equal(bd.phase("big_distribution", -2), "quiet_distribution");
  assert.equal(bd.phase("big_distribution", -14.5), "markdown");
  assert.equal(bd.phase("big_distribution", null), null);
  assert.equal(bd.phase("neutral", null), "neutral");
});

test("tiers of one session", () => {
  assert.equal(bd.tier(DAY, 1).buy_value, 841_200_000_000);
  const t5 = bd.tier(DAY, 5);
  assert.ok(Math.abs(t5.net_pct - -24.3) < 0.1, t5.net_pct);
  assert.equal(t5.signal, "big_distribution");
  assert.ok(t5.buy_average > 5000 && t5.buy_average < 6000);
  assert.ok(bd.tier(DAY, 10).buy_share_pct > t5.buy_share_pct);
});

test("the week reads like the Rust side, reasons included", () => {
  const r = bd.report(WEEK, "1w", 5, 5175, -14.46);
  assert.equal(r.signal, "big_distribution");
  assert.equal(r.phase, "markdown");
  assert.deepEqual(r.tiers.map((t) => t.top), [1, 3, 5, 10]);
  assert.equal(r.buyers.length, 5);
  assert.ok(Math.abs(r.foreign_net_pct - -41.4) < 0.1);
  assert.deepEqual(r.reasons, [
    "Top 5 buyers bought 4.23T, top 5 sellers sold 6.16T: net -1.93T, -19.5% of the 9.90T traded (big distribution, at -15% or less).",
    "Net by tier: top 1 -17.5%, top 3 -18.8%, top 5 -19.5%, top 10 -11.9%.",
    "The top 5 buyers took 42.7% of the value, the top 5 sellers 62.2%: selling is more concentrated than buying.",
    "Foreign investors net sold 4.10T (-41.4% of value).",
    "Price -14.5% over the range: big distribution with the price down more than 2% reads as markdown.",
    "The top 5 buyers paid 5,515 on average against the last price 5,175: 6.2% under water.",
    "The source rounds figures of a million and more to one decimal.",
  ]);
  const one = bd.report(WEEK, "1w", 1, null, null);
  assert.equal(one.phase, null);
  assert.ok(one.reasons.includes("No price move for the range, so no phase."));
  const none = bd.report(EMPTY, "1d", 5, 5175, 0);
  assert.equal(none.signal, "neutral");
  assert.equal(none.phase, "neutral");
  assert.deepEqual(none.reasons, ["No trades from 2026-10-17 to 2026-10-17."]);
});

test("the daily series counts sessions and the streak", () => {
  const days = ["12", "13", "14", "15"].map((d) => load(`brokersummary_bbca_202610${d}.html`)).concat([DAY, EMPTY]);
  const d = bd.daily(days, 5);
  assert.equal(d.days.length, 5);
  assert.equal(d.days[0].date, "2026-10-12");
  assert.equal(d.accumulation_days + d.distribution_days + d.neutral_days, 5);
  assert.ok(Math.abs(d.cumulative_net_value - d.days.reduce((s, x) => s + x.net_value, 0)) < 1);
  assert.ok(d.streak < 0);
});

test("periods start off the weekend, weekdays skip it", () => {
  assert.equal(bd.periodStart("1d", "2026-10-16"), "2026-10-16");
  assert.equal(bd.periodStart("1w", "2026-10-16"), "2026-10-12");
  assert.equal(bd.periodStart("1m", "2026-10-16"), "2026-09-17");
  assert.equal(bd.periodStart("3m", "2026-10-16"), "2026-07-17");
  assert.equal(bd.periodStart("1m", "2026-03-31"), "2026-03-02", "28 Feb + 1 is a Sunday");
  assert.deepEqual(bd.weekdays("2026-10-09", "2026-10-13"), ["2026-10-09", "2026-10-12", "2026-10-13"]);
  assert.throws(() => bd.checkTop(4), /top must be/);
  assert.throws(() => bd.checkPeriod("2w"), /invalid period/);
});

test("filters, presets and sorting", () => {
  const f = bd.parseFilters("acc>=5, fpct>0 ;signal=bigacc,phase!=markup");
  assert.equal(f.length, 4);
  assert.equal(f[2].value, "big_accumulation");
  for (const bad of ["pe<10", "signal>acc", "signal=up", "acc>lots"]) assert.throws(() => bd.parseFilters(bad), (e) => e.code === "usage", bad);
  for (const name of Object.keys(bd.PRESETS)) assert.ok(bd.parseFilters(bd.preset(name).filter).length);
  assert.equal(bd.preset("big_accumulation").name, "big-accumulation");
  assert.throws(() => bd.preset("everything"), /unknown preset/);
  assert.equal(bd.parseSort("fnet"), "fnet");
  assert.throws(() => bd.parseSort("phase"), /unknown sort key/);

  const row = bd.screenRow(bd.report(WEEK, "1w", 5, 5175, -14.46));
  assert.equal(row.top_buyers.length, 5);
  assert.ok(bd.matches(row, bd.parseFilters("acc<=-15")));
  assert.ok(bd.matches(row, bd.parseFilters("phase=markdown,signal=big_dist")));
  assert.ok(!bd.matches(row, bd.parseFilters("acc>=5")));
  assert.ok(bd.matches(row, bd.parseFilters("phase!=markup")));
  const rows = [{ ...row, symbol: "AAAA", net_pct: null }, row, { ...row, symbol: "BBRI", net_pct: 3 }];
  assert.deepEqual(bd.sortRows(rows, "acc", true).map((r) => r.symbol), ["BBRI", "BBCA", "AAAA"]);
});

test("numbers print like the Rust formatter", () => {
  assert.equal(bd.compact(6.6e12), "6.60T");
  assert.equal(bd.compact(-1.93e12), "-1.93T");
  assert.equal(bd.compact(841.2e9), "841.2B");
  assert.equal(bd.compact(43_560), "43.56K");
  assert.equal(bd.compact(950), "950");
  assert.equal(bd.price(5515), "5,515");
  assert.equal(bd.num(-1234567.891, 2), "-1,234,567.89");
});
