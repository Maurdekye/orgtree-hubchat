# Inline video and audio play in the chat

User requests 2026-10-30: "i think an inline video player for hubchat would
be nice as well" (after Orgtree's inline-video fix), then "Yes, add audio".
Hubchat 1.2.7 plays them as Orgtree's media card does after its 4.5.3 fix
(renderer `canvas/media.tsx`): same formats, ceiling, notes and behaviour, by
the symmetry rule. The Orgtree coordinator settled the choices below the same
day.

## What you see

- **Video** (MP4, WebM): its player opens on render, showing its first frame,
  paused, while its card is on screen. Only its own controls play it. Its
  fullscreen button fills the screen; Escape (desktop) or Back (Android)
  leaves, and Escape does nothing else: the chat, a prepared reply or an open
  panel stay.
- **Audio** (MP3, WAV, Ogg, M4A, AAC, FLAC): a **Play audio** button; a click
  shows its native control row, playing. Nothing is fetched before the click.
- Scrolled away, the window hidden, or the chat changed, a player lets go of
  its file and comes back paused. It never plays on its own.
- The file row stays below the player: Download, Open, Show in folder and
  progress, as for any file.
- Past 256 MiB, a format this device can't play, or a file that won't play:
  no player, the file row, and Orgtree's note ("Playback is limited to 256
  MiB. Download this file to open it.", "This format is not supported here.
  Download the file to open it in another app.", "Preview unavailable.
  Download the file to open it in another app.").

## How the bytes get there

A player never waits for a download. Its source is
`http://hcmedia.localhost/<message id>%2F<local id>` (`api.mediaUrl`, Tauri's
`convertFileSrc`), a scheme only the main window's page may use
(`src-tauri/src/media.rs`). Each request is one range, answered a 1 MiB piece
at a time (206), as Tauri's own asset protocol answers video:

- the sources are an image preview's, in its order, without its saved copy:
  the original this device sent, then a finished download, then the hub
  (`Engine::media_range`, `crates/hubchat-core/src/engine_media.rs`);
- from the hub it is a `Range` request on `GET /api/attachments/{id}`, kept in
  memory (`HubClient::get_range`, `crates/hubchat-core/src/media.rs`); the hub
  serves ranges from its file and changes nothing;
- nothing is saved and the attachment's download state is untouched; a hub
  that no longer has the file marks it expired, as for a preview.

`preload="metadata"` reads only the start of the file (and its index at the
end if it isn't faststart) before Play.

## Fullscreen

- **Desktop**: WebView2 asks no permission for fullscreen (Orgtree's maximize
  bug was its Electron permission handler), and Tauri turns a fullscreen
  element into a fullscreen window. `src/lib/media.ts` registers, before any
  component, the Escape handler that leaves a fullscreen player and stops the
  key there; a key that follows the WebView's own exit within 300 ms is held
  back too.
- **Android**: wry's chrome client refuses a page's fullscreen (its
  `onShowCustomView` hides the view at once). `FullscreenChromeClient`
  (`src-tauri/gen/android/.../VideoFullscreen.kt`) shows that view over the
  activity, system bars hidden, until the page leaves or Back is pressed; it
  forwards every other call wry's client overrides to that client by
  delegation, so the file chooser (the recovery-file import uses it) and
  permission prompts work as before. MainActivity installs it once wry has set
  its client (posted from `onWebViewCreate`). Android 8.0+ only
  (`WebView.getWebChromeClient()`); on 7.x the refusal stays.
  `tools/check-fullscreen-client.mjs` checks the forwarding against the wry
  that `Cargo.lock` pins: re-run it when wry changes.
- On Android a player's controls start no swipe-to-reply or long-press.

## Proof

- `cargo test -p hubchat-core media`: formats, ranges and pieces, the URL,
  refusal statuses, the hub's answers (a range, more than asked, a 200, a 416,
  a refusal), and the engine's sources (the sent original, a finished
  download, the hub without a download or saved file, expiry).
- `tools/proof-inline-media.mjs [repo]` (Electron, Session 0, muted): the
  mounted app on the desktop and Android layouts with silent synthetic media
  made by ffmpeg. A stand-in answers each range as the hub does (a 206 to the
  end, streamed as the player reads it) and counts what was read. Views:
  desktop video (first frame paused, only the start of the file read, a click
  plays, fullscreen, Escape), desktop stop (scroll, hidden, chat change),
  desktop audio, desktop limits, Android video and audio.
  `HUBCHAT_PROOF_PHASE=red` expects exactly the checks the source before the
  players fails. Electron's own custom protocol takes a short 206 for the end
  of the file (a seek there fails to decode), so the stand-in doesn't cut
  pieces: the shell's 1 MiB pieces are proven where they run, by the tests
  above and the e2e below.
- `tools/e2e-media.mjs`: the real desktop test build (WebView2 and hcmedia)
  against a scratch hub: the first frame from the hub in bounded ranges
  without a download, a click plays, a fullscreen window, Escape with Contact
  info as the canary, and a seek near the end playing to the end (the last
  piece fetched, nothing failing).
- Not measured here: Android's fullscreen, its file chooser through the
  wrapper, and Android WebView's handling of the 206 pieces. The first real
  test is an alpha on a phone.
