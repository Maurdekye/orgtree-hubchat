# Account avatar editor

User 2026-10-13 asked to edit the Account picture by clicking its avatar, with
an always-visible pencil badge. The later X ruling adds a separate top-right
Remove button whenever the picture metadata has a hash. Clicking the avatar
always opens the file chooser; the X uses the existing removal call and cannot
activate the sibling chooser button. Both are native HTML buttons, with labels,
tooltips, keyboard activation and focus styles. The pencil sits bottom-right.

The existing PictureEditor owns the chooser, size check, oriented preview,
cropping, save, errors and identity intent guard. Its optional trigger renders
the Account header and starts that same flow. Its ordinary shared editor mode
remains available for existing component tests. In Account, the file input is
hidden and the crop/help/status/error controls appear only during an edit; no
separate idle editor controls remain below the avatar. Cancel hides that step.
The existing native APIs, picture cache, decoder limits and shared avatars in
other surfaces remain unchanged. Desktop keeps its 72px avatar and Android 96px.
Theme variables supply badge, focus and hover colors.

Focused verification is prepared in `tools/proof-avatar-editor.mjs`: retained
mounted picture controls, source typecheck, and the actual Settings/Avatar/editor
in synthetic Chrome desktop/Android layouts with dark/light screenshots.
Pointer, Enter, Space, crop/save, X-only removal, missing-picture state and errors
are covered. Native APIs and chooser are controlled, with no real data, hub,
device or app packaging. Invoke under the existing kernel Job wrapper with
2048MB, 180s and 512MB Node heap, shared build queue, output in owned
`C:\orgtree-build\hubchat-dm-sol\avatar-editor`.

Measured at `8227957`: **37/38**, exit 1. All 32 desktop/Android dark/light
behavior/visual checks plus oversize, preview error, save error and zero-console
controls pass; the retained picture renderer proof also passes all **15/15**.
The one failure is the isolated typecheck configuration resolving installed
React DOM/Qrcode and Vite asset types through the shared dependency root. No
production type error was established by that configuration failure. The runner
now supplies explicit declaration paths/types; corrected typecheck is pending.
Screenshots of both layouts and themes, with and without a picture, and the crop
step were captured; desktop-light and Android-dark-empty were visually inspected.

Initial receipt/config errors are retained under
`D:\orgtree-work\rig\hubchat-dm-sol\avatar-editor\initial`.
Native device chooser/Tauri acceptance and installed artifact behavior remain
outside this synthetic proof.
