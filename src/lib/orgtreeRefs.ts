// Orgtree's mentions, drawn as chips (user 2026-11-08, docket
// hubchat-render-orgtree-mentions-as-chips-linking). The contract both apps
// implement is Orgtree's docs/rust-engine/orgtree-links.md.
//
// An agent writes `@item:<org>/<slug>`, `@doc:`, `@app:`, `@panel:`, `@mail:`,
// `@agent:`/`@project:` and bare `@name`. Hubchat draws each as a chip from the
// TEXT ALONE (it fetches nothing from Orgtree). On the PC that runs the
// mentioned org, a click opens the target in Orgtree through an orgtree://
// link, and the right-click menu opens just the org. Everywhere else the chip
// is display-only, with a hint.
//
// ⚠ PURE: no imports. Node's own test runner loads this file directly
// (tools/orgtree-chips.test.ts), and md.ts and MessageView call it.

/** The canonical token grammar, copied from Orgtree's
 *  packages/contracts/reference-tokens.ts and pinned against its own fixture
 *  (tools/orgtree-ref-tokens.json). Lowercase segments; a node may carry
 *  `@<generation>`; a token ends at a boundary or it is not one. */
const SEG = "[a-z0-9-]+";
const NODE = "[a-z0-9-]+(?:@[0-9]+)?";
const END = "(?![A-Za-z0-9_/-])(?!@(?!(?:item|doc|agent|project|mail|app|panel):))";
const TOKEN = "@(?:(item|doc|app|panel):(" + SEG + "/" + SEG + ")"
  + "|(agent|project):(" + SEG + "/" + NODE + ")"
  + "|(mail):(" + SEG + "/(?:user|org)/" + SEG + "|" + SEG + "/node/" + NODE + "/" + SEG + "))" + END;
const LEAD = /[A-Za-z0-9_@-]/;

export type OrefKind = "item" | "doc" | "app" | "panel" | "agent" | "project" | "mail";

export interface Oref {
  kind: OrefKind;
  org: string;
  /** everything after `@<kind>:<org>/` */
  rest: string;
  /** what the chip shows: the target's own id (a mail's id, a node's name) */
  id: string;
  /** the token as written */
  token: string;
}

/** A whole string that is exactly one token, or null. Never a guess. */
export function parseOref(token: string): Oref | null {
  const m = new RegExp("^" + TOKEN + "$").exec(String(token ?? ""));
  if (!m) return null;
  const kind = (m[1] ?? m[3] ?? m[5]) as OrefKind;
  const path = String(m[2] ?? m[4] ?? m[6]);
  const at = path.indexOf("/");
  const org = path.slice(0, at), rest = path.slice(at + 1);
  return { kind, org, rest, id: rest.slice(rest.lastIndexOf("/") + 1), token: m[0] };
}

/** The token at the very start of `src`, as marked's tokenizer asks. */
export function orefAtStart(src: string): Oref | null {
  const m = new RegExp("^" + TOKEN).exec(src);
  return m ? parseOref(m[0]) : null;
}

/** Every token in `text`, in order, with Orgtree's boundary rules: one inside a
 *  word is not a token, two written back to back both are. `first`: stop at one. */
export function scanOrefs(text: string, first = false): { index: number; ref: Oref }[] {
  const s = String(text ?? "");
  const re = new RegExp(TOKEN, "g");
  const out: { index: number; ref: Oref }[] = [];
  let end = -1;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s)) !== null) {
    const at = m.index;
    if (at !== 0 && at !== end && LEAD.test(s[at - 1]!)) { re.lastIndex = at + 1; continue; }
    const ref = parseOref(m[0]);
    if (!ref) { re.lastIndex = at + 1; continue; }
    out.push({ index: at, ref });
    if (first) break;
    end = at + m[0].length;
  }
  return out;
}

/** Where the next token starts in `src` (for marked's `start`), or undefined. */
export function nextOref(src: string): number | undefined {
  return scanOrefs(src, true)[0]?.index;
}

/** A bare `@name` that can name an Orgtree node: Orgtree's node alphabet, and
 *  never `@user`, which is how an agent addresses the user, not a node. */
