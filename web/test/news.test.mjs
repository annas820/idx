// node --test web/test/*.mjs (no network). A feed that never answers must
// turn into a warning, not a request that stays pending, and a feed that is
// down must not be retried by every reader.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { news } from "../src/api.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const fixture = (p) => readFileSync(join(root, "tests", "fixtures", "news", p), "utf8");
const KONTAN = fixture("kontan_investasi.xml");
const CNBC = fixture("cnbc_market_excerpt.xml");

// The cache table as D1 would hold it: k -> { v, expires_at, fetched_at }.
function memoryDb() {
  const rows = new Map();
  return {
    rows,
    prepare: (sql) => ({ bind: (...a) => ({
      first: async () => (sql.startsWith("SELECT") ? rows.get(a[0]) ?? null : null),
      run: async () => { if (sql.startsWith("INSERT")) rows.set(a[0], { v: a[1], expires_at: a[2], fetched_at: a[3] }); return {}; },
    }) }),
  };
}

// Kontan either answers with the fixture or behaves like the runtime when the
// deadline passes; CNBC always answers.
function stubFetch(kontanUp) {
  const calls = { kontan: 0, withDeadline: 0, total: 0 };
  const fn = async (url, init) => {
    calls.total++;
    if (init?.signal instanceof AbortSignal) calls.withDeadline++;
    if (!String(url).includes("kontan")) return new Response(CNBC, { status: 200 });
    calls.kontan++;
    if (kontanUp.value) return new Response(KONTAN, { status: 200 });
    throw new DOMException("The operation was aborted due to timeout", "TimeoutError");
  };
  return { fn, calls };
}

test("a silent feed becomes a warning and the other feeds still answer", async () => {
  const realFetch = globalThis.fetch;
  const kontanUp = { value: false };
  const { fn, calls } = stubFetch(kontanUp);
  globalThis.fetch = fn;
  try {
    const env = { DB: memoryDb() };
    const out = await news(env, "TLKM", 20);
    assert.equal(calls.withDeadline, calls.total, "every feed fetch carries a deadline");
    assert.equal(calls.kontan, 3);
    assert.ok(out.env.data.items.length > 0);
    assert.ok(out.env.data.items.every((i) => i.source.startsWith("cnbcindonesia/")));
    assert.deepEqual(out.env.warnings, [
      "kontan/investasi: no answer within 8s from investasi.kontan.co.id",
      "kontan/keuangan: no answer within 8s from keuangan.kontan.co.id",
      "kontan/industri: no answer within 8s from industri.kontan.co.id",
    ]);

    // Within the retry window Kontan is not asked again.
    const again = await news(env, "TLKM", 20);
    assert.equal(calls.kontan, 3);
    assert.match(again.env.warnings[0], /^kontan\/investasi: no answer within 8s from investasi\.kontan\.co\.id; next try in \d+s$/);
  } finally {
    globalThis.fetch = realFetch;
  }
});

test("a feed that goes down serves its cached copy, marked stale", async () => {
  const realFetch = globalThis.fetch;
  const kontanUp = { value: true };
  const { fn, calls } = stubFetch(kontanUp);
  globalThis.fetch = fn;
  try {
    const env = { DB: memoryDb() };
    const fresh = await news(env, null, 0);
    assert.equal(fresh.env.stale, false);
    assert.equal(fresh.env.warnings, undefined);

    // The feed rows expire and Kontan stops answering.
    for (const row of env.DB.rows.values()) row.expires_at = 0;
    kontanUp.value = false;
    const down = await news(env, null, 0);
    assert.equal(calls.kontan, 6);
    assert.equal(down.env.stale, true);
    assert.ok(down.env.data.items.some((i) => i.source === "kontan/investasi"));
    assert.ok(down.env.warnings.includes("kontan/investasi: live fetch failed (no answer within 8s from investasi.kontan.co.id); showing cached copy"));

    const later = await news(env, null, 0);
    assert.equal(calls.kontan, 6);
    assert.equal(later.env.stale, true);
    assert.ok(later.env.data.items.some((i) => i.source === "kontan/investasi"));
  } finally {
    globalThis.fetch = realFetch;
  }
});
