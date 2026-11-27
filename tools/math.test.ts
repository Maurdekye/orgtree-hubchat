// LaTeX math in messages (docket hubchat-render-latex-math-in-messages-like-orgtr): the delimiter rules,
// which are Orgtree's (markdownmath.ts). Pure rules, no DOM; the rendering is proved mounted by
// tools/proof-math.mjs.
//   node --test tools/math.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { mathBlockStart, mathInlineStart, mathToken } from "../src/lib/math.ts";

const inline = (s: string) => mathToken(s, false);
const block = (s: string) => mathToken(s, true);

test("the four delimiters: $ and \\( inline, $$ and \\[ display", () => {
  assert.deepEqual(inline("$x^2$ is it"), { type: "mathInline", raw: "$x^2$", text: "x^2", display: false });
  assert.deepEqual(inline("\\(a+b\\) too"), { type: "mathInline", raw: "\\(a+b\\)", text: "a+b", display: false });
  assert.deepEqual(inline("$$\\int_0^1 x\\,dx$$ inline"), { type: "mathInline", raw: "$$\\int_0^1 x\\,dx$$", text: "\\int_0^1 x\\,dx", display: true });
  assert.deepEqual(inline("\\[x\\]"), { type: "mathInline", raw: "\\[x\\]", text: "x", display: true });
});

test("plain dollar amounts and shell text stay text", () => {
  for (const s of ["$5 and $10", "$5", "$1,000.50 total", "${HOME}/x$", "$(date)$", "$PATH$", "$ x $", "$x $", "$x$y", "$$", "$ $", "$", "$\n$"])
    assert.equal(inline(s), undefined, s);
  for (const s of ["It costs $5 and $10.", "US$5 or US$7", "echo $HOME and $PATH", "a $ b $ c", "price: $5, tax $1"])
    assert.equal(mathInlineStart(s), undefined, s);
  // \(...\) still typesets what $ leaves alone
  assert.equal(inline("\\(5\\)")?.text, "5");
  assert.equal(inline("\\(PATH\\)")?.text, "PATH");
});

test("inline math stays on one line; display math may span lines", () => {
  assert.equal(inline("$a\nb$"), undefined);
  assert.equal(inline("\\(a\nb\\)"), undefined);
  assert.equal(inline("$$a\nb$$")?.text, "a\nb");
});

test("an escaped dollar belongs to the formula", () => {
  assert.equal(inline("$a\\$b$ end")?.text, "a\\$b");
  assert.equal(inline("$a\\\\$ end")?.text, "a\\\\");
});

test("where inline math starts in prose", () => {
  assert.equal(mathInlineStart("so $x^2$ grows"), 3);
  assert.equal(mathInlineStart("costs $5, but $y$ is"), 14);
  assert.equal(mathInlineStart("see \\(x\\)"), 4);
  assert.equal(mathInlineStart("word$x$"), undefined);
});

test("a display block is alone on its lines", () => {
  assert.deepEqual(block("$$\nE = mc^2\n$$\nafter"), { type: "mathBlock", raw: "$$\nE = mc^2\n$$", text: "\nE = mc^2\n", display: true });
  assert.equal(block("\\[x\\]")?.text, "x");
  assert.equal(block("$$x$$ and more"), undefined);
  assert.equal(block("$x$"), undefined);
  assert.equal(block("\\(x\\)"), undefined);
  assert.equal(mathBlockStart("text\n$$x$$"), 5);
  assert.equal(mathBlockStart("no math"), undefined);
});
