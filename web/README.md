# web/

Cloudflare Worker behind **https://ihsg.midoriresearch.com**.

- `public/` landing page (`index.html`) and the browser terminal (`terminal.html`),
  plain HTML/CSS/ES modules, no build step.
- `src/worker.js` entry: host check, security headers, `/api/*`, cron.
- `src/api.js` endpoints returning the CLI's JSON envelopes; D1-backed cache.
- `src/yahoo.js` the Yahoo endpoints and parsers; `src/analysis.js` ports of
  the Rust indicators, verdict, screener DSL and sentiment lexicon;
  `src/extras.js` ports of returns, dividends, news ordering and the
  ownership changes summary.
- Endpoints under `/api`: `market`, `quote`, `history`, `profile`,
  `fundamental`, `earnings`, `financials`, `dividends`, `returns`, `technical`,
  `brief`, `compare`, `screen`, `news`, `sentiment`, `ownership/ticker`,
  `ownership/entity`, `ownership/changes`, `ownership/foreign`,
  `ownership/releases`, `universe`.
- `migrations/` D1 schema. `test/` runs with `node --test web/test/*.mjs`.
- Search and sharing: canonical, Open Graph and Twitter tags on both pages,
  JSON-LD on the landing page, `public/robots.txt` and `public/sitemap.xml`;
  API responses carry `x-robots-tag: noindex`. The share card
  `public/og.png` (1200x630) and `public/apple-touch-icon.png` are rendered
  from `og/card.html` and `og/icon.html` by `node web/scripts/og.mjs`
  (headless Chrome, Node 22+); re-run it after editing either source.
  `test/seo.test.mjs` checks the tags against the files.
- The cron (`* * * * *`) refreshes the whole quote board as one JSON row and
  the IHSG tape every minute in IDX hours, every 15 minutes otherwise, which
  keeps writes far inside D1's free tier.
- Secrets: `ADMIN_TOKEN` (`wrangler secret put`) lets a maintainer force a
  refresh with `/api/refresh?token=…`.
- Ownership on the web: `src/ownership-sync.js` loads the KSEI register from
  the repository's `ownership-data` release (published by
  `.github/workflows/ownership.yml`) on the minute cron: index checked every
  30 minutes, one SHA-256-verified part per minute, newest month first, within
  `OWN_DAILY_WRITE_BUDGET`. No upload endpoint exists. Progress:
  `/api/ownership/status`; a maintainer can force a step with
  `/api/refresh?token=…&what=ownership`.

Access is domain-only: `workers_dev = false`, `preview_urls = false`, and the
Worker returns 404 for any Host other than `PUBLIC_HOST`.

```sh
cd web
npx wrangler dev      # local
npx wrangler deploy   # needs `wrangler login` on the Midori Research account
```
