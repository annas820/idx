// Loads the KSEI ">1 % holders" register into D1 from the GitHub mirror that
// .github/workflows/ownership.yml publishes (release tag `ownership-data`).
//
// Runs from the minute cron. There is no write endpoint: the Worker only
// reads index.json and the JSON parts from the repository's release, checks
// each part's SHA-256 against the index, and inserts it with one statement
// (`json_each` over the part text), one part per tick. A release becomes
// visible only after every part is in and the row count matches the index.
//
// Limits it respects: one D1 statement per part (D1 query caps), a daily
// rows-written budget (D1 free tier allows 100k/day; every holding is three
// writes with its two indexes), and the newest OWN_KEEP_RELEASES months.

import { fetchWithin } from "./net.js";

const DEFAULT_MIRROR = "https://github.com/fzn0x/idx/releases/download/ownership-data";
// Parts are a few MB at most; a stalled mirror gives up and the next tick retries.
const FETCH_TIMEOUT_MS = 30_000;
const SCHEMA = "idx.ownership.mirror.v1";
const CHECK_EVERY_SECS = 30 * 60;
const WRITES_PER_ROW = 3;
const UA = "idx-web (+https://ihsg.midoriresearch.com)";

export const mirrorBase = (env) => (env.OWN_MIRROR_URL || DEFAULT_MIRROR).replace(/\/+$/, "");
const keepReleases = (env) => Math.max(2, Number(env.OWN_KEEP_RELEASES || 6));
const dailyBudget = (env) => Math.max(5000, Number(env.OWN_DAILY_WRITE_BUDGET || 60000));
const wibDay = () => new Date(Date.now() + 7 * 3600e3).toISOString().slice(0, 10);

async function kvGet(env, k) {
  const row = await env.DB.prepare("SELECT v FROM kv WHERE k = ?1").bind(k).first();
  return row ? JSON.parse(row.v) : null;
}
async function kvPut(env, k, v) {
  await env.DB.prepare("INSERT OR REPLACE INTO kv (k, v, updated_at) VALUES (?1, ?2, ?3)").bind(k, JSON.stringify(v), Math.floor(Date.now() / 1000)).run();
}
async function kvDel(env, k) {
  await env.DB.prepare("DELETE FROM kv WHERE k = ?1").bind(k).run();
}

async function fetchText(url) {
  const r = await fetchWithin(url, { headers: { "User-Agent": UA, Accept: "application/json, */*" }, cf: { cacheTtl: 0, cacheEverything: false } }, { ms: FETCH_TIMEOUT_MS });
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.text();
}
export async function sha256Hex(text) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Picks the next action from the mirror index and what D1 holds. Pure, tested. */
export function plan(index, have, keep) {
  if (index?.schema !== SCHEMA) throw new Error(`unexpected mirror schema ${index?.schema}`);
  const wanted = [...index.releases].sort((a, b) => b.as_of.localeCompare(a.as_of)).slice(0, keep);
  const wantedDates = new Set(wanted.map((r) => r.as_of));
  const prune = [...have.keys()].filter((d) => !wantedDates.has(d)).sort();
  if (prune.length) return { prune: prune[0] };
  const load = wanted.find((r) => have.get(r.as_of) !== r.sha256);
  return load ? { load } : { upToDate: wanted.length };
}

const INSERT = `INSERT OR REPLACE INTO own_holdings
  (as_of, symbol, issuer_name, entity_key, holder, holder_raw, investor_type, local_foreign,
   nationality, domicile, shares_scripless, shares_scrip, shares_total, pct)
  SELECT ?1, json_extract(value, '$.symbol'), json_extract(value, '$.issuer_name'), json_extract(value, '$.entity_key'),
         json_extract(value, '$.holder'), json_extract(value, '$.holder_raw'), json_extract(value, '$.investor_type'),
         json_extract(value, '$.local_foreign'), json_extract(value, '$.nationality'), json_extract(value, '$.domicile'),
         json_extract(value, '$.shares_scripless'), json_extract(value, '$.shares_scrip'),
         json_extract(value, '$.shares_total'), json_extract(value, '$.pct')
  FROM json_each(?2)`;

/**
 * One step of the sync. `force` checks the mirror now instead of every 30
 * minutes. Returns what happened, for logs and the status endpoint.
 */
