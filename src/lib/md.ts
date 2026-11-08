// GitHub-flavored markdown for message bodies (user 2026-10-09 08:44Z).
// Message text comes from other people and agents, so it is untrusted, and
// it gets two walls before it reaches dangerouslySetInnerHTML:
//   1. marked parses it, and our renderer turns raw HTML into plain escaped
//      text, drops every link that is not http(s), and never loads a remote
//      picture (a link to it instead);
//   2. DOMPurify then keeps only the tags and attributes listed below, so a
//      slip in (1) still cannot ship a script, a style, an iframe or an
//      event handler.
// Links carry their target in data-href, never href: MessageView's click
// handler sends it to the system browser through openLink, which is http(s)
// only, and the WebView itself never navigates.
import { Marked, type MarkedToken, type Token, type Tokens } from "marked";
import DOMPurify from "dompurify";
import { resolveMentionTarget } from "./mentions";
import { chipHtml, nextOref, nodeChip, nodeNameAtStart, NO_OREF_CONTEXT, orefAtStart, parseOref, tokenChip, type OrefContext } from "./orgtreeRefs";

// The message being rendered: what its Orgtree mention chips may do (lib/orgtreeRefs.ts).
// Set for one synchronous parse by md(), and back to "nothing opens" after it.
let orefCtx: OrefContext = NO_OREF_CONTEXT;

/** Where the next bare `@name` that could be an Orgtree node starts: only in a message
 *  that has an org, and never a contact's name, which stays the contact's mention. */
const NODE_SCAN_RE = /(?:^|[^0-9A-Za-z._%+@:/-])@[a-z0-9]/g;
function nextNodeName(s: string): number | undefined {
  if (!orefCtx.nodes) return undefined;
  NODE_SCAN_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = NODE_SCAN_RE.exec(s)) !== null) {
    const at = m.index + m[0].length - 2;
    const name = nodeNameAtStart(s.slice(at));
    if (name && !resolveMentionTarget("@" + name)) return at;
    NODE_SCAN_RE.lastIndex = at + 1;
  }
  return undefined;
}

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
export const esc = (s: unknown) => String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ESC[c]);

const COPY_SVG =
  '<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/></svg>';

