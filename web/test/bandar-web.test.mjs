// node --test web/test/*.mjs (no network). The Worker side of bandarmology:
// /api/bandar, the nightly scan (src/bandar-scan.js) and /api/bandar/screen,
// over an in-memory D1 and the recorded IPOT and Yahoo responses.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { bandar, bandarScreen } from "../src/api.js";
import { targetSession, mostTraded, bandarScanStep, scanStatus } from "../src/bandar-scan.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const page = (f) => readFileSync(join(root, "tests", "fixtures", "ipot", f), "utf8");
const CHART = readFileSync(join(root, "tests", "fixtures", "yahoo", "chart_bbca_1mo_1d.json"), "utf8");
const quote = (symbol, price, volume) => ({ symbol, price, volume, previous_close: 6075, market_time: "2026-10-16T16:14:58+07:00" });

/** In-memory D1 for the statements the bandar code runs. */
function memoryDb() {
  const kv = new Map(), cache = new Map(), scan = new Map();
  const run = (sql, a) => {
    if (sql.startsWith("SELECT v, updated_at FROM kv")) return kv.has(a[0]) ? { v: kv.get(a[0]), updated_at: 0 } : null;
    if (sql.startsWith("SELECT v FROM kv")) return kv.has(a[0]) ? { v: kv.get(a[0]) } : null;
    if (sql.startsWith("INSERT OR REPLACE INTO kv")) return kv.set(a[0], a[1]);
    if (sql.startsWith("SELECT v, fetched_at, expires_at FROM cache")) return cache.get(a[0]) || null;
    if (sql.startsWith("INSERT OR REPLACE INTO cache")) return cache.set(a[0], { v: a[1], expires_at: a[2], fetched_at: a[3] });
    if (sql.startsWith("INSERT OR REPLACE INTO bandar_scan")) return scan.set([a[0], a[1], a[2]].join("|"), a[3]);
    if (sql.startsWith("SELECT symbol, v FROM bandar_scan")) {
      return [...scan].filter(([k]) => k.startsWith([a[0], a[1], ""].join("|"))).map(([k, v]) => ({ symbol: k.split("|")[2], v }));
    }
    if (sql.startsWith("DELETE FROM bandar_scan")) return null;
    throw new Error("memoryDb: unexpected SQL " + sql);
  };
  const stmt = (sql, a = []) => ({
    bind: (...b) => stmt(sql, b),
    first: async () => run(sql, a),
    run: async () => (run(sql, a), {}),
    all: async () => ({ results: run(sql, a) }),
    exec: () => run(sql, a),
  });
  return { DB: { prepare: (sql) => stmt(sql), batch: async (list) => list.map((s) => s.exec()) }, kv, scan };
}

/** IPOT and Yahoo from the recorded pages: BBCA, BRPT, TLKM; ZZZZ is unknown. */
function recordedFetch(calls = []) {
  return async (u) => {
    const url = String(u);
    calls.push(url);
    if (url.includes("finance/chart")) return new Response(CHART, { status: 200 });
    const code = url.match(/code=([A-Z]+)/)[1];
    const d = url.match(/start=(\d\d)\/(\d\d)\/(\d{4})&end=(\d\d)\/(\d\d)\/(\d{4})/);
    const oneDay = d[1] + d[2] === d[4] + d[5];
    if (code === "ZZZZ") return new Response(page("brokersummary_zzzz_invalid.html"), { status: 200 });
    if (code === "BBCA" && oneDay) return new Response(page("brokersummary_bbca_2026" + d[4] + d[5] + ".html"), { status: 200 });
    return new Response(page("brokersummary_" + code.toLowerCase() + "_20261012_20261016.html"), { status: 200 });
  };
}

async function withFetch(fn, body) {
  const realFetch = globalThis.fetch;
  globalThis.fetch = fn;
  try {
    await body();
  } finally {
    globalThis.fetch = realFetch;
  }
}

test("the scan targets the last finished session", () => {
  assert.equal(targetSession(new Date("2026-10-16T03:00:00Z")), "2026-10-15", "Friday 10:00 WIB: Thursday");
  assert.equal(targetSession(new Date("2026-10-16T09:50:00Z")), "2026-10-16", "Friday 16:50 WIB: Friday");
  assert.equal(targetSession(new Date("2026-10-17T03:00:00Z")), "2026-10-16", "Saturday: Friday");
  assert.equal(targetSession(new Date("2026-10-19T01:00:00Z")), "2026-10-16", "Monday 08:00 WIB: Friday");
  assert.deepEqual(mostTraded([quote("A", 10, 5), quote("B", 100, 5), quote("^JKSE", 7000, 9e9), quote("C", 1, 0)], 5), ["B", "A"]);
});

