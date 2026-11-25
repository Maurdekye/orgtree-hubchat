# Changelog in Settings

Settings › Changelog shows every release's notes, newest first, with the running version marked **This version**
(user 2026-11-25: "dont pop it up on every restart, make it just an extra tab in settings that you can optionally
navigate to and view").

- **Only opened by the user.** It is one more Settings tab, between Help and About on desktop and Hubchat Web, and a
  row in the same place in Android's settings list. Nothing opens it on start, restart or update.
  `tools/changelog.test.ts` fails if any other source file names it.
- **The desktop section list scrolls.** With 13 sections, a short window clipped the end of the list: About,
  and even before this tab, Quit Hubchat. `.set-nav` now scrolls on its own.
- **Source.** The text is `docs/release-notes-<v>.md`, the GitHub release text. Each file is bundled through a
  `?raw` import in `src/lib/releaseNotes.ts`, so the tab works offline. The release adds its import in the same
  commit as the notes. `.github/ci/release/check-version.mjs` refuses a release tag whose notes aren't imported, and
  the unit test checks that the list matches `docs/`.
- **What is shown.** The sections about getting the files (`## Downloads`, `## First run on macOS and Linux`) are
  left out. Every other heading moves three levels down, to sit under the version's own heading. Code blocks are left
  as they are.
- **Running version.** When the running version has notes, its entry is marked. A test build has no notes of its own
  (alpha builds are never named in the notes). For one, the tab says "You're running 1.4.0-alpha.1, a test build. Its
  notes come with the 1.4.0 release." Once that release's notes are bundled, a later test build of it marks them
  instead. Until the version is known, as in a browser mock, nothing is marked.
- **Rendering.** The messages' Markdown renderer `md()` renders the notes, so the result passes through the same
  DOMPurify allow-list and raw HTML shows as text. Links open in the system browser, as in Help. Each version is
  rendered once, the first time the tab opens.
- **Proof.** `tools/proof-changelog.mjs` mounts Settings on desktop and Android in Electron. It checks the tab's place,
  that it's not open at start, that every section can be reached, the entry order and the mark, and that the notes have no download table or script
  element. It captures both layouts, plus the desktop list scrolled to its end.
