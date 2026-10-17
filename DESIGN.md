---
name: idx web surface
description: The published financial statement page. Ink-ruled tables on cool newsprint, one committed band of idx green, red only for what fell.
colors:
  paper: "#f1f1ee"
  paper-deep: "#e6e6e1"
  ink: "#111312"
  ink-2: "#3b413d"
  ink-3: "#5e665f"
  rule: "#1b1e1c"
  rule-soft: "#b9bcb5"
  green: "#16a34a"
  green-deep: "#0f6b31"
  green-ink: "#0b4d24"
  green-tint: "#dcefe2"
  red: "#b3122f"
  band-text: "#ffffff"
  band-text-soft: "#e9f6ee"
typography:
  masthead:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "clamp(1.3rem, 2.2vw, 1.6rem)"
    fontWeight: 900
    lineHeight: 1.35
    letterSpacing: "0.04em"
    fontVariation: "'wdth' 75"
  strip:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "1.05rem"
    fontWeight: 800
    lineHeight: 1.05
    letterSpacing: "0.04em"
    fontVariation: "'wdth' 75"
  figure-display:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "clamp(1.6rem, 3vw, 2.2rem)"
    fontWeight: 800
    lineHeight: 1
    letterSpacing: "-0.02em"
    fontFeature: "tabular-nums lining-nums"
  symbol:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "1.02rem"
    fontWeight: 800
    letterSpacing: "-0.01em"
  body:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "0.95rem"
    fontWeight: 400
    lineHeight: 1.45
  table:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "0.9rem"
    fontWeight: 400
    fontFeature: "tabular-nums lining-nums"
  label:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "0.76rem"
    fontWeight: 700
    letterSpacing: "0.04em"
  caption:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "0.8rem"
    fontWeight: 500
    lineHeight: 1.45
  mono:
    fontFamily: "ui-monospace, Cascadia Mono, Menlo, Consolas, monospace"
    fontSize: "0.78rem"
    lineHeight: 1.45
rounded:
  none: "0px"
  mark: "7px"
spacing:
  hair: "0.25rem"
  row: "0.34rem"
  cell: "0.4rem"
  xs: "0.5rem"
  sm: "0.75rem"
  md: "1rem"
  lg: "1.5rem"
  xl: "2rem"
  col: "2.5rem"
  section: "3rem"
  gutter: "clamp(1rem, 3vw, 2.5rem)"
components:
  masthead-band:
    backgroundColor: "{colors.green-deep}"
    textColor: "{colors.band-text}"
    padding: "0.7rem 0"
  strip:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.strip}"
    padding: "0.35rem 0"
  strip-green:
    backgroundColor: "{colors.green-deep}"
    textColor: "{colors.band-text}"
    typography: "{typography.strip}"
    padding: "0.4rem 0.6rem"
  stamp:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "0.7rem 1.1rem"
  stamp-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
  stamp-active:
    backgroundColor: "{colors.green-deep}"
    textColor: "{colors.band-text}"
  button-run:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0.45rem 0.8rem"
  button-run-hover:
    backgroundColor: "{colors.green-deep}"
  button-copy:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0.2rem 0.5rem"
  button-copy-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
  hint-chip:
    backgroundColor: "transparent"
    textColor: "{colors.ink-2}"
    typography: "{typography.mono}"
    rounded: "{rounded.none}"
    padding: "0.15rem 0.5rem"
  hint-chip-hover:
    textColor: "{colors.ink}"
  ruled-box:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "0.6rem 0.75rem"
  prompt-input:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.mono}"
    rounded: "{rounded.none}"
    padding: "0.3rem 0"
  table-header:
    backgroundColor: "transparent"
    textColor: "{colors.ink-2}"
    typography: "{typography.label}"
    padding: "0.45rem 0.4rem 0.3rem"
  table-cell:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.table}"
    padding: "0.34rem 0.4rem"
---

# Design System: idx web surface

Scope: this system governs the hosted web surface only (`web/public`: the landing page, the browser terminal and their renderers). The CLI's table output in a shell and the README / `assets/logo.svg` wordmark are separate artifacts and are not styled by it. The web reuses `assets/mark.svg` as its brand mark, unaltered; the mark's own palette (slate `#0b0f14`, mint `#4ade80`, rose `#f43f5e`) is an asset, not a token of this system.

## Overview

