// Orgtree's mentions, drawn as chips (user 2026-11-08, docket
// hubchat-render-orgtree-mentions-as-chips-linking).
// RED STUB: the shapes exist so the tests can name them; nothing is drawn yet.
export type OrefKind = "item" | "doc" | "app" | "panel" | "agent" | "project" | "mail";
export interface Oref { kind: OrefKind; org: string; rest: string; id: string; token: string }
export interface LocalOrg { slug: string; name: string; address: string }
export interface OrefContext { opens: (org: string) => boolean; nodes: { slug: string | null; name: string } | null; orgName: (org: string) => string }
export interface OrefChip { mark: "item" | "doc" | "app" | "panel" | "mail" | "node"; token: string | null; org: string | null; hidden: string; id: string; open: boolean; title: string }
export const NO_OREF_CONTEXT: OrefContext = { opens: () => false, nodes: null, orgName: (slug) => slug };
export function parseOref(_token: string): Oref | null { return null; }
export function orefAtStart(_src: string): Oref | null { return null; }
export function scanOrefs(_text: string): { index: number; ref: Oref }[] { return []; }
export function nextOref(_src: string): number | undefined { return undefined; }
export function nodeNameAtStart(_src: string): string | null { return null; }
export function orefContext(_o: { peer: string; peerKind: string; peerName: string; outgoing: boolean; desktop: boolean; links: boolean; orgs: readonly LocalOrg[] }): OrefContext { return NO_OREF_CONTEXT; }
export const notHere = (_org: string) => "";
export function tokenChip(_ref: Oref, _ctx: OrefContext): OrefChip { return { mark: "item", token: null, org: null, hidden: "", id: "", open: false, title: "" }; }
export function nodeChip(_name: string, _ctx: OrefContext): OrefChip | null { return null; }
export function chipHtml(_c: OrefChip): string { return ""; }
export function orgMenuEntry(_token: string | undefined, _ctx: OrefContext): { label: string; org: string | null; title: string } { return { label: "", org: null, title: "" }; }
export function chipOpen(_token: string | undefined, _ctx: OrefContext): { org: string; token: string } | null { return null; }
