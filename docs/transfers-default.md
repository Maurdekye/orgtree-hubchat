# Transfers disclosure default audit

Historical audit, superseded by the 2026-10-19 move to
[Settings > Transfers](transfers-settings.md). The chat-list header no longer
mounts these disclosure controls; current transfer history is documented in
[transfer history](transfer-history.md).

User/coordinator 2026-10-14: every Transfers panel starts collapsed. Hubchat
already does this: TransfersChip initializes local open=false at both desktop
placements; TransfersStrip initializes local open=false on Android's chat list.
Snapshots update rows without changing that local expansion; remount starts
collapsed. Neither stores a disclosure preference. There is no completed-only
Transfers list; the core snapshot feeds unfinished transfers only.

No Hubchat product change is needed. The synthetic real-component fixture in
tools/transfers-default-fixture.tsx overrides only api.transfers with local rows,
uses the real store and exports refresh/remount handles for batch verification.
The paired Orgtree tools/transfers-default-verify.mjs builds it with existing
real Hubchat dependencies read in place. That tree's transfers-default-proof.py
checks desktop/Android and dark/light initial collapse, one-click expansion,
progress update without collapse, one-click collapse and remount default.

Measured browser verification is pending rig-opus batch; no installed-app,
Android device, build, install or push claim. Existing Orgtree shared panel
is the product change; Hubchat's source defaults remain unchanged.
