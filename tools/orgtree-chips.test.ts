// Orgtree mentions as chips (docket hubchat-render-orgtree-mentions-as-chips-linking;
// contract: Orgtree's docs/rust-engine/orgtree-links.md). Pure rules, no DOM:
//   node --test tools/orgtree-chips.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import {
  chipHtml, chipOpen, nodeChip, nodeNameAtStart, notHere, NO_OREF_CONTEXT, orefAtStart, orefContext,
  orgMenuEntry, parseOref, scanOrefs, tokenChip,
} from "../src/lib/orgtreeRefs.ts";

const fixture = JSON.parse(fs.readFileSync(new URL("./orgtree-ref-tokens.json", import.meta.url), "utf8"));
const orgs = [
  { slug: "alpha", name: "Alpha Works", address: "alpha.someone.1a2b3c" },
  { slug: "gamma", name: "Gamma Lab", address: "gamma.someone.4d5e6f" },
];
const ctx = (over: Partial<Parameters<typeof orefContext>[0]> = {}) => orefContext({
  peer: "alpha.someone.1a2b3c", peerKind: "org", peerName: "Alpha Works", outgoing: false,
  desktop: true, links: true, orgs, ...over,
});

test("the grammar is Orgtree's: its own fixture, token by token and in prose", () => {
  let n = 0;
  for (const [token, want] of Object.entries(fixture.parse) as [string, null | Record<string, string>][]) {
    const got = parseOref(token);
    if (want === null) assert.equal(got, null, token);
    else {
      assert.ok(got, token);
      assert.equal(got.kind, want.kind, token);
      assert.equal(got.org, want.org, token);
      assert.equal(got.id, want.id, token);
    }
    n++;
  }
  for (const [text, want] of Object.entries(fixture.prose) as [string, [string, string][]][]) {
    assert.deepEqual(scanOrefs(text).map(({ ref }) => [ref.kind, `${ref.org}/${ref.rest}`]), want, text);
    n++;
  }
  assert.ok(n >= 40, `fixture rows: ${n}`);
});

test("apps, panels and projects too; the label is the target's own id", () => {
  for (const [token, kind, org, id] of [
    ["@app:alpha/caption-app", "app", "alpha", "caption-app"],
    ["@panel:alpha/p12", "panel", "alpha", "p12"],
    ["@project:alpha/web-team", "project", "alpha", "web-team"],
    ["@mail:alpha/node/luna@4/ab12", "mail", "alpha", "ab12"],
  ]) assert.deepEqual([parseOref(token)?.kind, parseOref(token)?.org, parseOref(token)?.id], [kind, org, id], token);
  assert.equal(orefAtStart("@item:alpha/one, then more")?.token, "@item:alpha/one");
  assert.equal(orefAtStart("@item:alpha/one/extra"), null);
  assert.equal(orefAtStart("x @item:alpha/one"), null);
});

test("a bare name is Orgtree's node alphabet, and never @user", () => {
  assert.equal(nodeNameAtStart("@luna-reserve, hi"), "luna-reserve");
  assert.equal(nodeNameAtStart("@luna."), "luna");
  assert.equal(nodeNameAtStart("@luna: please look"), "luna", "a colon after a name is punctuation");
  for (const s of ["@user", "@user,", "@Luna", "@luna_x", "@-luna", "@luna-", "@luna@4", "@ luna", "luna", "@net:pat.abcdef", "@item:Alpha/x", "@luna/x"])
    assert.equal(nodeNameAtStart(s), null, s);
});

test("which org a message has (Q1) and when its chips open (Q2)", () => {
  // an org of this PC, matched by address: its tokens of this PC's orgs open
  assert.equal(ctx().opens("alpha"), true);
  assert.equal(ctx().opens("gamma"), true, "another org of this PC, named by an org of this PC");
  assert.equal(ctx().opens("beta"), false, "not an org of this PC");
  assert.deepEqual(ctx().nodes, { slug: "alpha", name: "Alpha Works" });
  // my message TO that org: the same org
  assert.deepEqual(ctx({ outgoing: true }).nodes, { slug: "alpha", name: "Alpha Works" });
  // another person's org that happens to share a slug: identity is the address
  const foreign = ctx({ peer: "alpha.other.9z9z9z", peerName: "Their Alpha" });
  assert.equal(foreign.opens("alpha"), false);
  assert.deepEqual(foreign.nodes, { slug: null, name: "Their Alpha" });
  // between people: no org; theirs display-only, mine of this PC
  const theirs = ctx({ peer: "pat.abcdef", peerKind: "person", peerName: "Pat" });
  assert.equal(theirs.nodes, null);
  assert.equal(theirs.opens("alpha"), false);
  assert.equal(ctx({ peer: "pat.abcdef", peerKind: "person", peerName: "Pat", outgoing: true }).opens("alpha"), true);
  // desktop only, and only with an Orgtree that takes orgtree:// links
  assert.equal(ctx({ desktop: false }).opens("alpha"), false);
  assert.equal(ctx({ links: false }).opens("alpha"), false);
  assert.equal(NO_OREF_CONTEXT.opens("alpha"), false);
  assert.equal(ctx().orgName("alpha"), "Alpha Works");
  assert.equal(ctx().orgName("beta"), "beta");
});

