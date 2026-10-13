import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { splitGuide } from "../src/lib/guide-sections";

const source = readFileSync(process.argv[2], "utf8");
const clean = source.replace(/\r\n?/g, "\n").replace(/^# .*\n+/, "")
  .replace(/<!-- screenshots -->[\s\S]*?<!-- \/screenshots -->\n*/g, "");
const actual = splitGuide(source);
assert.equal(actual.sections.length, 8, "every current setup section becomes a tab");
assert.equal(actual.sections[0].title, "Developer teams: shared standalone mailhub");
assert.equal(actual.sections[1].label, "With Orgtree and Tailscale");
assert.equal(actual.sections[7].label, "When it stops working");
assert.equal(actual.intro + actual.sections.map(section => section.markdown).join(""), clean,
  "splitting preserves every retained character in order");
assert.ok(!actual.sections.some(section => section.markdown.includes("<!-- screenshots -->")));

const fixture = "# Guide\r\n\r\nIntro.\r\n\r\n## 1. First\r\n\r\n```text\r\n## Not a section\r\n```\r\n\r\n### Details\r\n\r\nText.\r\n\r\n## 2. Second\r\n\r\nTail.\r\n";
const split = splitGuide(fixture);
assert.deepEqual(split.sections.map(section => section.label), ["First", "Second"]);
assert.ok(split.sections[0].markdown.includes("## Not a section"), "fenced heading text is retained");
assert.deepEqual(split.sections[0].anchors, ["1-first", "details"]);
assert.deepEqual(splitGuide("Intro without sections.").sections, []);
assert.equal(splitGuide("Intro without sections.").intro, "Intro without sections.");
assert.deepEqual(splitGuide("## Same\n\nOne\n\n## Same\n\nTwo").sections.map(s => s.anchors),
  [["same"], ["same-1"]]);
console.log("Guide splitting: 12 assertions passed; full document conserved, fences/anchors/duplicates covered.");
