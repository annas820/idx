---
version: 1
slug: "web-public-index-html"
primary_target: "web/public/index.html"
related_targets: ["web/public/terminal.html"]
---

# Surface: ihsg.midoriresearch.com landing page (web/public/index.html) and the web terminal (web/public/terminal.html)

Scope: Persuade (landing) with the terminal as an Operate surface inside the same world.
Audience: IDX retail investors and analysts arriving from GitHub or a shared link; AI-agent builders evaluating the MCP server.
Job: understand in seconds what idx is, see it prove itself with live IDX data, open the terminal or install.
Proof/content: live quote board from /api/market (real, minute-refreshed), real CLI output, real JSON, real command list. No testimonials, logos or benchmarks exist and none are invented.
Constraints: English UI; brand mark and greens from assets/mark.svg; disclaimers one sentence; works without JS for the copy; AA contrast; reduced motion honoured; CSP forbids inline scripts.

## Direction contract

THESIS: The page is a "Laporan Keuangan Publikasi", the full-page audited statement Indonesian Tbk companies print in the newspaper: the product's own tables are the page, set in ruled black ink on newsprint, with one committed corporate band in idx green. It refuses the developer-tool landing (hero copy beside a screenshot, three feature cards, install block).

OWN-WORLD: Ground is cool newsprint (#f1f1ee, never cream), ink is near-black (#111), hairline and heavy rules in ink, a committed band of idx green (#16a34a / deep #0f6b31) that owns the masthead and section strips at page scale, red (#c81e3a) only for negative figures. Type: Archivo (variable; the ad's black condensed grotesk for mastheads and section strips in uppercase, regular/tabular for the numbers), no serif, no mono except literal commands. Components: ruled tables with double rule under the header and a heavy rule closing each statement; "Catatan" numbered notes; a signature block with a stamped action. With the content removed the page still reads as a printed statement: rules, bands, column grid.

STORY: A visitor recognises the published-statement idiom instantly, reads that idx is one free binary for IDX data in three faces (table, JSON, MCP), sees the live board prove it with today's prices and timestamps, and either opens the terminal (stamped action) or copies the install line.

FIRST VIEWPORT: Masthead band (green) edge to edge: mark + "idx" wordmark left, statement title "LAPORAN PASAR · Indonesian stock market CLI, MCP server and web terminal" centre, dateline right ("Diterbitkan setiap menit · as of HH:MM WIB"). Below, a two-column ruled sheet: left 2/3 is the live board "PAPAN HARGA" with the composite index line at top and 12 heavyweights (symbol, name, price, change, %, volume, market cap) in tabular figures, double-ruled header, heavy closing rule, a one-line provenance footer. Right 1/3 is "KETERANGAN": three terse lines (what, who, free/no keys), then the primary action, a stamped black box "Open the terminal →" (rotated 1.5°, ink stamp texture via CSS) above the install one-liner in a ruled box. Nothing scrolls above the fold; the board is the thesis.

FORM: Grounded candidate 1 of my ordered list (published financial statement page), presented as IMPECCABLE'S PICK and chosen by the user over the assigned carbonless-form card; seed key e6686001, mode persuade. Raises kept from the declined challengers: ticker codes set large where they lead a row (alphabet storm), layout snapped to the rule grid (azulejo), rows packed with no card padding (toy catalogue), the board as the single compositional law of the first viewport (collider). Signature interaction: live re-inking; every 60 s the board refetches and changed cells darken as if freshly printed then settle, the dateline advances; on load the rules draw once left to right (one authored moment, disabled under reduced motion). Terminal page: same sheet, the prompt line pinned at the bottom of the sheet as "Perintah:", each result printed as its own ruled statement with the command as its caption.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Unresolved
- Ownership data on the web: only if the maintainer syncs a register; the terminal states the limitation.
- Screenshots for README come from this build once reviewed.
