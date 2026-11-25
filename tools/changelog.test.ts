// The changelog as a Settings tab (docket hubchat-changelog-as-an-optional-settings-tab-hu; user 2026-11-25
// m9547cf39e33c: "dont pop it up on every restart, make it just an extra tab in settings that you can optionally
// navigate to and view"). The release notes are bundled at build time; nothing opens the tab by itself.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { changelog, cmpVersion, forApp, versionOf, DROPPED_SECTIONS, type NoteFile } from "../src/lib/changelog.ts";

const root = path.resolve(import.meta.dirname, "..");
const src = (p: string) => fs.readFileSync(path.join(root, p), "utf8");
const docs = fs.readdirSync(path.join(root, "docs")).filter((f) => /^release-notes-\d+\.\d+\.\d+\.md$/.test(f));
const files: NoteFile[] = docs.map((f) => ({ version: versionOf(f)!, text: src("docs/" + f) }));

test("versions compare as numbers, not text", () => {
  assert.equal(versionOf("../../docs/release-notes-1.3.13.md"), "1.3.13");
  assert.equal(versionOf("docs/update-notes-1.3.13.txt"), null);
  assert.ok(cmpVersion("1.3.13", "1.3.9") > 0);
  assert.ok(cmpVersion("1.10.0", "1.9.9") > 0);
  assert.equal(cmpVersion("1.4.0", "1.4.0"), 0);
});

test("every release-notes file is bundled, and nothing else", () => {
  const list = src("src/lib/releaseNotes.ts");
  const bundled = [...list.matchAll(/docs\/(release-notes-[^"'?]+\.md)\?raw/g)].map((m) => m[1]).sort();
  assert.deepEqual(bundled, [...docs].sort(), "src/lib/releaseNotes.ts imports each docs/release-notes-<v>.md");
  assert.ok(docs.length >= 30, "an empty scan is not a pass");
  assert.match(src(".github/ci/release/check-version.mjs"), /src\/lib\/releaseNotes\.ts/, "a release refuses notes the app does not bundle");
});

test("newest first, every version once", () => {
  const { entries } = changelog(files, "1.3.13");
  const want = [...files].map((f) => f.version).sort((a, b) => cmpVersion(b, a));
  assert.deepEqual(entries.map((e) => e.version), want);
  assert.equal(entries[0]!.version, want[0]);
  assert.ok(entries.findIndex((e) => e.version === "1.3.10") < entries.findIndex((e) => e.version === "1.3.9"));
});

test("the running version is marked; a test build of an unreleased version says its notes come later", () => {
  const rel = changelog(files, "1.3.13");
  assert.deepEqual(rel.entries.filter((e) => e.current).map((e) => e.version), ["1.3.13"]);
  assert.equal(rel.pending, null);
  const alpha = changelog(files, "9.9.0-alpha.1");
  assert.deepEqual(alpha.entries.filter((e) => e.current), []);
  assert.deepEqual(alpha.pending, { running: "9.9.0-alpha.1", release: "9.9.0", test: true });
  const withNotes = changelog([...files, { version: "9.9.0", text: "Hubchat 9.9.0 does things." }], "9.9.0-alpha.2");
  assert.deepEqual(withNotes.entries.filter((e) => e.current).map((e) => e.version), ["9.9.0"], "the notes of the version being built mark it");
  assert.equal(withNotes.pending, null);
  assert.equal(changelog(files, null).pending, null, "no version known (a browser mock): nothing said");
});

test("the app shows what changed, not how to download", () => {
  assert.deepEqual(DROPPED_SECTIONS, ["Downloads", "First run on macOS and Linux"]);
  const text = "Summary line.\n\n## Downloads\n\n| File | For |\n|---|---|\n| a | b |\n\n## Long chats\n\n- **Fast.** Yes.\n\n### Detail\n\nMore.\n\n## First run on macOS and Linux\n\nRight-click.\n\n## Notes\n\n```\n## not a heading\n```\n";
  const out = forApp(text);
  assert.doesNotMatch(out, /Downloads|\| File|Right-click|First run/);
  assert.match(out, /^Summary line\./);
  assert.match(out, /^##### Long chats$/m, "a section heading sits under the version's own heading");
  assert.match(out, /^###### Detail$/m);
  assert.match(out, /^##### Notes$/m);
  assert.match(out, /^## not a heading$/m, "a code block is left as it is");
  for (const f of files) assert.doesNotMatch(forApp(f.text), /^#{1,6} Downloads$/m, f.version);
});

test("a tab in Settings on every layout, opened only by the user", () => {
  const settings = src("src/components/Settings.tsx");
  const tabs = settings.match(/export const TABS[^\n]*/)![0];
  assert.match(tabs, /\["help", "Help", "help"\], \["changelog", "Changelog", "history"\], \["about", "About", "info"\]/, "between Help and About");
  assert.match(settings, /case "changelog": return <ChangelogTab \/>;/);
  assert.match(settings, /function ChangelogTab\(\) \{\s+return <Changelog running=\{useVersion\(\)\} \/>;/, "the running version, as About reads it");
  assert.match(settings, /<Row icon="history" t1="Changelog"[^\n]*onClick=\{\(\) => onOpen\("changelog"\)\}/, "Android's settings list");
  // nothing else names the tab: no start, restart or update path can open it
  const named: string[] = [];
  const walk = (d: string) => { for (const e of fs.readdirSync(path.join(root, d), { withFileTypes: true })) {
    const p = d + "/" + e.name;
    if (e.isDirectory()) walk(p); else if (/\.(tsx?|css)$/.test(e.name) && /changelog/i.test(src(p))) named.push(p);
  } };
  walk("src");
  assert.deepEqual(named.sort(), ["src/components/Changelog.tsx", "src/components/Settings.tsx", "src/lib/changelog.ts", "src/lib/releaseNotes.ts", "src/styles/setup.css"]);
  const view = src("src/components/Changelog.tsx");
  assert.match(view, /import \{ md \} from "\.\.\/lib\/md";/, "rendered by the messages' sanitizing renderer");
  assert.equal(view.match(/dangerouslySetInnerHTML/g)?.length, 1);
  assert.match(view, /dangerouslySetInnerHTML=\{\{ __html: e\.html \}\}/);
  assert.doesNotMatch(view, /fetch\(|https?:\/\//, "offline: no network");
});
