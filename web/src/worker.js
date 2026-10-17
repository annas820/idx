// idx web: Cloudflare Worker in front of the static holding page.
//
// Milestone 6 turns this into the hosted terminal (D1-backed snapshot
// refreshed every minute). Until then it serves `public/` and enforces
// domain-only access: any request whose Host is not PUBLIC_HOST gets a 404,
// so even an accidental workers.dev or preview route exposes nothing.

const SECURITY_HEADERS = {
  "x-content-type-options": "nosniff",
  "x-frame-options": "DENY",
  "referrer-policy": "strict-origin-when-cross-origin",
  "permissions-policy": "camera=(), microphone=(), geolocation=()",
  "content-security-policy":
    "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; script-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname !== env.PUBLIC_HOST) {
      return new Response("Not found", { status: 404 });
    }
    if (url.pathname === "/healthz") {
      return new Response("ok", { headers: { "content-type": "text/plain" } });
    }
    const response = await env.ASSETS.fetch(request);
    const headers = new Headers(response.headers);
    for (const [k, v] of Object.entries(SECURITY_HEADERS)) headers.set(k, v);
    return new Response(response.body, { status: response.status, headers });
  },
};
