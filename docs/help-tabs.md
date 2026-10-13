# Help section tabs

The setup guide stays in `docs/setup.md`, shared by GitHub and the application.
`splitGuide` strips the app-excluded screenshot blocks and document title, then
uses Markdown heading tokens to split at each level-two heading. Code fences do
not create tabs. The short introduction remains above the tab strip; every
section retains its heading and complete content in document order.

Settings Help and both `openGuide` dialogs use the same `GuideBody`. Its tabs have
linked tab/panel roles, one tab stop, arrow-key wraparound and Home/End navigation.
The strip scrolls horizontally and long labels wrap inside a tab bounded by the
strip width. Dialog Escape and Android back handlers remain unchanged.

Bundled guide fragment links are preserved through the existing sanitizer using
private non-navigating targets, then handled locally: heading anchors select the
owning section and reveal its heading. External links still use `openLink` and
the system browser. Message markdown's http(s)-only link policy is unchanged.


## Verification receipt ? 2026-10-13

Rig-opus measured the combined Hubchat batch (source includes this branch's
four implementation/check commits through e3eb3f8): root typecheck passed;
12 guide splitting assertions passed; all 200 browser checks in eight
Settings/desktop-modal/Android-screen dark/light cases passed. The owner
read the result and inspected Settings desktop dark, narrow desktop dark,
and Android light screenshots. Evidence:
`D:/orgtree-work/tmp/rig-opus/batch-430/evidence/h1-help4/result.json`.

Checks cover document conservation, section order, ARIA links, every tab,
narrow label geometry, roving arrow/Home/End focus, local anchor navigation,
external-link adapter, message-link policy, and original close/Escape/back.
The raw source assertion preserves exact text; the rendered comparison
ignores only formatting whitespace between separately rendered blocks.

These are synthetic real-component/headless Edge checks, not an installed
Hubchat or Android-device run. Rig ran with the batch tree's dependency
install temporarily moved aside to avoid a second React copy. The portable
proof now explicitly aliases React/ReactDOM to one existing dependency root
(syntax checked, not separately rerun); no source/dependency links or copies
are needed. Earlier proof-bootstrap/formatting failures were fixture
errors and remain recorded in the owner's breadcrumbs.