test("the nightly scan fills D1 and the screen reads it like the CLI", async () => {
  const env = memoryDb();
  env.kv.set("board", JSON.stringify({ ts: 0, quotes: [quote("BBCA", 5175, 9e8), quote("TLKM", 2690, 3e8), quote("BRPT", 1210, 2e8), quote("ZZZZ", 50, 1e6)] }));
  await withFetch(recordedFetch(), async () => {
    await assert.rejects(bandarScreen(env, "big-distribution"), (e) => e.code === "not_found");
    const now = new Date("2026-10-17T03:00:00Z");
    assert.deepEqual(await bandarScanStep(env, { now }), { session: "2026-10-16", scanned: 4, of: 4, done: true });
    assert.deepEqual(await bandarScanStep(env, { now }), { idle: true, session: "2026-10-16" });
    const st = await scanStatus(env);
    assert.equal(st.complete, "2026-10-16");
    assert.deepEqual(st.failed, ["ZZZZ"]);
    assert.equal(env.scan.size, 12, "three stocks x four periods");

    const out = await bandarScreen(env, "big-distribution", "turnover>1B", "1w");
    assert.equal(out.schema, "idx.bandar.screen.v1");
    assert.equal(out.provider, "ipot");
    const s = out.data;
    assert.deepEqual([s.from, s.to, s.preset, s.descending], ["2026-10-12", "2026-10-16", "big-distribution", false]);
    assert.deepEqual(s.filters, ["acc<=-15", "turnover>1B"]);
    assert.deepEqual([s.scanned, s.checked], [4, 3]);
    assert.deepEqual(s.rows.map((r) => r.symbol), ["BBCA", "TLKM"], "same order as the Rust screen test");
    assert.equal(s.rows[0].phase, "markdown");
    assert.ok(Math.abs(s.rows[0].price_change_pct - (5175 / 6050 - 1) * 100) < 1e-9);
    assert.ok(out.warnings.includes("1 of 4 scanned stocks had no broker summary"), out.warnings);

    const all = await bandarScreen(env, null, "acc>-100", "1w", "10", "fnet", "0", "1");
    assert.equal(all.data.rows.length, 1);
    assert.equal(all.data.total_matches, 3);
    assert.equal(all.data.sort, "fnet");
    for (const bad of [["everything"], [null, "pe<10"], [null, null, "2w"], [null, null, null, "4"], [null, null, null, null, "phase"]]) {
      await assert.rejects(bandarScreen(env, ...bad), (e) => e.code === "usage", JSON.stringify(bad));
    }
  });
});

test("a holiday is noticed and skipped", async () => {
  const env = memoryDb();
  env.kv.set("board", JSON.stringify({ ts: 0, quotes: [quote("BBCA", 5175, 9e8)] }));
  await withFetch(async () => new Response(page("brokersummary_bbca_20261017_no_trades.html"), { status: 200 }), async () => {
    assert.deepEqual(await bandarScanStep(env, { now: new Date("2026-10-17T03:00:00Z") }), { holiday: "2026-10-16" });
    assert.equal((await scanStatus(env)).holiday, true);
    assert.equal(env.scan.size, 0);
  });
});

test("/api/bandar reads a range with its daily series", async () => {
  const env = memoryDb();
  env.kv.set("board", JSON.stringify({ ts: 0, quotes: [quote("BBCA", 5175, 9e8)] }));
  const calls = [];
  await withFetch(recordedFetch(calls), async () => {
    const out = await bandar(env, "BBCA", null, null, "2026-10-12", "2026-10-16", null, "all", "all", "1");
    assert.equal(out.schema, "idx.bandar.v1");
    const r = out.data[0];
    assert.equal(r.period, "custom");
    assert.equal(r.name, "BANK CENTRAL ASIA Tbk");
    assert.equal(r.signal, "big_distribution");
    assert.equal(r.phase, "markdown");
    assert.equal(r.buyers[0].name, "CLSA SEKURITAS INDONESIA");
    assert.equal(r.reasons[0], "Top 5 buyers bought 4.23T, top 5 sellers sold 6.16T: net -1.93T, -19.5% of the 9.90T traded (big distribution, at -15% or less).");
    assert.equal(r.daily.days.length, 5);
    assert.ok(r.daily.streak < 0);
    assert.equal(calls.filter((u) => u.includes("indopremier")).length, 6, "the range and five sessions");
    for (const bad of [
      ["BBCA,BBRI,BMRI,BBNI,TLKM,ASII"],
      ["BBCA", "2w"],
      ["BBCA", null, null, null, null, "4"],
      ["BBCA,BBRI", null, null, null, null, null, "all", "all", "1"],
      ["BBCA", "3m", null, null, null, null, "all", "all", "1"],
      ["BBCA", null, "2026-10-16", "2026-10-01"],
      ["BBCA", null, null, null, null, null, "retail"],
    ]) await assert.rejects(bandar(env, ...bad), (e) => e.code === "usage", JSON.stringify(bad));
  });
});
