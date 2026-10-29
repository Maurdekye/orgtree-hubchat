# A local Orgtree org's badge opens it in Orgtree

User request 2026-10-29 (relayed by the Orgtree coordinator): "when inspecting
the info of a contact, if its an orgtree org from the local install of orgtree
on the same pc, then the "orgtree org" should be a clickable link that opens
that org up". The design (a listing file, an exact address match, a registered
install and Orgtree's own taskbar argument) was approved by the Orgtree
coordinator the same day.

## What the user sees

On desktop, Contact info's **Orgtree org** badge of an org that the Orgtree
install on this PC hosts is a button. It looks like the plain chip and has a
pointer, an underline on hover, the tooltip **Open in Orgtree** and the spoken
label "Open <name> in Orgtree". Clicking it starts Orgtree on that org. A
running Orgtree opens or focuses the org's window; a stopped one starts and
opens it. If Orgtree can't be started, one message says "Orgtree couldn't be
opened" and the badge is plain until the next check.

Every other badge stays plain: an org on another computer, any org on Android
(a phone hosts no Orgtree), and the conversation header's badge (the user
named Contact info; the coordinator kept the header plain).

## How it knows

Orgtree's engine keeps `local-orgs.json` in its data root,
`%APPDATA%\Orgtree v2\data`: `{"v":1,"orgs":[{"slug","name","address"}]}`, the
active orgs that have a hub address, nothing secret, rewritten whenever that
list changes (Orgtree `local_orgs.rs`). `src-tauri/src/local_orgtree.rs`
(Windows only) reads it:

- An entry counts only with an Orgtree slug (lowercase letters, digits and
  single hyphens, none at either end, at most 128 characters) and an address.
  Another version, a damaged file or one over 256 KiB lists nothing.
- The listing counts only while an Orgtree install is registered: an
  Uninstall entry named "Orgtree" or "Orgtree <version>" whose program
  (DisplayIcon, else InstallLocation\Orgtree.exe) is an existing absolute
  `Orgtree.exe`. A listing an uninstalled Orgtree left behind links nothing.
  A development channel ("Orgtree Dev") keeps its own data root and is not
  matched.
- A contact is local when its address equals a listed address exactly.

`hc_local_orgtree_orgs` returns the listing; `src/lib/localOrgtree.ts` asks
at most once a minute, and again when the window comes back after that.
Nothing here uses the network, Orgtree's port or token, or its
`engine-attach.json`.

## How it opens

`hc_open_orgtree_org` accepts only a slug the listing holds now, finds the
registered `Orgtree.exe` in the registry (never from the listing) and starts
it detached with exactly one argument, `--orgtree-taskbar=org:<slug>`, with no
shell. That is Orgtree's own taskbar entry point (Orgtree
`apps/desktop/main/jumplist.ts`): a second instance hands it to the running
Orgtree, and a cold start queues it until its windows are ready. The log
records only that Orgtree was started or could not be.

## Focused verification

- `cargo test --locked -p hubchat --lib local_orgtree -- --nocapture`
  (Windows; `dist/index.html` must exist for the Tauri build): slugs, the
  listing, the registered program, the single argument, and a stand-in
  `Orgtree.exe` compiled by the test that records the arguments `launch`
  starts it with (printed as `recorded argv`).
- `tools/proof-orgtree-badge.mjs` (kernel Job, a new output folder under
  `C:/orgtree-build`, a Session 0 turn: its offscreen Electron asserts
  Session 0) mounts the real App and Contact info on the synthetic API: a
  local org (the badge is a button with the tooltip, label and chip look; the
  click asks to start Orgtree on its slug), an org on another computer
  (plain), a start that fails (one message, then plain) and Android (plain,
  never asks). `HUBCHAT_PROOF_PHASE=red` expects exactly the plain badge's
  failures. Green captures each view, the local badge under the pointer.
