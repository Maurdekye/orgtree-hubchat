# Copy image, and the zoomed picture's menu

User 2026-11-25: "add a "copy image" context menu entry to images in hubchat, and add context menu actions to zoomed
in images (they have none atm)".

## What you see

- **Every picture's menu has Copy image**, right after View. This covers a chat picture (right-click on desktop and
  Hubchat Web, long-press on Android) and a picture in Contact info's files. It puts the picture itself on the
  clipboard and says "Image copied", or "Couldn't copy the image: <reason>".
- **On Android a long-press on a chat picture opens its menu**, as a long-press on a mention opens its sheet. The menu
  holds the picture's actions, then Reply and Select. A long-press elsewhere on a message still selects it, and a swipe
  that starts on the picture still replies.
- **The zoomed picture has the same menu**: right-click on desktop, long-press on Android. It is the same menu
  component and the same entries, less View (you're already viewing it). Clicking away to dismiss the menu leaves the
  viewer open; a click on the backdrop with no menu open still closes it. On Android the sheet shows above the viewer.

## How

- `copyImage` (`src/lib/images.ts`) writes one `ClipboardItem` of `image/png`. PNG is the one picture type every
  clipboard takes, so a JPEG, WebP or GIF is drawn into PNG first (a GIF gives its first frame). The bytes are the
  attachment's preview, which is the file itself, up to `PREVIEW_MAX`. The write is called inside the menu click, with
  the picture still loading, so the click is the user gesture the clipboard asks for.
- The menu is `attachmentMenu(m, a, platform, viewing)` (`src/lib/attachmentActions.ts`). `viewing` leaves out View.
  `menuIsOpen()` (`src/lib/ctxmenu.ts`) lets the viewer tell the dismissing click from a closing one.

## Proof and limits

- `tools/proof-copy-image.mjs` mounts the real App on desktop and Android in Electron, with a stub clipboard. It checks
  the menu entries and their order, that the menus show on top, the PNG written (type, magic bytes, 64x48 from a JPEG
  source), the toast, the viewer's menu, dismissal, and the Android long-press rules. It captures both menus on both
  layouts.
- Not measured: the real clipboard of WebView2 (Windows), WebKitGTK (Linux, the Steam Frame), WKWebView (macOS) and the
  Android System WebView. The code uses only the standard async clipboard API. If a WebView refuses the write, the user
  sees "Couldn't copy the image" with the reason, and nothing else changes. Check at the alpha hands-on. A native write
  (arboard, already used for Paste) is the fallback if a desktop WebView refuses.