export async function ownershipSync(env, { force = false } = {}) {
  const now = Math.floor(Date.now() / 1000);
  const base = mirrorBase(env);
  let state = await kvGet(env, "own:sync");

  if (!state) {
    const checked = (await kvGet(env, "own:checked")) || 0;
    if (!force && now - checked < CHECK_EVERY_SECS) return { idle: true, next_check_in: CHECK_EVERY_SECS - (now - checked) };
    const index = JSON.parse(await fetchText(`${base}/index.json`));
    await kvPut(env, "own:checked", now);
    const have = new Map((await env.DB.prepare("SELECT as_of, sha256 FROM own_releases").all()).results.map((r) => [r.as_of, r.sha256]));
    const next = plan(index, have, keepReleases(env));
    if (next.upToDate != null) return { up_to_date: next.upToDate };
    if (next.prune) {
      await env.DB.batch([
        env.DB.prepare("DELETE FROM own_releases WHERE as_of = ?1").bind(next.prune),
        env.DB.prepare("DELETE FROM own_holdings WHERE as_of = ?1").bind(next.prune),
      ]);
      await kvDel(env, "own:checked"); // look again next tick
      return { pruned: next.prune };
    }
    const r = next.load;
    // Hide any older copy of this month and clear leftovers of an aborted load.
    await env.DB.batch([
      env.DB.prepare("DELETE FROM own_releases WHERE as_of = ?1").bind(r.as_of),
      env.DB.prepare("DELETE FROM own_holdings WHERE as_of = ?1").bind(r.as_of),
    ]);
    state = {
      as_of: r.as_of, sha256: r.sha256, format: r.format, source: r.source_url || `${base}/${r.file}`,
      rows: r.rows, issuers: r.issuers ?? null, entities: r.entities ?? null, parts: r.parts, next: 0, started_at: now,
    };
    await kvPut(env, "own:sync", state);
  }

  let budget = await kvGet(env, "own:budget");
  if (!budget || budget.day !== wibDay()) budget = { day: wibDay(), rows: 0 };
  const part = state.parts[state.next];
  if (budget.rows + part.rows * WRITES_PER_ROW > dailyBudget(env)) {
    return { waiting_for_budget: { day: budget.day, used: budget.rows, limit: dailyBudget(env) }, loading: state.as_of };
  }
  const text = await fetchText(`${base}/${part.file}`);
  if ((await sha256Hex(text)) !== part.sha256) {
    await kvDel(env, "own:sync");
    await kvDel(env, "own:checked");
    throw new Error(`${part.file} no longer matches the index it was listed in; restarting from the new index`);
  }
  await env.DB.prepare(INSERT).bind(state.as_of, text).run();
  budget.rows += part.rows * WRITES_PER_ROW;
  await kvPut(env, "own:budget", budget);
  state.next += 1;
  if (state.next < state.parts.length) {
    await kvPut(env, "own:sync", state);
    return { loading: state.as_of, part: state.next, of: state.parts.length };
  }

  const count = (await env.DB.prepare("SELECT COUNT(*) AS n FROM own_holdings WHERE as_of = ?1").bind(state.as_of).first()).n;
  await kvDel(env, "own:sync");
  if (count !== state.rows) {
    await env.DB.prepare("DELETE FROM own_holdings WHERE as_of = ?1").bind(state.as_of).run();
    throw new Error(`${state.as_of}: loaded ${count} rows but the index lists ${state.rows}; discarded`);
  }
  await env.DB.prepare(
    "INSERT OR REPLACE INTO own_releases (as_of, format, source, rows, synced_at, sha256, issuers, entities) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
  ).bind(state.as_of, state.format, state.source, count, now, state.sha256, state.issuers, state.entities).run();
  await kvDel(env, "own:checked"); // next tick looks for the next month to load
  return { committed: state.as_of, rows: count };
}

/** What the terminal shows while the register is empty or loading. */
export async function syncStatus(env) {
  const [state, checked, budget] = await Promise.all([kvGet(env, "own:sync"), kvGet(env, "own:checked"), kvGet(env, "own:budget")]);
  return {
    mirror: mirrorBase(env),
    loading: state ? { as_of: state.as_of, part: state.next, of: state.parts.length, started_at: new Date(state.started_at * 1000).toISOString() } : null,
    checked_at: checked ? new Date(checked * 1000).toISOString() : null,
    writes_today: budget && budget.day === wibDay() ? budget.rows : 0,
    daily_write_budget: dailyBudget(env),
    keep_releases: keepReleases(env),
  };
}
