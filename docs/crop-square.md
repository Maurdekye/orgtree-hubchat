# Profile picture crop square

User 2026-10-14 requested a crop square over the full fitted image, replacing
three sliders. `SquareCrop` is presentation only: it consumes the existing
oriented native PNG preview; original file bytes remain with `PictureEditor`.
Save still sends the original bytes and `PictureCrop`. Decoder, limits,
ownership, cancellation, removal and revision semantics are unchanged.

The square moves by pointer capture (mouse, touch or pen). Each corner resizes
against its opposite corner, projecting the drag onto a square and clamping to
the image and the existing 10% short-side minimum. Outside the square is dimmed.
The small 192 px live preview remains. Arrow keys move by 1% of the short image
axis; Shift + left/up shrinks, Shift + right/down grows around the center, bounded
by the image. Instructions and focusable, labeled handles provide keyboard
access. A cancelled pointer keeps its last visible crop and ends the gesture;
the existing Cancel button discards the entire edit. Other simultaneous pointers
are ignored (pinch is not implemented). Busy state and replacement previews end
the gesture. Touch handles have 44 px targets.

Native x/y are fractions of available top-left travel, not image coordinates.
`cropmath` converts those to image-pixel squares and back. Hubchat's desktop and
Android shared editor carries matching component/math/CSS in its own repository;
there is no cross-repository runtime dependency.

## Verification handoff (source only)

No author execution: rig-opus runs the shared lock/Job batch. Bundle
`tools/cropmath.test.ts` and run its three Node tests. The updated existing
`tools/proof-pictures-renderer.mjs` has 16 mounted checks: full image/four handles
instead of sliders and keyboard resizing to the native save contract, alongside
existing owner/late-preview/removal/privacy checks. On predecessor 4f3db23 with
this same proof, expect two failures (new controls absent and size unchanged),
with other checks passing. Preserve that distinction from pure geometry tests.
The existing avatar proof now expects the crop square instead of a slider.

`tools/crop-visual.mjs` requires CROP_JOB_CONTAINED=1, CROP_OUT under the runner's
C build root, CROP_DEPS for Hubchat dependencies, CROP_TOOLS_DEPS for esbuild,
and CROP_CDP pointing to this repo's tools/cdp.mjs. It captures before/after in
both themes for desktop and 390 px mobile Account settings. The predecessor
editor comes from git show 4f3db23 (or CROP_BASE). Green checks cover keyboard,
actual pointer/touch corner resize and movement, bounds/square, responsive
layout and save. Synthetic native commands and emulated touch are not native
image decoding, Android WebView, real chooser or installed-app acceptance.