test("chips: an opening one is a keyboard link, any other carries the hint", () => {
  const item = tokenChip(parseOref("@item:alpha/fix-login")!, ctx());
  assert.deepEqual(item, { mark: "item", token: "@item:alpha/fix-login", org: "alpha", hidden: "@item:alpha/", id: "fix-login", open: true,
    title: "Ticket alpha/fix-login · Open in Orgtree" });
  const html = chipHtml(item);
  assert.match(html, /^<a class="oref k-item on" data-oref="@item:alpha\/fix-login" title="Ticket alpha\/fix-login · Open in Orgtree" tabindex="0" role="link"><svg class="omark" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 15h7v2H7z[^"]*"\/><\/svg><span class="otok">@item:alpha\/<\/span>fix-login<\/a>$/);
  const mail = tokenChip(parseOref("@mail:alpha/user/m184ce37")!, ctx());
  assert.equal(mail.title, "Mail alpha/user/m184ce37 · Open in Orgtree");
  assert.equal(mail.id, "m184ce37");
  const doc = tokenChip(parseOref("@doc:beta/notes")!, ctx());
  assert.equal(doc.open, false);
  assert.equal(doc.title, "Opens in Orgtree on the PC that runs beta");
  assert.equal(chipHtml(doc), '<a class="oref k-doc" data-oref="@doc:beta/notes" title="Opens in Orgtree on the PC that runs beta">'
    + '<svg class="omark" viewBox="0 0 24 24" aria-hidden="true"><path d="M8 16h8v2H8zm0-4h8v2H8zm6-10H6c-1.1 0-2 .9-2 2v16c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8zm4 18H6V4h7v5h5z"/></svg>'
    + '<span class="otok">@doc:beta/</span>notes</a>');
  // one generic mark for agents and projects (user m0dc5cb44bd7f)
  assert.equal(tokenChip(parseOref("@project:alpha/web")!, ctx()).mark, "node");
  assert.equal(tokenChip(parseOref("@agent:alpha/luna")!, ctx()).mark, "node");
  assert.equal(tokenChip(parseOref("@project:alpha/web")!, ctx()).title, "Project alpha/web · Open in Orgtree");
  assert.equal(notHere("Alpha Works"), "Opens in Orgtree on the PC that runs Alpha Works");
  assert.match(chipHtml(tokenChip(parseOref("@item:alpha/a-b")!, ctx({ links: false }))), /^<a class="oref k-item" data-oref/);
});

test("a bare name is a node of the message's org, and plain text between people (Q1, Q3)", () => {
  const local = nodeChip("luna-reserve", ctx())!;
  assert.deepEqual(local, { mark: "node", token: "@agent:alpha/luna-reserve", org: "alpha", hidden: "@", id: "luna-reserve", open: true,
    title: "Agent or project alpha/luna-reserve · Open in Orgtree" });
  assert.match(chipHtml(local), /<span class="otok">@<\/span>luna-reserve<\/a>$/);
  const elsewhere = nodeChip("luna", ctx({ peer: "alpha.other.9z9z9z", peerName: "Their Alpha" }))!;
  assert.deepEqual([elsewhere.token, elsewhere.org, elsewhere.open, elsewhere.title],
    [null, null, false, "Opens in Orgtree on the PC that runs Their Alpha"]);
  assert.ok(!chipHtml(elsewhere).includes("data-oref"));
  assert.equal(nodeChip("luna", ctx({ peer: "pat.abcdef", peerKind: "person", peerName: "Pat" })), null);
  assert.equal(nodeChip("luna", NO_OREF_CONTEXT), null);
});

test("a click opens only what this PC can open, decided from the token", () => {
  assert.deepEqual(chipOpen("@item:alpha/fix-login", ctx()), { org: "alpha", token: "@item:alpha/fix-login" });
  assert.deepEqual(chipOpen("@agent:alpha/luna", ctx()), { org: "alpha", token: "@agent:alpha/luna" });
  for (const token of [undefined, "", "@item:beta/x", "@item:alpha/x/y", "orgtree://item/alpha/x", "@item:Alpha/x"])
    assert.equal(chipOpen(token, ctx()), null, String(token));
  assert.equal(chipOpen("@item:alpha/x", ctx({ desktop: false })), null);
});

test("the menu entry opens just the org, or says why it cannot (user mb6a4bdb7c944)", () => {
  assert.deepEqual(orgMenuEntry("@item:alpha/fix-login", ctx()), { label: "Open Alpha Works in Orgtree", org: "alpha", title: "" });
  assert.deepEqual(orgMenuEntry("@item:beta/x", ctx()),
    { label: "Open beta in Orgtree", org: null, title: "Opens in Orgtree on the PC that runs beta" });
  assert.deepEqual(orgMenuEntry(undefined, ctx({ peer: "alpha.other.9z9z9z", peerName: "Their Alpha" })),
    { label: "Open Their Alpha in Orgtree", org: null, title: "Opens in Orgtree on the PC that runs Their Alpha" });
  assert.equal(orgMenuEntry("@item:alpha/x", ctx({ links: false })).org, null);
});