/** The one kind of link a message may carry: http(s), checked twice. */
function safeUrl(href: string | null | undefined): string | null {
  const h = String(href || "").trim();
  if (!/^https?:\/\//i.test(h)) return null;
  try { const p = new URL(h).protocol; return p === "http:" || p === "https:" ? h : null; } catch { return null; }
}

function codeBlock(text: string, lang: string): string {
  const lines = text.replace(/\n$/, "").split("\n").map((l) => {
    const e = esc(l);
    if (lang === "diff" && /^\+/.test(l)) return '<span class="d-add">' + e + "</span>";
    if (lang === "diff" && /^-/.test(l)) return '<span class="d-del">' + e + "</span>";
    return e;
  }).join("\n");
  return '<div class="code"><div class="code-h"><span>' + esc(lang || "text") + "</span>" +
    '<button class="code-copy" data-act="copy-code" title="Copy code">' + COPY_SVG + "<span>Copy</span></button></div>" +
    "<pre><code>" + lines + "</code></pre></div>";
}

const link = (href: string, inner: string) => {
  const u = safeUrl(href);
  return u ? '<a class="md-a" data-href="' + esc(u) + '" title="' + esc(u) + '" tabindex="0" role="link">' + inner + "</a>" : inner;
};

const MENTION_SCAN_RE = /(?:^|[^0-9A-Za-z._%+-])(@(?:(?:net|org):)?[A-Za-z0-9](?:(?:\.|[_-]+)?[A-Za-z0-9])*)/gi;
const MENTION_HEAD_RE = /^@(?:(?:net|org):)?[A-Za-z0-9](?:(?:\.|[_-]+)?[A-Za-z0-9])*/i;

const marked = new Marked({
  gfm: true,
  // a chat line break is a line break, as it always was here
  breaks: true,
  tokenizer: {
    // Plain text must stay plain: a message that merely starts with spaces is
    // not a code block, and a line over "---" is not a heading.
    code: () => undefined,
    lheading: () => undefined,
    // Raw HTML is only ever text here. As a block it would swallow every
    // line up to the next blank one, markdown included; as inline it is
    // escaped where it stands.
    html: () => undefined,
  },
  extensions: [{
    // An Orgtree mention token (`@item:org/slug` and its kin): a chip, before the
    // contact mentions so `@app:…` never reads as a contact named "app".
    name: "oref",
    level: "inline",
    start: (s: string) => nextOref(s),
    tokenizer(src: string) {
      const ref = orefAtStart(src);
      return ref ? { type: "oref", raw: ref.token, html: chipHtml(tokenChip(ref, orefCtx)) } : undefined;
    },
    renderer: (t: Tokens.Generic) => String(t.html),
  }, {
    name: "addr",
    level: "inline",
    start: (s: string) => {
      MENTION_SCAN_RE.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = MENTION_SCAN_RE.exec(s)) !== null) {
        const raw = m[1]!;
        const at = m.index + (m[0].length - raw.length);
        const next = s[at + raw.length];
        if (next && /[0-9A-Za-z@]/.test(next)) continue;
        if (resolveMentionTarget(raw)) return at;
      }
      return undefined;
    },
    tokenizer(src: string) {
      const m = MENTION_HEAD_RE.exec(src);
      if (!m) return undefined;
      const raw = m[0];
      const next = src[raw.length];
      if (next && /[0-9A-Za-z@]/.test(next)) return undefined;
      const resolved = resolveMentionTarget(raw);
      if (!resolved) return undefined;
      return { type: "addr", raw, slug: resolved.slug, label: resolved.label, title: resolved.title };
    },
    renderer: (t: Tokens.Generic) =>
      '<a class="addr mention" data-slug="' + esc(t.slug) + '" title="' + esc(t.title || t.label || ("@net:" + t.slug)) + '" tabindex="0" role="link">' + esc(t.label || ("@net:" + t.slug)) + "</a>",
  }, {
    // A bare `@name` that is no contact, in a message with an org: that org's agent or
    // project, as one generic chip (user 2026-11-08 m0dc5cb44bd7f, m7c183781895c Q1/Q3).
    name: "onode",
    level: "inline",
    start: (s: string) => nextNodeName(s),
    tokenizer(src: string) {
      const name = orefCtx.nodes ? nodeNameAtStart(src) : null;
      if (!name || resolveMentionTarget("@" + name)) return undefined;
      const chip = nodeChip(name, orefCtx);
      return chip ? { type: "onode", raw: "@" + name, html: chipHtml(chip) } : undefined;
    },
    renderer: (t: Tokens.Generic) => String(t.html),
  }],
  renderer: {
    html: ({ text, block }) => {
      const e = esc(text).replace(/\n/g, "<br>");
      return block ? "<p>" + e.replace(/(<br>)+$/, "") + "</p>" : e;
    },
    code: ({ text, lang }) => codeBlock(text, (lang || "").trim().split(/\s+/)[0] || ""),
    heading({ tokens, depth }) {
      return "<h" + depth + ' class="md-h md-h' + Math.min(depth, 3) + '">' + this.parser.parseInline(tokens) + "</h" + depth + ">";
    },
    link({ href, tokens }) { return link(href, this.parser.parseInline(tokens)); },
    // never fetch a picture a stranger points at: show its description as a link
    image: ({ href, text }) => link(href, esc(text || href)),
    checkbox: ({ checked }) => '<span class="md-task' + (checked ? " on" : "") + '" aria-hidden="true"></span> ',
    tablecell(t) {
      const tag = t.header ? "th" : "td";
      return "<" + tag + (t.align ? ' class="al-' + t.align + '"' : "") + ">" + this.parser.parseInline(t.tokens) + "</" + tag + ">";
    },
    table(t) {
      const row = (cells: Tokens.TableCell[]) => "<tr>" + cells.map((c) => this.tablecell(c)).join("") + "</tr>";
      return '<div class="md-table"><table><thead>' + row(t.header) + "</thead><tbody>" + t.rows.map(row).join("") + "</tbody></table></div>";
    },
  },
});