**Creative North Star: "Laporan Keuangan Publikasi"**

The page is the full-page audited statement that Indonesian Tbk companies print in the newspaper. The product's own tables are the page: ruled black ink on cool newsprint, a double rule under every column header, a heavy rule closing every statement, numbered "Catatan" notes, and a signature block with a stamped action. One corporate band of idx green owns the masthead; everything below it is ink. There are no cards, no shadows, no rounded surfaces, no hero copy beside a screenshot. With the content removed the sheet still reads as a printed statement: rules, bands, column grid.

Density is newspaper density. Rows pack at 0.34rem vertical padding with no card padding around them; ticker symbols are set heavier and slightly larger than the row they lead so the board scans like a listings column. Everything is set in one variable face, Archivo: condensed black weights for mastheads and section strips in uppercase, regular weights with tabular lining figures for numbers and prose. Monospace appears only where the content is a literal command, tool name, JSON or the terminal prompt.

The one authored motion is the page coming off the press: on load the strip rules wipe in left to right once, and every minute the board refetches and cells whose value changed darken as if freshly inked, then settle. Both are disabled or absent under reduced motion. Visual rejections confirmed by the build: no cream paper, no serif, no gradient, no drop shadow, no border radius on any surface, no feature cards, no icon set.

**Key Characteristics:**
- Cool newsprint ground (`paper`) with near-black ink; rules carry the structure, not boxes
- One committed band of deep idx green at page scale (masthead, optional green strip); green ink for links and positive figures
- Red is reserved for negative figures and errors
- Archivo throughout: condensed 900/800 uppercase for mastheads and strips, tabular lining figures everywhere
- Ruled tables: 3px double rule under the header, 1px soft rules between rows, 2px heavy rule closing the statement
- Zero radius, zero shadow; the only rotation on the page is the stamp
- Motion is print, not UI: a one-time rule wipe and a 1.4s re-ink of changed cells

## Colors

A two-ink palette on newsprint: black ink does the work, deep green is the single corporate colour, red marks losses.

### Primary
- **Deep idx green** (`green-deep`): the committed band. Fills the masthead edge to edge and the optional `.strip.green` section head; the active state of the stamp and the hover state of the terminal's Run button; the prompt caret; the focus ring. This is the green the page is built from.
- **Green ink** (`green-ink`): green pushed dark enough to print as text on paper. Links at rest and every positive figure (`.up`). Also the positive sparkline stroke.
- **idx green** (`green`): the brand's bright green from `mark.svg`. On the page it appears only as the text-selection highlight; it is not a surface colour here.
- **Green tint** (`green-tint`): the flash behind a freshly re-inked cell at the start of the `reink` animation, fading to transparent.

### Neutral
- **Paper** (`paper`): the page ground and the text colour of anything printed on ink (stamp hover, Run button, Copy hover). Cool, never cream.
- **Paper deep** (`paper-deep`): scrollbar track only.
- **Ink** (`ink`): body text, headings, the stamp border, the 3px masthead underline, inverted hover fills.
- **Ink 2** (`ink-2`): column headers, company names, row labels, terminal warnings and `dt` keys. The second-pass grey of a halftone.
- **Ink 3** (`ink-3`): captions, provenance footers, strip subtitles, the disclaimer, scroll cues. Quiet but still AA on paper.
- **Rule** (`rule`): every structural rule: the 3px strip top, the 3px double header rule, the 2px closing rule, the 1px box border.
- **Rule soft** (`rule-soft`): the 1px hairline between table rows and note paragraphs, the prompt input underline at rest, the hint chip border, the dashed sparkline baseline.
- **Band text / band text soft** (`band-text`, `band-text-soft`): white and a pale green-white for text printed on the green band (title, dateline, nav).

### Semantic
- **Loss red** (`red`): negative figures (`.dn`), the negative sparkline stroke, terminal error lines. The stylesheet's `--stamp` property shares this value and colours the decorative `[PUBLIC]` stamp in the landing footer.

### Named Rules
**The One Band Rule.** Deep green is applied at page scale exactly once per page, as the masthead band; a second green strip is permitted for a section head but never for a card, a button fill at rest, or a background tint. Below the band, green is ink only (links, gains).

**The Red Means Down Rule.** Red appears on figures that fell and on errors. It is never a highlight, a brand colour, or a call to action.

