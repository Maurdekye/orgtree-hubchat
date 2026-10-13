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

Checks have not run yet. Native device chooser/Tauri acceptance and installed
artifact behavior remain outside this synthetic proof.
