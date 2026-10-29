# Symmetry S4: Hubchat menus, rows, settings and composer

USER 2026-10-29 12:50-13:25Z (via the Orgtree coordinator): the UX symmetry audit's findings are fixed, "symmetry wins unless my
ruling overrides it, but bring it up with me every time i make a ruling that conflicts with symmetry". This page is the S4
batch of twelve findings (audit `ux02` S4, `ux05` S3/S4/S5/S7, `ux07` S4/S5, `ux08` S3/S4/S5, `ux10` S3/S4). No ruling was
contradicted; nothing was parked.

## What changed

| Audit | Behaviour | Code |
| --- | --- | --- |
| #10 ux02 S4 | Right-clicking an address in a message offers Open chat, **Contact info** and Copy address: the one contact block (`contactEntries`) the chat list, Directory and New chat share. Desktop passes `contactInfo` (that chat, panel open) down through `Conversation`. | `ChatList.tsx`, `MessageView.tsx`, `Conversation.tsx`, `Desktop.tsx` |
| #12/#13 ux05 S3/S4 | A contact in New chat (Recent and matching rows) is the Directory's `ContactRow`: about line, one chip per hub (a hub that is not connected is dashed), presence, and on desktop the chat menu (Message, Contact info, Copy address). | `Directory.tsx` (`ContactRow`), `NewChat.tsx`, `Desktop.tsx` |
| #14 ux05 S5 | A finished transfer says `Sent to <name>` / `Received from <name>` like an unfinished one; the name is a link-style button that opens that chat. View and Redownload stay independent. | `Transfers.tsx` |
| #15 ux05 S7 (Hubchat side) | The next history page loads when the end of the list comes within 240 px (an `IntersectionObserver` on `.xhistory-end`), one page at a time; `Load more` stays as the manual path; nothing loads by itself while an error stands. Orgtree's transfer history does the same (S2, @app-net-opus). | `Transfers.tsx` |
| #45 ux07 S5 | Retry repeats only the failed request: a later page's cursor, or the first read; rows already loaded stay. | `Transfers.tsx` |
| #28 ux07 S4 | Contact info Files shows `Loading files…`, the read error with a Retry (files already read stay), and `No files yet` only after a read that answered. `useMessages` now reports `error`. | `ContactInfo.tsx`, `store.ts` |
| #32 ux08 S4 | The desktop Settings section list is a tab list (`role=tablist`/`tab`/`tabpanel`, the open section `aria-selected`, one tab stop, Up/Down/Left/Right wrap, Home, End; modified keys ignored), as Orgtree's `SettingsTabs`. | `Settings.tsx` |
| #31 ux08 S3 | An unsaved Profile name or About survives a switch to another section and back; Save or closing Settings ends it. The modal holds the draft. | `Settings.tsx` |
| #46 ux08 S5 | A boolean setting row (`ToggleRow`) toggles from its label and hint (a `<label>` for the switch) and says `on` / `off`, like Orgtree's toggle rows. The hub enable switch keeps its own row with its Retry button. | `Settings.tsx`, `ui.tsx` (`Switch id`), `base.css` |
| #38 ux10 S3 | Desktop: Settings > General > **Enter key** = `Send message` (default: Enter sends, Shift+Enter new line) or `Insert new line` (Enter new line, Ctrl/Command+Enter sends). The hint under the box and the Send button's title follow it; an IME composition's Enter never sends. Stored as localStorage `hubchat.enter-key`. Android keeps its Send button. | `lib/enterKey.ts`, `Composer.tsx`, `Settings.tsx` |
| #39 ux10 S4 | A pasted file of any kind joins the attachments under its own name (an image keeps its time-stamped `pasted-…` name; text on the clipboard still wins; over 64 MiB is refused with a pointer to the picker or drag-and-drop, since a pasted file goes through the page into the app as bytes). The native `hc_save_pasted` keeps the file's own name: last path component, Windows-refused characters replaced, device names made ordinary, at most 150 characters. | `Composer.tsx`, `lib/images.ts`, `src-tauri/src/media.rs` |

## Decisions made, to flag

- **Order, labels:** Open chat / Message, Contact info, Copy address in every contact menu; `on` / `off` lowercase as Orgtree prints them; `Load more`, `Retry`, `Loading…` as Orgtree's gallery and Hubchat's transfer history already say.
- **Paste size:** 64 MiB is this batch's choice (`MAX_PASTED_BYTES`); the picker and drop hand over a path and have no such limit.
- **Hub enable switch:** not converted (its row has other actions: Retry now).
- **Profile edits on Android:** unchanged. A phone leaves Profile by going back to the list, there is no section switch.

## Verification

Mounted real components in jsdom with a synthetic API (no hub, native file or clipboard), run under the repository Job limiter
(`tools/symmetry-s4-harness.mjs`; each proof's red run fails exactly the checks its change adds):

| Proof | Checks |
| --- | --- |
| `tools/proof-symmetry-s4-transfers.mjs` | #14 #15 #45, desktop and Android |
| `tools/proof-symmetry-s4-contact-files.mjs` | #28 |
| `tools/proof-symmetry-s4-newchat.mjs` | #12 #13 |
| `tools/proof-symmetry-s4-mention.mjs` | #10 |
| `tools/proof-symmetry-s4-settings.mjs` | #31 #32 #46 |
| `tools/proof-symmetry-s4-composer.mjs` | #38 #39 |
| `cargo test -p hubchat --lib media` | `pasted_file_name` |
| `tools/proof-symmetry-s4-capture.mjs` | offscreen Electron pictures of every state, desktop and Android mock, silent, Session 0 |

Not established: a real Windows clipboard paste of copied files (the page gets `File` objects only in a real webview), a real
screen reader on the tab list, and native key delivery for the Enter-key choice; the user's checks.