**The Ink On Paper Rule.** Interactive emphasis is inversion, not tinting: hover prints paper on ink (`stamp-hover`, `button-copy-hover`); there are no translucent, pastel or alpha-blended hover fills.

## Typography

**Display Font:** Archivo variable (Google Fonts, `wdth` 62–125, `wght` 100–900) with Helvetica Neue, Arial, sans-serif
**Body Font:** Archivo, same file
**Label/Mono Font:** ui-monospace, Cascadia Mono, Menlo, Consolas, monospace

**Character:** One grotesk doing every job, the way a newspaper's statement page is set in a single family. Condensing to `wdth` 75 at 800–900 gives the black masthead and section heads; the same face at 400–700 with tabular lining figures gives the board. Negative tracking on the large figures and symbols, positive tracking on the uppercase heads.

### Hierarchy
- **Masthead** (900, condensed `wdth` 75, `clamp(1.3rem, 2.2vw, 1.6rem)`, 1.35, uppercase, +0.04em): the statement title in the green band ("LAPORAN PASAR", "TERMINAL"). Its subtitle is a 500-weight span at `clamp(0.85rem, 1.2vw, 0.95rem)` in `band-text-soft`.
- **Strip** (800, condensed, 1.05rem, uppercase, +0.04em): section heads set on a 3px-over-1px ruled strip ("PAPAN HARGA", "CATATAN"). A 500-weight 0.8rem subtitle in `ink-3` sits at the right of the strip, baseline-aligned.
- **Figure display** (800, `clamp(1.6rem, 3vw, 2.2rem)`, 1, -0.02em, tabular): the composite index price on the index line. The only large number on the page.
- **Symbol** (800, 1.02rem, -0.01em): the four-letter ticker leading each row, set heavier and larger than the row body.
- **Headline** (800, 1.05 line height): the default for `h1`–`h3`; statement titles (`.stmt-title`, 0.8rem uppercase, 1px rule beneath) are the small case of this.
- **Body** (400, 0.95rem, 1.45): keterangan lines, notes, terminal prose (max 72ch). Base page size is 16px; body copy inside statements steps down to 0.95rem.
- **Table** (400, 0.9rem; 0.84rem under 46rem): board cells with tabular lining figures, right-aligned, no wrapping.
- **Label** (700–800, 0.76rem, uppercase, +0.04em to +0.06em, `ink-2`): column headers, box labels ("INSTALL · LINUX, MACOS"), the index-line row label, the prompt label, Copy and Run buttons.
- **Caption** (500, 0.8–0.82rem, `ink-3`): strip subtitles, provenance footers, datelines, the disclaimer, terminal entry metadata.
- **Mono** (0.76–0.85rem): literal commands in `<code>` and `<pre>`, MCP tool names, the terminal command caption (700), the prompt input (1rem), hint chips, JSON output.

### Named Rules
**The Literal Mono Rule.** Monospace is used only for text a user could type or paste: commands, tool names, JSON, the prompt. Never for labels, headings or numbers in tables.

**The Tabular Rule.** `font-variant-numeric: tabular-nums lining-nums` is set on `body` and inherited; every figure in every column aligns without per-cell styling.

**The Condensed Head Rule.** The condensed axis (`wdth` 75) is reserved for uppercase heads, the stamp and note numerals. Body, table and figure text stay at normal width.

## Layout

The sheet is a single centred column `min(84rem, 100% - 2 * gutter)` with a fluid gutter `clamp(1rem, 3vw, 2.5rem)`. The masthead band runs edge to edge and carries the same `.wrap` inside it as a three-column grid (brand · title · dateline). Below, `.sheet` pads 1.5rem top and 3rem bottom.

The first viewport is a two-column lead: `minmax(0, 2fr)` for the board, `minmax(17rem, 1fr)` for the keterangan aside, 2.5rem column gap and no row gap, both tracks aligned to start. Lower sections (`.section`) are separated by 2.5rem of air and begin with a strip; the movers and the three-form overview are `repeat(3, minmax(0, 1fr))` grids with a 2rem gap; the notes run as two CSS columns with a 2.5rem gap and a CSS counter. The signature block is `1fr auto`, separated by a 3px rule and 3rem of space.

