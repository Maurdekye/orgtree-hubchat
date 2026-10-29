# Symmetry fixes S3: labels, hovers and keyboard access (2026-10-29)

Docket `symmetry-fixes-s3-hubchat-labels-hovers-and-keyb`. The UX symmetry audit
(2026-10-29, after the user's principle of a highly symmetrical UX, USER
mb6da9f3ee862) found Hubchat surfaces that lacked a hover, a spoken label or
keyboard access that their sibling surfaces have. The user authorized fixing all
50 findings and ruled (m72500eeca0c0): symmetry wins unless an explicit ruling of
theirs overrides it, and every conflicting ruling goes to them to decide. This
batch is the small single-surface Hubchat items; none of them met a signed ruling
(grepped in `AGENTS.md` and `docs/` for hover, unread, theme, quote and time).

| Audit | What was different | Now |
| --- | --- | --- |
| #5 | Orgtree's offline contact is a red ring, Hubchat's a grey one | Check only, no change: offline is the grey `--faint` ring on the avatar dot, presence text, hub picker and hub dots (six rules in `base.css`, `desktop.css`, `mobile.css`); the slashed unknown state stays separate |
| #8 | An active transfer row opened its chat by click only | Focusable, Enter and Space open the chat (`pressKeys`); Cancel stays its own control |
| #16 | The expanded chat row's long name had no hover; the rail row had | `title` = the full name |
| #17 | A non-image composer chip had no hover; the image thumbnail had | Both titles come from `fileTitle(name, bytes)` = `name · size` |
| #18 | Directory addresses and about lines were cut with no hover | `title` = the full value |
| #22 | The expanded row's unread badge read as a bare number; the rail row says "N unread" | The badge is a labelled image (`title`, `aria-label` "N unread", the number stays visible); one `unreadLabel()` serves both rows |
| #29 | Android theme choices were clickable divs with `role=radio` and no keyboard | Native radio inputs (visually hidden, `.sr`) inside labels, one `radiogroup`; the focused row is outlined |
| #30 | The Android Settings profile card had no keyboard access | `tabIndex` 0 and `pressKeys`; the Copy address button stays separate |
| #36 | A quote jumped to its original by pointer only | `role=button`, `tabIndex` 0, Enter and Space jump; "Original message is not on this device" stays plain text |
| #42 | The conversation header picture had no hint; the name and info button say Contact info | `title` "Contact info" (the presence dot keeps its own status title) |
| #44 | The collapsed hub button was announced as "Hub connections" only | `aria-label` = the hub summary, then "Hub connections" (its hover already said so, with F4) |
| #47 | The system theme option was "Match system" on desktop, "Follow system" on Android | "Match system" everywhere, including the Android Settings row |
| #50 | A message's clock had no full date and time | A `<time>` element; its `title` and `aria-label` are the full local date, time and zone (`fullWhen`: `2026-10-28 13:25:12 GMT+3`, the shape of Orgtree's mail hover); Message info keeps its separate clocks |

## Verification

- `tools/proof-symmetry-s3.mjs` (through `tools/verify-symmetry-s3.ps1`, kernel
  Job, a new output folder under `C:/orgtree-build`): mounts the real `App` on
  the synthetic API (`tools/symmetry-s3-entry.tsx`) on desktop and Android in a
  DOM with no window. 66 checks: at the base exactly the 36 `#<audit number>`
  checks fail and the 30 controls pass; at the fixes all 66 pass. Any session.
- `tools/proof-symmetry-s3-capture.mjs` (same script with `-Script`): the same
  checks in real Chromium, in an offscreen Electron window, **Session 0 only**
  (it asserts the session; muted, permissions refused, http(s) blocked), and a
  screenshot of each changed state. Before each focus screenshot a trusted Escape
  key goes through the window's own DevTools connection so a script `focus()` is
  drawn as keyboard focus; the proof fails if any focused element is not
  `:focus-visible`. A script `click()` on a radio does not move the focus, so the
  fixture moves it first, as the arrow keys do, and checks that the chosen radio holds it.
- `tsc --noEmit` over `src` (dependencies read in place, no copy or link).
