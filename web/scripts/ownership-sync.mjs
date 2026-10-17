#!/usr/bin/env node
// Copies the CLI's local ownership database (idx ownership import ...) into the
// Worker's D1 so the web terminal can answer `ownership ticker BBCA`.
//
//   node web/scripts/ownership-sync.mjs [path/to/ownership.sqlite] [--local]
//
// Reads with node:sqlite (Node 22.13+), writes through `wrangler d1 execute`
// in batches. Re-running replaces each release's rows.
import { DatabaseSync } from "node:sqlite";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir, homedir, platform } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
const remote = !args.includes("--local");
const explicit = args.find((a) => !a.startsWith("--"));
const defaultDb = platform() === "win32"
  ? join(process.env.LOCALAPPDATA || homedir(), "idx", "ownership.sqlite")
  : join(process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"), "idx", "ownership.sqlite");
const dbPath = explicit || process.env.IDX_OWNERSHIP_DB || defaultDb;

const db = new DatabaseSync(dbPath, { readOnly: true });
const releases = db.prepare("SELECT id, as_of, format, source, row_count FROM releases ORDER BY as_of").all();
if (!releases.length) {
  console.error(`no releases in ${dbPath}; run idx ownership import first`);
  process.exit(1);
}
const q = (v) => (v == null ? "NULL" : typeof v === "number" ? String(v) : `'${String(v).replace(/'/g, "''")}'`);
const dir = mkdtempSync(join(tmpdir(), "idx-own-"));
let files = 0;
function run(sql) {
  const f = join(dir, `batch-${files++}.sql`);
  writeFileSync(f, sql);
  execFileSync("npx", ["wrangler", "d1", "execute", "idx", remote ? "--remote" : "--local", "--file", f], { stdio: "inherit", cwd: join(import.meta.dirname, ".."), shell: true });
}
const now = Math.floor(Date.now() / 1000);
for (const r of releases) {
  const rows = db.prepare(
    `SELECT h.symbol, h.issuer_name, e.entity_key, e.display_name AS holder, h.holder_raw, h.investor_type,
            h.local_foreign, h.shares_total, h.pct
     FROM holdings h JOIN entities e ON e.id = h.entity_id WHERE h.release_id = ?`,
  ).all(r.id);
  console.log(`${r.as_of}: ${rows.length} rows`);
  let sql = `DELETE FROM own_holdings WHERE as_of = ${q(r.as_of)};\nINSERT OR REPLACE INTO own_releases (as_of, format, source, rows, synced_at) VALUES (${q(r.as_of)}, ${q(r.format)}, ${q(r.source)}, ${rows.length}, ${now});\n`;
  let n = 0;
  for (const h of rows) {
    sql += `INSERT OR REPLACE INTO own_holdings (as_of, symbol, issuer_name, entity_key, holder, holder_raw, investor_type, local_foreign, shares_total, pct) VALUES (${q(r.as_of)}, ${q(h.symbol)}, ${q(h.issuer_name)}, ${q(h.entity_key)}, ${q(h.holder)}, ${q(h.holder_raw)}, ${q(h.investor_type)}, ${q(h.local_foreign)}, ${h.shares_total}, ${h.pct});\n`;
    if (++n % 500 === 0) { run(sql); sql = ""; }
  }
  if (sql) run(sql);
}
rmSync(dir, { recursive: true, force: true });
console.log("done");