Vertical rhythm comes from rules rather than margins: rows at 0.34rem padding, strips at 0.35rem, note paragraphs at 0.5rem, box internals at 0.45–0.6rem, statement blocks at 0.6rem. Spacing steps that recur: 0.25, 0.34, 0.4, 0.5, 0.75, 1, 1.5, 2, 2.5, 3rem.

Breakpoints (max-width): at 64rem the lead, the three-up grids and the notes collapse to one column and the aside gains 2rem top margin. At 46rem the board hides its Name and Volume columns so five columns fit without horizontal scroll, the masthead becomes two columns with the title dropping to a full-width third row, the table steps to 0.84rem, the signature stacks, and the terminal prompt puts its label on its own row. Tables that still overflow get `overflow-x: auto`, a right-edge fade mask while more columns remain, and a text scroll cue.

The terminal page is the same sheet as a flex column filling the viewport: output grows, each result is an `.entry` opened by a 3px rule, and the prompt form is sticky to the bottom on paper with a 3px rule above and 1px below.

## Elevation & Depth

Flat. No `box-shadow` is declared anywhere in the stylesheet and no surface is tinted to suggest lift. Depth is conveyed by rule weight: 3px for the top of a statement, 3px double under a header, 2px to close, 1px soft between rows. The sticky prompt is distinguished from the output only by its rules and the paper background. The single transient tone is the `reink` flash, a green tint that fades over 1.4s behind a changed cell.

### Named Rules
**The Rule Weight Rule.** Hierarchy is 3 / 3-double / 2 / 1px in `rule` and `rule-soft`. Add a rule before adding a border, and a border before adding a background.

**The No Shadow Rule.** Nothing casts a shadow. Overlap is not part of the vocabulary; the page is one sheet.

## Shapes

Everything is square. Buttons, boxes, chips, inputs and the stamp have 0 radius. The one exception is the 34px brand mark, whose `7px` radius and 1px 35%-white border follow the rounded square of `mark.svg`. Borders are solid ink at 1px (boxes, copy button), 2px (small stamp, pill, closing rules) or 3px (primary stamp). The stamp is the page's only rotated element: -1.5° for the primary action, +2° for the small red `[PUBLIC]` stamp, -1° for terminal pills. The table scroll fade is a mask, not a shape. Sparklines and the tape are thin SVG polylines (1.6px, non-scaling) over a dashed baseline, inside the column grid.

## Components

### Buttons
- **Shape:** square (0 radius), no shadow.
- **Stamp (primary action):** transparent fill, 3px `ink` border, `ink` text at 900 condensed uppercase 1.1rem with +0.08em tracking, padding 0.7rem 1.1rem, rotated -1.5°. One per page.
- **Hover / Focus:** inverts to `ink` fill with `paper` text over 120ms ease-out; `:focus-visible` adds the global 2px `green-deep` outline at 2px offset. `:active` fills `green-deep` with white text.
- **Run (terminal submit):** `ink` fill, `paper` text, 0.78rem 800 uppercase +0.06em, padding 0.45rem 0.8rem, no border; hover fills `green-deep`.
- **Copy (inside ruled boxes):** transparent, 1px `rule` border, 0.76rem 700 uppercase label, padding 0.2rem 0.5rem; hover inverts to ink on paper.
- **Hint chips (terminal):** transparent, 1px `rule-soft` border, mono 0.8rem in `ink-2`; hover darkens border and text to `ink`.

### Links
`green-ink` with a 1px underline at 0.16em offset; hover turns to `ink`. On the band, links are white with no colour change.

### Cards / Containers
- There are no cards. The container is the **ruled box** (`.box`): 1px `rule` border, 0.6rem 0.75rem padding, a 0.76rem uppercase label, a `pre` at 0.78rem, and a right-aligned Copy button in a 0.45rem grid gap.
- **Statement** (`.stmt`): a 0.8rem uppercase title over a 1px rule, then a table or `pre` closed by a 2px rule.
- **Terminal entry** (`.entry`): opened by a 3px rule, a caption row (mono command at 700, meta in `ink-3` pushed right), then a body that may be a table, a `dl` grid of key/values (dt in `ink-2`, dd at 600), a 72ch prose block, a reasons list, or a JSON `pre` capped at 28rem with 1px top and 2px bottom rules.

### Inputs / Fields
- **Prompt input:** transparent, no border except a 1px `rule-soft` underline, mono 1rem in `ink`, `green-deep` caret, outline suppressed in favour of the underline.
- **Focus:** underline darkens to `ink`. The form itself is sticky with a 3px rule above.
- **Label:** "Perintah · command" at 0.78rem 800 uppercase.