const PURIFY = {
  ALLOWED_TAGS: [
    "p", "br", "hr", "strong", "em", "del", "code", "pre", "a", "span", "div", "button",
    "ul", "ol", "li", "blockquote", "h1", "h2", "h3", "h4", "h5", "h6",
    "table", "thead", "tbody", "tr", "th", "td", "svg", "path",
  ],
  ALLOWED_ATTR: ["class", "title", "data-href", "data-slug", "data-oref", "data-act", "start", "viewBox", "d", "aria-hidden", "tabindex", "role", "dir"],
  ALLOW_DATA_ATTR: false,
  ALLOW_ARIA_ATTR: false,
  ALLOW_UNKNOWN_PROTOCOLS: false,
};
// the only attributes that can lead anywhere: data-href must be http(s)
DOMPurify.addHook("afterSanitizeAttributes", (n) => {
  if (n.hasAttribute("data-href") && !safeUrl(n.getAttribute("data-href"))) n.removeAttribute("data-href");
  if (n.nodeName === "A" && n.classList.contains("md-a") && !n.hasAttribute("data-href")) n.removeAttribute("class");
  if (n.hasAttribute("data-act") && n.getAttribute("data-act") !== "copy-code") n.removeAttribute("data-act");
  // an Orgtree chip's token: only on a chip, and only one the grammar reads
  if (n.hasAttribute("data-oref") && !(n.nodeName === "A" && n.classList.contains("oref") && parseOref(n.getAttribute("data-oref") || "")))
    n.removeAttribute("data-oref");
  // keyboard reach for links and addresses (they carry no href): only tabindex 0 and role link, only on them
  const linky = n.nodeName === "A" && (n.hasAttribute("data-href") || n.hasAttribute("data-slug")
    || (n.classList.contains("oref") && n.classList.contains("on") && n.hasAttribute("data-oref")));
  if (n.hasAttribute("tabindex") && !(linky && n.getAttribute("tabindex") === "0")) n.removeAttribute("tabindex");
  if (n.hasAttribute("role") && !(linky && n.getAttribute("role") === "link")) n.removeAttribute("role");
  // each block takes its own direction (Arabic, Hebrew), without mirroring the layout
  if (/^(P|LI|H[1-6]|BLOCKQUOTE|TD|TH)$/.test(n.nodeName)) n.setAttribute("dir", "auto");
  else if (n.hasAttribute("dir")) n.removeAttribute("dir");
});

/** The second wall, on its own: only the allow-listed tags and attributes survive. */
export const clean = (html: string): string => DOMPurify.sanitize(html, PURIFY) as unknown as string;

/** Markdown to HTML that is safe for dangerouslySetInnerHTML. `oref`: what this message's
 *  Orgtree mention chips may do (lib/orgtreeRefs.ts orefContext); without it they only show. */
export function md(src: string, oref: OrefContext = NO_OREF_CONTEXT): string {
  orefCtx = oref;
  try {
    return clean((marked.parse(String(src || "").replace(/\r\n?/g, "\n"), { async: false }) as string).trim());
  } finally {
    orefCtx = NO_OREF_CONTEXT;
  }
}

/** Bundled guide only: preserve local section links without widening the
 * message renderer's http(s)-only policy. The private URL survives the same
 * sanitizer, then becomes a non-navigating data target handled by GuideBody. */
export function guideMd(src: string): string {
  const prefix = "https://hubchat-guide.invalid/";
  const html = marked.parse(src, { async: false, walkTokens(token) {
    if (token.type === "link" && token.href.startsWith("#")) token.href = prefix + token.href;
  } }) as string;
  return clean(html.trim()).split(`data-href="${prefix}#`).join('data-href="#')
    .split(`title="${prefix}#`).join('title="#');
}

// --- plain one-line text, for the chat list and quotes ----------------------

function flat(ts: Token[] | undefined): string {
  let out = "";
  for (const tok of ts || []) {
    const t = tok as MarkedToken;
    switch (t.type) {
      case "code": out += " [code] "; break;
      case "html": out += t.raw; break;
      case "image": out += t.text; break;
      case "hr": case "br": case "space": out += " "; break;
      case "def": case "checkbox": break;
      case "table": out += " " + [t.header, ...t.rows].map((r) => r.map((c) => flat(c.tokens)).join(" ")).join(" ") + " "; break;
      case "list": out += " " + t.items.map((i) => flat(i.tokens)).join(" ") + " "; break;
      case "paragraph": case "heading": case "blockquote": out += " " + flat(t.tokens) + " "; break;
      default: {
        const g = t as Tokens.Generic;
        out += g.tokens ? flat(g.tokens) : String(g.text ?? g.raw ?? "");
      }
    }
  }
  return out;
}

const plainCache = new Map<string, string>();
/** Plain one-line text for previews: the words, without the markdown. */
export function plain(src: string): string {
  const s = String(src || "");
  const hit = plainCache.get(s);
  if (hit !== undefined) return hit;
  const out = flat(marked.lexer(s.replace(/\r\n?/g, "\n"))).replace(/\s+/g, " ").trim();
  if (plainCache.size > 400) plainCache.clear();
  plainCache.set(s, out);
  return out;
}
