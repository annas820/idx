# Ownership data

Since March 2026 KSEI compiles, and IDX publishes monthly, the list of every
shareholder holding **more than 1 %** of a listed company ("Pemegang Saham di
Atas 1%"). `idx ownership` imports those files into a local SQLite database
and answers questions from it offline.

## Source files

| period | format | where |
| --- | --- | --- |
| June 2026 onwards | XLSX, 12 columns | idx.co.id, "Data Kepemilikan Saham" page, `Media/.../*-satu-persen.xlsx` |
| February to May 2026 | PDF announcement | idx.co.id announcements ("Pemegang Saham di atas 1% (KSEI)") |

Columns: `DATE, SHARE_CODE, ISSUER_NAME, INVESTOR_NAME, INVESTOR_CLASSIFICATION`
(`INVESTOR_TYPE` in the first releases), `LOCAL_FOREIGN, NATIONALITY, DOMICILE,
HOLDINGS_SCRIPLESS, HOLDINGS_SCRIP, TOTAL_HOLDING_SHARES, PERCENTAGE`.
The importer accepts only this layout; the "di Atas 5 %" and investor-type
workbooks on the same page are rejected with a clear message.

## Getting the files

idx.co.id is behind Cloudflare bot management, so a plain HTTP client gets a
challenge page. `ownership import --latest|--all` and `ownership discover` try,
in order:

1. a browser-shaped request (Chrome user agent and headers);
2. a `curl-impersonate` binary on `PATH` (`curl_chrome131`, `curl_chrome124`,
   `curl_chrome116`, `curl-impersonate-chrome`, `curl-impersonate`; override
   with `IDX_CURL_IMPERSONATE=/path/to/binary`);
3. the GitHub mirror (below): the same files, downloaded from the
   repository's `ownership-data` release and checked against the SHA-256 in
   its `index.json`. A warning says when the mirror was used.
   `IDX_OWNERSHIP_MIRROR=<url>` points at another mirror, `off` disables it.

`--file <path>` and `--url <url>` import one file directly; `--url` also
accepts `file://` paths. `idx ownership discover` lists what is available,
where it comes from and which months are already imported.

## The mirror

`.github/workflows/ownership.yml` runs `scripts/ownership_mirror.py` at 08:17,
14:17 and 20:17 WIB:

1. fetches the IDX page with [curl_cffi](https://github.com/lexiforest/curl_cffi)
   (browser TLS fingerprint), saves it, and parses it with
   `idx ownership discover` (one parser for CLI and CI);
2. reuses files already mirrored, downloads new or re-published ones (a new
   IDX URL for the same month counts as re-published) and checks they are
   real XLSX/PDF files;
3. imports every month, oldest first, into a fresh database with
   `idx ownership import --file`; any parse failure fails the run, so a broken
   file never reaches the mirror;
4. exports each month as JSON parts of 1,000 rows and writes `index.json`:

```json
{
  "schema": "idx.ownership.mirror.v1",
  "generated_at": "2026-10-17T01:17:40Z",
  "source_page": "https://www.idx.co.id/id/perusahaan-tercatat/data-kepemilikan-saham/",
  "releases": [
    { "as_of": "2026-09-30", "title": "Pemegang Saham di Atas 1% per 30 September 2026",
      "format": "xlsx", "source_url": "https://www.idx.co.id/Media/.../peng-2026-09-00024-satu-persen.xlsx",
      "file": "ksei-above1-2026-09-30.xlsx", "sha256": "804b3bb7…", "bytes": 563730,
      "rows": 7151, "issuers": 961, "entities": 5145,
      "parts": [ { "file": "ksei-above1-2026-09-30.part01.json", "sha256": "…", "rows": 1000 } ] }
  ]
}
```

5. uploads only what changed, `index.json` last, and deletes assets the new
   index no longer lists. The release is a prerelease so it never becomes the
   repository's "latest" release.

The web terminal's Worker (`web/src/ownership-sync.js`) loads the parts into
D1 on its minute cron, newest month first, one part per minute, within a daily
rows-written budget (`OWN_DAILY_WRITE_BUDGET`, default 60,000, inside D1's free
tier). It keeps `OWN_KEEP_RELEASES` months (default 6). A month is published
only after its row count matches the index; a re-published IDX file (new
SHA-256) is reloaded. `/api/ownership/status` shows progress. There is no
upload endpoint: the only input is the repository's own release.

Running the workflow by hand: Actions → "ownership data" → Run workflow
(tick "dry run" to parse without uploading). Locally:
`pip install curl_cffi && python scripts/ownership_mirror.py --idx target/debug/idx --repo fzn0x/idx --dry-run`.

## Database

Default path: `%LOCALAPPDATA%\idx\ownership.sqlite` on Windows,
`$XDG_DATA_HOME/idx/ownership.sqlite` or `~/.local/share/idx/ownership.sqlite`
elsewhere. Override with `IDX_OWNERSHIP_DB` or `ownership.db` in the config.

Tables:

- `releases` one row per imported month (`as_of` unique; re-importing a month
  replaces it), with the source path/URL and SHA-256 of the file.
- `entities` one row per normalised holder: upper-cased alphanumerics, single
  spaces, legal-form tokens `PT`, `TBK`, `PERSERO` removed. `PT ALAMTRI
  RESOURCES INDONESIA TBK` and `Alamtri Resources Indonesia, Tbk.` share one
  entity.
- `aliases` every raw spelling seen, pointing at its entity.
- `holdings` one row per (release, symbol, entity) with investor type,
  local/foreign, nationality, domicile, scripless/scrip/total shares and
  percent.

Percentages are rounded to four decimals on import to remove binary noise in
the source (`1.1499999999999999` becomes `1.15`). Nothing else is altered.

## Queries

```sh
idx ownership ticker BBCA                       # holders above 1 %, newest month
idx ownership ticker BBCA --as-of 2026-07-31
idx ownership entity "Dwimuria Investama"       # every position of one holder
idx ownership changes --from 2026-07-31 --to 2026-08-31 --symbol BBCA
idx ownership changes --from 2026-02-27 --to 2026-08-31 --min-change 1 --limit 0
idx ownership changes --from 2026-07-31 --to 2026-08-31 --limit 100 --offset 100   # page 2
idx ownership foreign BBCA                      # foreign vs local share, month by month
idx ownership foreign                           # every issuer's foreign change, newest two months
idx ownership releases
```

`changes` compares two imported months per (symbol, entity): `new` holders
(absent before: crossed 1 % or newly listed), `exited` holders (absent after)
and `changed` positions, sorted by absolute percentage-point move. Rows carry
`local_foreign` (`L`/`A`). The `summary` block is computed over every match
before `--limit`/`--offset` paging: counts of new, exited, increased and
decreased holders, net percentage points split by foreign/local and by KSEI
investor type, per-issuer flow (with the foreign part), and the top rises and
falls. For a single symbol the net figures are exact; across the register they
are a flow indicator.

`foreign` sums the `L`/`A` flag KSEI prints on each listed holder. With a
symbol it gives one issuer's disclosed foreign percent, holder counts and
month-over-month change per release; without one it ranks every issuer by the
change in disclosed foreign stake between two releases. Sums cover holders
above 1 % only, so this is the *disclosed* foreign stake, not total foreign
ownership.

JSON schemas: `idx schema ownership-holders|ownership-entity|ownership-changes|
ownership-foreign|ownership-foreign-flow|ownership-releases|ownership-import|
ownership-discover`.

## Caveats

- Holders below 1 % are not published, so `listed_pct` is below 100 %.
- Entity grouping is heuristic: it merges spelling variants, not legally
  distinct vehicles with similar names. The raw spelling is kept on every row.
- PDF imports depend on the text layer of the announcement; numbers are
  checked but a reformatted PDF may need the parser updated.