### Navigation
- The masthead is the only navigation: brand (mark + 1.45rem 800 wordmark "idx" at -0.02em) on the left, statement title centred, dateline right in 0.82rem `band-text-soft` with the time in white 700. Nav links in the band are white at 0.88rem with 1rem left margin. On mobile the title drops below as a full-width left-aligned row. No menu, no hamburger.

### Ruled Table (signature)
`.tbl` is the page's unit. 100% width, collapsed borders, 0.9rem; header cells 0.76rem 700 uppercase in `ink-2`, right-aligned, padded 0.45rem 0.4rem 0.3rem, under a 3px double `rule`; body cells padded 0.34rem 0.4rem, right-aligned, no wrap, 1px `rule-soft` beneath; the last row closes with a 2px `rule`. First column (and any `.l`) is left-aligned. `.sym` sets the ticker at 1.02rem 800; `.name` is `ink-2` and truncates at 16rem (8rem on mobile); `.up` / `.dn` colour the change columns; `.muted` is `ink-3`. Statement tables inside `.stmt` left-align and wrap. A provenance `.foot` line in 0.8rem `ink-3` follows every live table.

### Index line and tape (signature)
Above the board: a baseline-aligned flex row with the uppercase 0.76rem row label, the 800-weight figure display price, a 1rem 700 change in `.up`/`.dn`, and breadth counts pushed right in `ink-3`, closed by a 1px soft rule. Beneath it a 44px full-width SVG tape; terminal entries use a 36px sparkline. Strokes are `green-ink` or `red` at 1.6px over a dashed `rule-soft` baseline.

### Strip (signature)
The section head: 3px `rule` on top, 1px beneath, 0.35rem padding, condensed uppercase 1.05rem 800 title left and a 500-weight 0.8rem `ink-3` subtitle right, baseline-aligned. `.strip.green` prints the same strip as a `green-deep` band with white text and 0.4rem 0.6rem padding. With `body.draw` and motion allowed, the strip's top rule wipes in from left to right over 900ms (`cubic-bezier(0.16, 1, 0.3, 1)`), once.

### Pill (terminal verdicts)
Inline, 2px `currentColor` border, 0.8rem 800 uppercase +0.06em, padding 0.05rem 0.45rem, rotated -1°. Inherits the semantic colour of its text (`.up`, `.dn`, ink).

## Do's and Don'ts

### Do:
- **Do** start every statement, section or terminal entry with a 3px `rule` and close tables with a 2px `rule`; keep 1px `rule-soft` for rows.
- **Do** set numbers in Archivo with tabular lining figures, right-aligned, and lead each row with the ticker at 1.02rem 800.
- **Do** keep the green band to the masthead (and at most one `.strip.green`); use `green-ink` for links and gains below it.
- **Do** use `red` only for negative figures and errors.
- **Do** make hover and press states by inverting ink and paper within 120ms; keep `:focus-visible` as the 2px `green-deep` outline.
- **Do** set uppercase heads in the condensed axis (`wdth` 75) at 800–900 with +0.04em to +0.08em tracking.
- **Do** reserve monospace for literal commands, tool names, JSON and the prompt.
- **Do** honour `prefers-reduced-motion`: the rule wipe is gated on `no-preference`; nothing else moves except the 1.4s re-ink of changed cells.
- **Do** collapse to one column at 64rem and drop the Name and Volume columns at 46rem instead of forcing a horizontal scroll on the board.

### Don't:
- **Don't** add `box-shadow`, gradients, translucent fills or a border radius to any surface; only the 34px brand mark is rounded.
- **Don't** introduce cards, feature grids, hero screenshots or an icon set; the table is the component.
- **Don't** use cream, warm grey or pure white as the page ground; paper is `paper` and the only white is text on the green band.
- **Don't** use bright `green` as a fill or text colour on the sheet; it exists only in the brand mark and the selection highlight.
- **Don't** colour a figure or label red for emphasis, warning badges or branding.
- **Don't** add a second typeface, a serif, or a mono label style; Archivo carries every role.
- **Don't** add a second rotated or stamped element to a page that already has its primary stamp.
- **Don't** restyle the CLI's shell tables or the README logo to match this file; they are out of scope.
