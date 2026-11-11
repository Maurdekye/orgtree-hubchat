# Taskbar attention (Windows, Hubchat 1.2.3)

The taskbar icon carries a red dot with a light rim while this identity has
unread messages or unanswered incoming questions. This includes unread chats
hidden with Close DM and history-only unread reported by a hub. Reading clears
the unread reason; a read question needs an explicit answer/skip or any later
outgoing message in the same chat. USER 2026-10-19 chose "Later reply settles it":
an ordinary send deals with every earlier question for attention, including
existing history. Structured card answers and picked options stay unchanged.
Outgoing questions await the other person and do not light this device's dot.
The tray icon retains its existing unread-only meaning.

`Store::taskbar_attention` reads only three active-reason indexes, without
loading bodies, attachments, chat pages or already answered question history.
Migration19 creates the metadata cache. Migration20 backfills the later-message
rule on upgrade, so existing settled conversations clear without a live-data
rewrite outside the normal migration. SQLite triggers
maintain question settlement in the existing message/reply-link transactions:
an explicit opposite-direction reply in the same chat settles it, including queued
answers, empty skips and secondary multi-replies. Any later outgoing message also
settles earlier incoming questions, including queued/attachment-only messages and
other-device sync. "Later" uses the chat's stored created_at order; equal times
alone do not establish later. Sync retiming refreshes this projection. Deleting
a settling message reopens attention only if no retained explicit or later
outgoing message settles it. Deleting a question or
wiping the identity removes its attention. Received kind/direction/reply target
are immutable after admission; later multi-reply expansion uses reply_links,
whose insert/delete triggers keep the cache consistent.

The native tray publisher refreshes on Incoming/Chat/Directory/Hub changes,
coalesces bursts, binds publication to the producing identity and deduplicates
the taskbar value separately from its tray icon. Startup sync and identity
adoption populate it while the renderer is hidden; sign-out clears it. A late
reset cannot clear a new owner's overlay. Showing a tray-hidden main window
invalidates the taskbar cache so the new button gets the current overlay.
Read errors retain a known same-identity value and retry on the next event;
they do not manufacture an all-clear. Failed OS updates stay uncached for retry.

The pinned Tauri2.12.1 WebviewWindow::set_overlay_icon API sets/removes the
overlay on `main`. It leaves the executable resource, native WM_SETICON masks,
Jump List and ordinary notification settings unchanged. The red/rim colours
match the existing tray dot. This is passive display, with no new user action
requiring an agent tool. Other desktop platforms and Android launcher badges
are outside this Windows implementation.

Focused author verification covers global unread/read/clear, closed and
history-only reasons, read incoming vs outgoing questions, same-chat direction,
skip and queued-answer parity, multi-reply/other-device settlement, deletion,
identity wipe, version18 backfill and use of active indexes. Older migration
fixtures now construct their real version14/15 schemas rather than dropping
selected tables from the newest schema. They preserve their history/file
assertions while avoiding later migration residue.

The standalone native artwork test exports the exact 16x16 RGBA sent to Tauri.
`tools/proof-taskbar-dot.mjs` composes those pixels with the real Hubchat icon
in one hidden Chromium preview and captures clear/unread/pending/cleared cases.
It requires assigned output paths and the existing kernel Job runner. This is
a synthetic native-pixel preview, not an Explorer/taskbar or installed-app
screenshot. Live shell placement, Explorer restart recovery, device behavior
and final packaged acceptance belong to the publishing review/build; no app
build/install/live send is part of the author phase.

## Showing why the dot is on (1.3.7, USER 2026-11-11 18:09Z "A: show where")

The rule above is unchanged. A pending incoming question lit the dot with no
other sign in the app, so the dot now explains itself:

- `Store::taskbar_counts()` returns (unread incl. history-only unread, pending
  questions) from the same tables as `taskbar_attention`; no second projection.
  `ChatSummary.needs_answer` / `first_question` are the per-chat pending count
  and the oldest pending card, read from `taskbar_questions` for open chats.
- The chat list row (and the collapsed rail button) of such a chat has a bold
  name and a "?" badge ("N question(s) need your answer"). Clicking the row
  opens the chat, scrolls to the oldest pending card and highlights it, through
  the existing `showMessage` jump (desktop and Android).
- The taskbar button's hover text (the window title) reads
  "Hubchat - N question(s) await your answer - M unread", or "Hubchat" when
  neither applies (`taskbar::hint`, set with the overlay from `tray_recent`).

Not in this change: a menu listing of pending questions (a follow-up job).
Verification: `node --test tools/needs-answer.test.ts` (label and click decision, `src/lib/asks.ts`), `store_taskbar::tests::taskbar_counts_and_chat_needs_answer_follow_the_taskbar_rule`
and `taskbar::tests::the_hover_text_says_why_the_dot_is_on`. The row marker,
click-to-card navigation and hover text on a real taskbar are not tested.
