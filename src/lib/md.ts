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
  return u ? '<a class="md-a" data-href="' + esc(u) + '" title="' + esc(u) + '">' + inner + "</a>" : inner;
};

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
    name: "addr",
    level: "inline",
    start: (s: string) => { const i = s.indexOf("@net:"); return i < 0 ? undefined : i; },
    tokenizer(src: string) {
      const m = /^@net:([a-z0-9][a-z0-9._-]*[a-z0-9])/.exec(src);
      return m ? { type: "addr", raw: m[0], slug: m[1] } : undefined;
    },
    renderer: (t: Tokens.Generic) => '<a class="addr" data-slug="' + esc(t.slug) + '">@net:' + esc(t.slug) + "</a>",
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
  ALLOWED_ATTR: ["class", "title", "data-href", "data-slug", "data-act", "start", "viewBox", "d", "aria-hidden"],
  ALLOW_DATA_ATTR: false,
  ALLOW_ARIA_ATTR: false,
  ALLOW_UNKNOWN_PROTOCOLS: false,
};
// the only attributes that can lead anywhere: data-href must be http(s)
DOMPurify.addHook("afterSanitizeAttributes", (n) => {
  if (n.hasAttribute("data-href") && !safeUrl(n.getAttribute("data-href"))) n.removeAttribute("data-href");
  if (n.nodeName === "A" && n.classList.contains("md-a") && !n.hasAttribute("data-href")) n.removeAttribute("class");
  if (n.hasAttribute("data-act") && n.getAttribute("data-act") !== "copy-code") n.removeAttribute("data-act");
});

/** The second wall, on its own: only the allow-listed tags and attributes survive. */
export const clean = (html: string): string => DOMPurify.sanitize(html, PURIFY) as unknown as string;

/** Markdown to HTML that is safe for dangerouslySetInnerHTML. */
export function md(src: string): string {
  return clean((marked.parse(String(src || "").replace(/\r\n?/g, "\n"), { async: false }) as string).trim());
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