// a name ends at a boundary; `@name:x` is the prefix of something else
// (`@net:`, `@item:`), while "@name: please" is a name and its colon
const NODE_NAME = /^@([a-z0-9](?:[a-z0-9-]*[a-z0-9])?)(?![A-Za-z0-9_@/-])(?!:[^\s])/;
export function nodeNameAtStart(src: string): string | null {
  const m = NODE_NAME.exec(src);
  return m && m[1] !== "user" ? m[1]! : null;
}

// ---------------------------------------------------------------- context

/** An org of this PC's Orgtree (its local-orgs.json). */
export interface LocalOrg { slug: string; name: string; address: string }

/** What one message lets its chips do (contract: "What Hubchat does"). */
export interface OrefContext {
  /** a token of this org may open here */
  opens: (org: string) => boolean;
  /** the org a bare `@name` belongs to: null in a chat between people, where
   *  bare names stay text; `slug` null when that org is not on this PC */
  nodes: { slug: string | null; name: string } | null;
  /** a display name for an org slug of this PC, else the slug */
  orgName: (org: string) => string;
}

/** The rules, decided by the user 2026-11-08 m7c183781895c (Q1-Q4):
 *  - a message's org is its sender, or the org I sent it to; between people
 *    there is none;
 *  - a token opens only when that org is one of this PC's orgs BY ADDRESS and
 *    the token's org is one of this PC's slugs; my own messages to people
 *    count as this PC's; desktop only, and only with an Orgtree that takes
 *    orgtree:// links. */
export function orefContext(o: {
  peer: string; peerKind: string; peerName: string; outgoing: boolean;
  desktop: boolean; links: boolean; orgs: readonly LocalOrg[];
}): OrefContext {
  const org = o.peerKind === "org";
  const local = org ? o.orgs.find((x) => x.address === o.peer) ?? null : null;
  const thisPc = !!local || (o.outgoing && !org);
  const slugs = new Set(o.orgs.map((x) => x.slug));
  return {
    opens: (slug) => o.desktop && o.links && thisPc && slugs.has(slug),
    nodes: org ? { slug: local?.slug ?? null, name: local?.name || o.peerName } : null,
    orgName: (slug) => o.orgs.find((x) => x.slug === slug)?.name || slug,
  };
}

/** No message context (a preview, a guide): every chip is display-only. */
export const NO_OREF_CONTEXT: OrefContext = { opens: () => false, nodes: null, orgName: (slug) => slug };

// ---------------------------------------------------------------- chips

const KIND_WORD: Record<OrefKind | "node", string> = {
  item: "Ticket", doc: "Document", app: "App", panel: "Panel", mail: "Mail",
  agent: "Agent or project", project: "Project", node: "Agent or project",
};

/** Orgtree's own marks (its canvas/refmarks.tsx), and one generic mark for an
 *  agent or a project, which a mention cannot tell apart (user 2026-11-08
 *  m0dc5cb44bd7f): Material's AccountTreeOutlined, a node in a tree. */
export const OREF_MARKS: Record<"item" | "doc" | "app" | "panel" | "mail" | "node", string> = {
  item: "M7 15h7v2H7zm0-4h10v2H7zm0-4h10v2H7zm12-4h-4.18C14.4 1.84 13.3 1 12 1s-2.4.84-2.82 2H5c-.14 0-.27.01-.4.04-.39.08-.74.28-1.01.55-.18.18-.33.4-.43.64-.1.23-.16.49-.16.77v14c0 .27.06.54.16.78s.25.45.43.64c.27.27.62.47 1.01.55.13.02.26.03.4.03h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2m-7-.25c.41 0 .75.34.75.75s-.34.75-.75.75-.75-.34-.75-.75.34-.75.75-.75M19 19H5V5h14z",
  doc: "M8 16h8v2H8zm0-4h8v2H8zm6-10H6c-1.1 0-2 .9-2 2v16c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8zm4 18H6V4h7v5h5z",
  app: "M4 8h4V4H4zm6 12h4v-4h-4zm-6 0h4v-4H4zm0-6h4v-4H4zm6 0h4v-4h-4zm6-10v4h4V4zm-6 4h4V4h-4zm6 6h4v-4h-4zm0 6h4v-4h-4z",
  panel: "M19 4H5c-1.11 0-2 .9-2 2v12c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.89-2-2-2m0 14H5V8h14z",
  mail: "M22 6c0-1.1-.9-2-2-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2zm-2 0-8 4.99L4 6zm0 12H4V8l8 5 8-5z",
  node: "M22 11V3h-7v3H9V3H2v8h7V8h2v10h4v3h7v-8h-7v3h-2V8h2v3zM7 9H4V5h3zm10 6h3v4h-3zm0-10h3v4h-3z",
};

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ESC[c]!);

