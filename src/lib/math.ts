// LaTeX math in messages (user 2026-11-27 m13da5b728a34 "does hubchat not support latex? it should").
// The delimiter rules are Orgtree's, copied as they are (Orgtree's
// apps/desktop/renderer/src/canvas/markdownmath.ts at 4.7.1), so a message reads the same in both apps:
//   inline  $...$  \(...\)      display  $$...$$  \[...\]
// Pure rules, no DOM and no KaTeX here: md.ts renders what this finds (tools/math.test.ts).
// It runs inside marked's lexer, so code spans and fenced code keep their precedence,
// and an unfinished delimiter stays ordinary text.

export interface MathToken { type: "mathInline" | "mathBlock"; raw: string; text: string; display: boolean }

/** The math at the very start of `src`, or undefined. `block`: a display formula alone on its line(s). */
export function mathToken(src: string, block: boolean): MathToken | undefined {
  const open = block ? (src.startsWith("$$") ? "$$" : src.startsWith("\\[") ? "\\[" : "")
    : src.startsWith("$$") ? "$$" : src.startsWith("\\[") ? "\\["
      : src.startsWith("\\(") ? "\\(" : src.startsWith("$") ? "$" : "";
  if (!open) return undefined;
  const display = open === "$$" || open === "\\[";
  const close = open === "\\[" ? "\\]" : open === "\\(" ? "\\)" : open;
  if (open === "$" && (!src[1] || /[\s$]/.test(src[1]))) return undefined;
  if (open === "$" && /[({]/.test(src[1]!)) return undefined; // shell ${VAR} and $(command)
  let end = open.length;
  for (; end < src.length; end++) {
    if (!display && /[\r\n]/.test(src[end]!)) return undefined;
    if (!src.startsWith(close, end)) continue;
    // A dollar escaped with an odd number of backslashes is part of the LaTeX.
    if (close[0] === "$") {
      let slashes = 0;
      for (let i = end - 1; i >= 0 && src[i] === "\\"; i--) slashes++;
      if (slashes % 2) continue;
    }
    break;
  }
  if (end >= src.length) return undefined;
  const text = src.slice(open.length, end);
  if (!text.trim()) return undefined;
  if (open === "$") {
    if (/\s/.test(text[text.length - 1]!) || /[\w$]/.test(src[end + 1] ?? "")) return undefined;
    // Plain amounts ("$5 and $10") and shell names ($PATH) stay text, on purpose.
    // \(...\) still typesets a number or an uppercase name.
    if (/^\d[\d.,]*(?:\s|$)/.test(text) || /^[A-Z_][A-Z0-9_]*$/.test(text)) return undefined;
  }
  const raw = src.slice(0, end + close.length);
  if (block && /^[^\S\n]*\S/.test(src.slice(raw.length))) return undefined;
  return { type: block ? "mathBlock" : "mathInline", raw, text, display };
}

/** Where marked should stop plain text for inline math: never inside a word or after a backslash (US$5). */
export function mathInlineStart(src: string): number | undefined {
  for (const m of src.matchAll(/\$|\\[([]/g)) {
    const i = m.index ?? 0;
    if (i > 0 && /[\w\\]/.test(src[i - 1]!)) continue;
    if (mathToken(src.slice(i), false)) return i;
  }
  return undefined;
}

/** Where a display formula on its own line starts. */
export function mathBlockStart(src: string): number | undefined {
  const m = /(?:^|\n)(?=\$\$|\\\[)/.exec(src);
  return m ? m.index + m[0].length : undefined;
}
