// The changelog in Settings (user 2026-11-25 m9547cf39e33c: never shown by
// itself, "just an extra tab in settings"). Its text is the repo's release
// notes (docs/release-notes-<v>.md, bundled by releaseNotes.ts): newest
// first, the running version marked. No DOM and no imports, so the node tests
// read it as it is.

export type NoteFile = { version: string; text: string };
export type Entry = { version: string; markdown: string; current: boolean };
/** A test build whose release has no notes yet: what the tab says instead of marking an entry. */
export type Pending = { running: string; release: string; test: boolean };

/** Sections of the GitHub release text that are about getting the files, not about what changed. */
export const DROPPED_SECTIONS = ["Downloads", "First run on macOS and Linux"];

/** "1.3.13" from a path ending in release-notes-1.3.13.md, else null. */
export function versionOf(file: string): string | null {
  const m = /release-notes-(\d+\.\d+\.\d+)\.md$/.exec(file);
  return m ? m[1]! : null;
}

/** The release a version belongs to: "1.4.0" for "1.4.0-alpha.1". */
export const releaseOf = (v: string) => v.split(/[-+]/)[0]!;

/** Compares two x.y.z versions as numbers. */
export function cmpVersion(a: string, b: string): number {
  const pa = releaseOf(a).split(".").map(Number), pb = releaseOf(b).split(".").map(Number);
  for (let i = 0; i < 3; i++) { const d = (pa[i] ?? 0) - (pb[i] ?? 0); if (d) return d; }
  return 0;
}

/** The notes as the app shows them: without the download sections, each heading three levels
 *  down so it sits under the version's own heading (h4). Code blocks are left as they are. */
export function forApp(text: string): string {
  const out: string[] = [];
  let fence = false, dropping = false;
  for (const line of text.replace(/\r\n?/g, "\n").split("\n")) {
    if (!fence) {
      const h2 = /^## +(.*?)\s*$/.exec(line);
      if (h2) dropping = DROPPED_SECTIONS.includes(h2[1]!);
      else if (/^# /.test(line)) dropping = false;
    }
    if (/^ {0,3}(```|~~~)/.test(line)) fence = !fence;
    if (dropping) continue;
    out.push(!fence && /^#{1,6} /.test(line) ? "#".repeat(Math.min(6, line.indexOf(" ") + 3)) + line.slice(line.indexOf(" ")) : line);
  }
  return out.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/** Newest first; the running version's release marked, or what to say when it has no notes yet. */
export function changelog(files: NoteFile[], running: string | null | undefined): { entries: Entry[]; pending: Pending | null } {
  const release = running ? releaseOf(running) : null;
  const entries = [...files].sort((a, b) => cmpVersion(b.version, a.version))
    .map((f) => ({ version: f.version, markdown: forApp(f.text), current: f.version === release }));
  const pending = running && !entries.some((e) => e.current) ? { running, release: release!, test: running !== release } : null;
  return { entries, pending };
}