/** The hint on a chip that cannot open here (accepted copy, Q5). */
export const notHere = (org: string) => `Opens in Orgtree on the PC that runs ${org}`;

/** One chip's facts: what it shows, its token, and whether it opens. */
export interface OrefChip {
  mark: keyof typeof OREF_MARKS;
  /** the token it opens (`@agent:<org>/<name>` for a bare name), or null */
  token: string | null;
  /** the org its menu entry opens, or null when not on this PC */
  org: string | null;
  /** the text before the shown id, kept in the DOM so a copy is the original */
  hidden: string;
  id: string;
  open: boolean;
  title: string;
}

export function tokenChip(ref: Oref, ctx: OrefContext): OrefChip {
  const open = ctx.opens(ref.org);
  const mark = ref.kind === "agent" || ref.kind === "project" ? "node" : ref.kind;
  return { mark, token: ref.token, org: ref.org, hidden: ref.token.slice(0, ref.token.length - ref.id.length), id: ref.id, open,
    title: open ? `${KIND_WORD[ref.kind]} ${ref.org}/${ref.rest} · Open in Orgtree` : notHere(ctx.orgName(ref.org)) };
}

/** A bare `@name` in a message that has an org (null otherwise: plain text). */
export function nodeChip(name: string, ctx: OrefContext): OrefChip | null {
  const n = ctx.nodes;
  if (!n) return null;
  const open = !!n.slug && ctx.opens(n.slug);
  return { mark: "node", token: n.slug ? `@agent:${n.slug}/${name}` : null, org: n.slug, hidden: "@", id: name, open,
    title: open ? `${KIND_WORD.node} ${n.slug}/${name} · Open in Orgtree` : notHere(n.slug ? ctx.orgName(n.slug) : n.name) };
}

/** The chip as markup for md.ts. Sanitized after this like everything else;
 *  only a chip that opens is a keyboard stop. */
export function chipHtml(c: OrefChip): string {
  return '<a class="oref k-' + c.mark + (c.open ? " on" : "") + '"'
    + (c.token ? ' data-oref="' + esc(c.token) + '"' : "")
    + ' title="' + esc(c.title) + '"' + (c.open ? ' tabindex="0" role="link"' : "") + ">"
    + '<svg class="omark" viewBox="0 0 24 24" aria-hidden="true"><path d="' + OREF_MARKS[c.mark] + '"/></svg>'
    + '<span class="otok">' + esc(c.hidden) + "</span>" + esc(c.id) + "</a>";
}

/** What a chip's mention copies: its token as written, or (a bare name of an org not on this PC has no token) its text. */
export function chipMention(token: string | undefined, text: string): string {
  return token && parseOref(token) ? token : text.trim();
}

/** The kind word of a chip's left-click entry ("Open ticket in Orgtree"). */
export function chipOpenLabel(token: string | undefined): string {
  const ref = token ? parseOref(token) : null;
  return (ref ? KIND_WORD[ref.kind] : KIND_WORD.node).toLowerCase();
}

/** The right-click entry: "Open <org> in Orgtree", the org only (user
 *  2026-11-08 mb6a4bdb7c944), disabled with the hint when it cannot open.
 *  `token` is the chip's data-oref; a bare name of an org not on this PC has
 *  none, and its entry names the conversation's org. */
export function orgMenuEntry(token: string | undefined, ctx: OrefContext): { label: string; org: string | null; title: string } {
  const ref = token ? parseOref(token) : null;
  const name = ref ? ctx.orgName(ref.org) : ctx.nodes?.name || "this org";
  const org = ref && ctx.opens(ref.org) ? ref.org : null;
  return { label: `Open ${name} in Orgtree`, org, title: org ? "" : notHere(name) };
}

/** A click on a chip: the org and token to open, or null when it cannot open
 *  here. Decided again from the token, never from the chip's class. */
export function chipOpen(token: string | undefined, ctx: OrefContext): { org: string; token: string } | null {
  const ref = token ? parseOref(token) : null;
  return ref && ctx.opens(ref.org) ? { org: ref.org, token: ref.token } : null;
}
