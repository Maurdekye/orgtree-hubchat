# Transfers in Settings

The desktop chat-list header and compact rail, and Android's chat-list header,
no longer show a Transfers bar or chip. Open Settings > Transfers to see current
progress and finished transfer history, including when nothing is running.
This follows the user's 2026-10-19 23:13Z request; the user explicitly allowed
either Settings or a small button, and this uses the shared Settings navigation.

Active rows reuse cancellation and chat navigation. Finished rows retain View,
Redownload, missing/expired-file handling and bounded Load more paging. History
loads only when the section mounts. In-chat transfer progress, scheduling,
storage, native commands and file capabilities are unchanged. Existing agent
file/shell access covers file operations; this navigation relocation adds no
underlying capability requiring a new tool.

The focused `tools/proof-transfers-settings.mjs` fixture mounts the real app with
synthetic APIs on both layouts. It checks header/rail removal, filters, Settings
entry, progress/history, Cancel/View, chat navigation and idle history access,
and captures both the chat list and Transfers section. It does not connect to a
hub, open files, write live state, build/install an app or exercise a real device.
The release version/notes and publishing work belong to the coordinator.

Author verification: the final synthetic desktop/Android fixture passes 26/26
checks at `beccd11`, with TypeScript also passing. The tests-only control
`5589f27` matches 19 expected failures and seven passing controls (desktop
10 fail/3 pass, Android 9 fail/4 pass). The combined chat-list and Settings
capture was inspected. One filter-label fixture mismatch and one cross-layout
saved-chat leak were corrected; their evidence is retained separately. Actual
native transfer/device behavior and release builds remain release verification.
