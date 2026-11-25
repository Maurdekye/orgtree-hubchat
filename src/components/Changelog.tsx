// Settings › Changelog: every release's notes, newest first, the running
// version marked. Only ever opened from Settings (user 2026-11-25
// m9547cf39e33c: no popup on start, restart or update). Bundled text, no
// network; the messages' sanitizing renderer, whose links go to the system
// browser as in Help.
import { useMemo, type KeyboardEvent as ReactKeyEvent, type MouseEvent } from "react";
import { changelog, versionOf, type NoteFile } from "../lib/changelog";
import { md } from "../lib/md";
import { openLink } from "../lib/native";
import { RELEASE_NOTES } from "../lib/releaseNotes";
import "../styles/setup.css";

const FILES: NoteFile[] = RELEASE_NOTES.map((n) => ({ version: versionOf(n.file)!, text: n.text }));
let rendered: Map<string, string> | null = null;
/** Each version's HTML, rendered once, the first time the tab opens. */
function html(version: string, markdown: string): string {
  rendered ??= new Map();
  let h = rendered.get(version);
  if (h === undefined) { h = md(markdown); rendered.set(version, h); }
  return h;
}

export function Changelog({ running }: { running: string }) {
  const { entries, pending } = useMemo(() => changelog(FILES, running), [running]);
  const list = entries.map((e) => ({ ...e, html: html(e.version, e.markdown) }));
  const click = (e: MouseEvent) => {
    const a = (e.target as HTMLElement).closest("[data-href]");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("data-href") || "";
    if (/^https?:/i.test(href)) void openLink(href);
  };
  const keys = (e: ReactKeyEvent) => {
    if (e.key === "Enter" && (e.target as HTMLElement).closest("[data-href]")) {
      e.preventDefault(); (e.target as HTMLElement).closest<HTMLElement>("[data-href]")?.click();
    }
  };
  return (
    <div className="guide changelog" onClick={click} onKeyDown={keys}>
      {pending ? <p className="changelog-pending">{pending.test
        ? `You're running ${pending.running}, a test build. Its notes come with the ${pending.release} release.`
        : `You're running ${pending.running}. Its notes aren't written yet.`}</p> : null}
      {list.map((e) => (
        <section key={e.version} className="changelog-entry" aria-labelledby={"changelog-" + e.version}>
          <h4 className="changelog-v" id={"changelog-" + e.version}>{e.version}{e.current ? <span className="chip acc">This version</span> : null}</h4>
          <div dangerouslySetInnerHTML={{ __html: e.html }} />
        </section>
      ))}
    </div>
  );
}
