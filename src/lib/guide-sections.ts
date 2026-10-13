import { Lexer } from "marked";

export interface GuideSection {
  title: string;
  label: string;
  markdown: string;
  anchors: string[];
}

/** Keep the bundled/GitHub document as the single source. Lexer headings
 * distinguish real sections from heading-like text inside fenced code. */
export function splitGuide(source: string): { intro: string; sections: GuideSection[] } {
  const body = source.replace(/\r\n?/g, "\n").replace(/^# .*\n+/, "")
    .replace(/<!-- screenshots -->[\s\S]*?<!-- \/screenshots -->\n*/g, "");
  let intro = "";
  const sections: GuideSection[] = [];
  const counts = new Map<string, number>();
  for (const token of Lexer.lex(body)) {
    if (token.type === "heading" && token.depth === 2) {
      sections.push({ title: token.text, label: token.text.replace(/^\d+\.\s*/, ""), markdown: "", anchors: [] });
    }
    const section = sections[sections.length - 1];
    if (section) {
      section.markdown += token.raw;
      if (token.type === "heading") {
        const slug = token.text.toLowerCase().replace(/[^\p{L}\p{N}_\- ]/gu, "").replace(/ /g, "-");
        const count = counts.get(slug) ?? 0;
        counts.set(slug, count + 1);
        section.anchors.push(slug + (count ? `-${count}` : ""));
      }
    } else intro += token.raw;
  }
  return { intro, sections };
}
