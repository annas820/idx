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
challenge page. `ownership import --latest|--all|--url` tries, in order:

1. a browser-shaped request (Chrome user agent and headers);
2. a `curl-impersonate` binary on `PATH` (`curl_chrome131`, `curl_chrome124`,
   `curl_chrome116`, `curl-impersonate-chrome`, `curl-impersonate`; override
   with `IDX_CURL_IMPERSONATE=/path/to/binary`);
3. otherwise it stops with instructions to download the workbook in a browser
   and run `idx ownership import --file <path>`.

`idx ownership discover` lists what the page currently offers and which
months are already imported.

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
idx ownership releases
```

`changes` compares two imported months per (symbol, entity): `new` holders
(absent before: crossed 1 % or newly listed), `exited` holders (absent after)
and `changed` positions, sorted by absolute percentage-point move.

JSON schemas: `idx schema ownership-holders|ownership-entity|ownership-changes|
ownership-releases|ownership-import|ownership-discover`.

## Caveats

- Holders below 1 % are not published, so `listed_pct` is below 100 %.
- Entity grouping is heuristic: it merges spelling variants, not legally
  distinct vehicles with similar names. The raw spelling is kept on every row.
- PDF imports depend on the text layer of the announcement; numbers are
  checked but a reformatted PDF may need the parser updated.
