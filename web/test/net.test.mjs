// node --test web/test/*.mjs (no network). Every outbound call of the Worker
// goes through fetchWithin: a source that never answers must end in an error
// within the deadline instead of a request that stays pending.
import { test } from "node:test";
import assert from "node:assert/strict";

import { fetchWithin } from "../src/net.js";
import * as y from "../src/yahoo.js";
import * as ipot from "../src/ipot.js";

// Node's AbortSignal.timeout does not keep the process alive; a pending
// connection would, so the stubs hold a timer until the abort.
const untilAbort = (signal, then) => { const keep = setTimeout(() => {}, 10_000); signal.addEventListener("abort", () => { clearTimeout(keep); then(signal.reason); }); };
// Accepts the request and never answers, like Kontan in Oct 2026.
const silent = (url, init) => new Promise((_, reject) => untilAbort(init.signal, reject));

test("a silent source fails at the deadline with the host named", async () => {
  const t = Date.now();
  await assert.rejects(fetchWithin("https://slow.example/rss", {}, { ms: 50, fetchFn: silent }), { message: "no answer within 0.05s from slow.example" });
  assert.ok(Date.now() - t < 2000);
});

test("a body that stalls counts against the same deadline", async () => {
  const stalled = async (url, init) => new Response(new ReadableStream({
    start(c) { c.enqueue(new TextEncoder().encode("<rss>")); untilAbort(init.signal, (why) => c.error(why)); },
  }));
  await assert.rejects(fetchWithin("https://slow.example/rss", {}, { ms: 50, fetchFn: stalled }), /no answer within 0.05s/);
});

test("the buffered response keeps status, headers and body", async () => {
  const ok = async () => new Response('{"a":1}', { status: 404, headers: { "set-cookie": "A3=x; Path=/", "content-length": "7" } });
  const r = await fetchWithin("https://x.example/", {}, { fetchFn: ok });
  assert.equal(r.status, 404);
  assert.equal(r.ok, false);
  assert.match(r.headers.get("set-cookie"), /^A3=x/);
  assert.deepEqual(await r.json(), { a: 1 });
  const empty = await fetchWithin("https://x.example/", {}, { fetchFn: async () => new Response(null, { status: 204 }) });
  assert.equal(empty.status, 204);
  await assert.rejects(fetchWithin("https://x.example/", {}, { fetchFn: async () => { throw new TypeError("fetch failed"); } }), /fetch failed/);
});

test("IPOT gives up on a silent page without retrying it", async () => {
  let calls = 0;
  const hang = (u, init) => { calls++; return silent(u, init); };
  await assert.rejects(ipot.fetchPage("https://www.indopremier.com/x", { pauseMs: 1, ms: 50, fetchFn: hang }), /no answer within 0.05s from www.indopremier.com/);
  assert.equal(calls, 1);
});

test("every Yahoo call carries a deadline", async () => {
  const realFetch = globalThis.fetch;
  const seen = [];
  globalThis.fetch = async (url, init) => {
    seen.push([String(url).split("?")[0], init?.signal instanceof AbortSignal]);
    if (String(url).startsWith("https://fc.yahoo.com")) return new Response("", { status: 404, headers: { "set-cookie": "A3=d=abc; Path=/" } });
    if (String(url).includes("getcrumb")) return new Response("crumb123");
    return new Response("{}", { status: 200 });
  };
  const env = { DB: { prepare: () => ({ first: async () => null, bind: () => ({ first: async () => null, run: async () => ({}) }) }) } };
  try {
    await y.fetchChart("BBCA");
    await y.fetchEvents("BBCA");
    await y.fetchTimeseries("BBCA");
    await y.fetchQuotes(env, ["BBCA"]);
    await y.fetchSummary(env, "BBCA", y.FUNDAMENTAL_MODULES);
  } finally {
    globalThis.fetch = realFetch;
  }
  assert.ok(seen.length >= 7);
  assert.deepEqual(seen.filter(([, ok]) => !ok), []);
});
