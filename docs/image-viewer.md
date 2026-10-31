# The image viewer zooms the picture, never the app

User report 2026-10-31 15:03:04Z (md50218422de7, via the phone link): "hubchat
android bug: when i zoom in on an image and close the image, the app stays
zoomed in and i cant undo it without restarting the app".

## What you see

Tap an image in a chat to open it full size. In the viewer:

- **Pinch** with two fingers (Android) to zoom the picture, from fit up to 6x.
  The point between your fingers stays where it is.
- **Drag** with one finger to move a zoomed picture. At fit there is nothing to
  move, and the picture never leaves its box: an edge stops at the edge of the
  screen.
- **Double tap** (desktop: double click) toggles between fit and 2.5x at the
  point you tapped.
- **Desktop trackpad pinch or Ctrl + mouse wheel** zooms about the pointer.
- **Close** with the back arrow, the system back gesture, a tap outside the
  picture (desktop: Esc too). The end of a pinch or a drag is never read as that
  tap, so letting go does not close the viewer.

Closing, however it happens, leaves the app at its normal size. Zooming in the
viewer moves only the picture.

## Why it broke

The viewer was a plain picture in an overlay. The Android WebView lets a page
be pinch-zoomed (the viewport meta sets no limit), so a pinch over the viewer
scaled the whole page, and nothing put it back on close.

## What changed

- The overlay takes the touches itself (`touch-action: none` in
  `src/styles/images.css`), so the WebView no longer scales the page under a
  gesture on the viewer.
- Pinch, pan, double tap and Ctrl + wheel are `src/components/pinchmath.ts`
  (pure functions) driven by pointer events in `AttImage.tsx`; the picture is
  moved with a CSS transform.
- On opening and on closing, `resetPageZoom` pins the viewport meta to
  `minimum-scale=1, maximum-scale=1` for two animation frames and restores its
  original content, which returns the page to 1:1 even if something else scaled
  it. Nested resets (open, then close; React's double effect) restore the
  original once, at the end.

## Proof, and what it does not show

- `tools/pinchmath.test.ts`: the math and the reset (7 Node tests).
- `tools/proof-image-zoom.mjs`: the mounted viewer in jsdom, mock core only:
  overlay claims the touches, a pinch zooms the picture and not the page, the
  end of a pinch does not close, opening and closing pin and restore the
  viewport meta. At the predecessor 3c19428 it passes 10 of 20 checks; with the
  fix, 20 of 20.
- jsdom has no pinch-zoom of its own, so this does not show the real WebView.
  The Android-viewport check (emulated mobile with touch pinch, reading
  `window.visualViewport.scale` before, during and after) is the integrator's;
  no device run is claimed here.

Orgtree's own inline image viewer (`canvas/gallery.tsx`) has no touch handling
either; its desktop is driven by a mouse and the bug was not checked there.
