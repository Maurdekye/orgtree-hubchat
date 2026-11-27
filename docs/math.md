# LaTeX math in messages

User 2026-11-27 m13da5b728a34: "does hubchat not support latex? it should". Docket
hubchat-render-latex-math-in-messages-like-orgtr. Hubchat 1.4.1.

## What you see

- **Inline math**: `$...$` or `\(...\)` typesets inside the line, e.g. `$e^{i\pi} + 1 = 0$`.
- **Display math**: `$$...$$` or `\[...\]` typesets as its own centred block. Alone on its lines it is a block;
  inside a line it is still a display formula. A formula wider than the bubble scrolls inside it.
- **Text stays text**: plain amounts (`$5 and $10`), shell names (`$PATH`, `${HOME}`, `$(date)`), `US$5`, code spans
  and fenced code are never math. A formula KaTeX can't read shows exactly as typed, never as an error box.
- The chat list and reply quotes show the formula's source with its delimiters. Copy (the message menu or Android's
  Copy) copies the message text as sent. A selection copied across a formula puts its TeX source on the clipboard
  (KaTeX's copy-tex), written as `$...$` inline and `$$...$$` for display, whichever delimiter was typed.

Same on desktop, Android and Hubchat Web. Nothing is fetched: KaTeX and its fonts are in the app.

## Rules (from Orgtree)

The delimiter rules are Orgtree's, copied as they are (`apps/desktop/renderer/src/canvas/markdownmath.ts`
at 4.7.1) into `src/lib/math.ts`, so a message reads the same in both apps. KaTeX is the same version (0.16.47)
with Orgtree's options: `trust: false` (no `\href`, `\includegraphics`), `strict: "error"`, `maxExpand: 1000`,
`maxSize: 20`, fresh `macros` per formula (no `\gdef` carries over), output `htmlAndMathml`.

Deliberate differences from Orgtree:
- A copied selection uses copy-tex's `$`/`$$` delimiters; Orgtree's own selection map keeps the delimiter as typed.
- Hubchat has no `&lt;` step: its raw HTML is text before marked sees it, so the formula is the source itself.

## Safety

KaTeX's output passes the same DOMPurify wall as all message markup (`src/lib/md.ts`). The MathML tags, `style`
and the MathML/SVG sizing attributes are allowed, but attributes are kept only under a `.katex` root, and a
`style` there is dropped if it holds `url(`, `expression(`, `@import` or a backslash. A message's own markup can't
make any of it: raw HTML in a message is shown as text. Fonts are never inlined as `data:` URLs
(`assetsInlineLimit` in both Vite configs), because Hubchat Web's CSP allows fonts only from `'self'`.

## Tests

- `node --test tools/math.test.ts`: the delimiter rules (6 tests).
- `node tools/proof-math.mjs` (kernel Job, Session 0, output under C:/orgtree-build): the mounted App on a
  synthetic chat, desktop and Android, dark and light, 10 checks each: inline and display typeset, a long
  formula scrolls in its bubble, amounts/code/fences/broken LaTeX stay text, `\color` survives the sanitizer
  while a typed tag stays text, KaTeX's fonts load locally, the chat list shows the source, and a copied
  selection carries the TeX (a page-made ClipboardEvent; the real clipboard is never touched). Red
  (`HUBCHAT_PROOF_PHASE=red`) expects exactly 6 failures at the source before math, and captures
  `before-*.png`; green captures `after-*.png`.
- Not measured: Android's WebView and WebKitGTK rendering (the proof uses Chromium in both layouts).
