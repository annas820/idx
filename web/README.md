# web/

Cloudflare Worker behind **https://ihsg.midoriresearch.com**.

Today it serves the holding page in `public/`. Milestone 6 replaces it with the
hosted terminal backed by D1 (snapshot refreshed every minute by a cron
trigger) and the landing page with screenshots.

Access is domain-only: `workers_dev = false`, `preview_urls = false`, and the
Worker returns 404 for any Host other than `PUBLIC_HOST`.

```sh
cd web
npx wrangler dev      # local
npx wrangler deploy   # needs `wrangler login` on the Midori Research account
```
