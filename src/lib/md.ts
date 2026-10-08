// Mini-markdown for message bodies (ported from the prototype's HC.md).
// Everything from the message is HTML-escaped before any markup is added, so
// the result is safe for dangerouslySetInnerHTML. Supported: paragraphs,
// headings (rendered bold), lists, quotes, tables, fenced code with Copy,
// inline code, bold, italic, links and @net: addresses.

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
export const esc = (s: unknown) => String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ESC[c]);

const COPY_SVG =
  '<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/></svg>';

function inline(src: string): string {
  const codes: string[] = [];
  let s = String(src).replace(/`([^`]+)`/g, (_, c: string) => { codes.push(c); return "\u0000" + (codes.length - 1) + "\u0000"; });
  s = esc(s);
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  s = s.replace(/(^|[^*\w])\*([^*\s][^*]*?)\*(?!\w)/g, "$1<em>$2</em>");
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a class="md-a" data-href="$2" title="$2">$1</a>');
  s = s.replace(/(^|[\s(])(https?:\/\/[^\s<]+[^\s<.,;:!?)])/g, '$1<a class="md-a" data-href="$2" title="$2">$2</a>');
  s = s.replace(/@net:([a-z0-9][a-z0-9._-]*[a-z0-9])/g, '<a class="addr" data-slug="$1">@net:$1</a>');
  s = s.replace(/\u0000(\d+)\u0000/g, (_, i: string) => "<code>" + esc(codes[+i]) + "</code>");
  return s;
}

function codeBlock(text: string, lang: string): string {
  const lines = text.split("\n").map((l) => {
    const e = esc(l);
    if (lang === "diff" && /^\+/.test(l)) return '<span class="d-add">' + e + "</span>";
    if (lang === "diff" && /^-/.test(l)) return '<span class="d-del">' + e + "</span>";
    return e;
  }).join("\n");
  return '<div class="code"><div class="code-h"><span>' + esc(lang || "text") + "</span>" +
    '<button class="code-copy" data-act="copy-code" title="Copy code">' + COPY_SVG + "<span>Copy</span></button></div>" +
    "<pre><code>" + lines + "</code></pre></div>";
}

function table(rows: string[]): string {
  const cells = (r: string) => r.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
  const head = cells(rows[0]);
  const body = rows.slice(1).filter((r) => !/^\s*\|?\s*:?-{2,}/.test(r)).map(cells);
  return '<div class="md-table"><table><thead><tr>' + head.map((c) => "<th>" + inline(c) + "</th>").join("") + "</tr></thead><tbody>" +
    body.map((r) => "<tr>" + r.map((c) => "<td>" + inline(c) + "</td>").join("") + "</tr>").join("") + "</tbody></table></div>";
}

const BLOCK_START = /^(```|\s*\||\s*[-*] |\s*\d+\. |> ?|#{1,3} )/;

/** Markdown to (escaped) HTML. */
export function md(src: string): string {
  const lines = String(src || "").replace(/\r/g, "").split("\n");
  const out: string[] = []; let i = 0;
  while (i < lines.length) {
    const L = lines[i];
    if (/^```/.test(L)) {
      const lang = L.slice(3).trim(); const buf: string[] = []; i++;
      while (i < lines.length && !/^```/.test(lines[i])) buf.push(lines[i++]);
      i++; out.push(codeBlock(buf.join("\n"), lang)); continue;
    }
    if (/^\s*\|/.test(L)) { const rows: string[] = []; while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(lines[i++]); out.push(table(rows)); continue; }
    if (/^\s*[-*] /.test(L)) { const it: string[] = []; while (i < lines.length && /^\s*[-*] /.test(lines[i])) it.push(lines[i++].replace(/^\s*[-*] /, "")); out.push("<ul>" + it.map((t) => "<li>" + inline(t) + "</li>").join("") + "</ul>"); continue; }
    if (/^\s*\d+\. /.test(L)) { const it: string[] = []; while (i < lines.length && /^\s*\d+\. /.test(lines[i])) it.push(lines[i++].replace(/^\s*\d+\. /, "")); out.push("<ol>" + it.map((t) => "<li>" + inline(t) + "</li>").join("") + "</ol>"); continue; }
    if (/^> ?/.test(L)) { const it: string[] = []; while (i < lines.length && /^> ?/.test(lines[i])) it.push(lines[i++].replace(/^> ?/, "")); out.push("<blockquote>" + it.map(inline).join("<br>") + "</blockquote>"); continue; }
    if (/^#{1,3} /.test(L)) { out.push('<p class="md-h">' + inline(L.replace(/^#{1,3} /, "")) + "</p>"); i++; continue; }
    if (!L.trim()) { i++; continue; }
    const buf: string[] = [];
    while (i < lines.length && lines[i].trim() && !(buf.length && BLOCK_START.test(lines[i]))) buf.push(lines[i++]);
    out.push("<p>" + buf.map(inline).join("<br>") + "</p>");
  }
  return out.join("");
}

/** Plain one-line text for previews. */
export const plain = (src: string) =>
  String(src || "").replace(/```[\s\S]*?```/g, "[code]").replace(/[*_`>#|]/g, "").replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/\s+/g, " ").trim();
