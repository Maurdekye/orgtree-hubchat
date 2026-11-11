# Hubchat Web

Hubchat Web is Hubchat as a web page: the same UI and the same Rust core as
the desktop and Android apps, running in a browser tab or as a Home Screen
app on an iPhone. The page's server only hands out files. It never sees a
key, a message or a hub address: the core runs inside the browser and talks
to the hub itself.

This document covers the first slice (the trial build). What it does not do
yet is listed under [Not there yet](#not-there-yet).

## What works

- Making an identity, or bringing one back with its 24 recovery words.
- Adding hubs, the chat list, text messages both ways with Markdown,
  replies, receipts, typing, presence, drafts, deleting, the directory,
  Settings.
- Saving the recovery words: Copy, or Download (the share sheet on an
  iPhone). On the web the words must be saved before the chat list opens.
- One tab at a time: a second tab says "Hubchat is open in another tab or
  window" and can take over with **Use it here**.
- On an iPhone or iPad, a browser tab only shows how to add Hubchat to the
  Home Screen. The app itself runs from the Home Screen, where the phone
  keeps its storage.

## How it is put together

```
page (React UI, src/)  <- postMessage ->  worker (hubchat-core as WebAssembly)  <- fetch ->  hub
     src/web/webApi.ts                    src/web/core.worker.ts, crates/hubchat-web
```

- **The core** is `hubchat-core`, compiled for `wasm32-unknown-unknown`. It
  runs in one dedicated worker. Its database is the same SQLite schema as
  the apps', in the origin's private file system (OPFS, through
  `sqlite-wasm-vfs`'s `sahpool`). That file system gives its fast file
  handles to dedicated workers only, and to one owner at a time.
- **The shell** is `crates/hubchat-web`. `shell.rs` answers the UI's
  commands from the core; it knows nothing of a browser, so its tests run
  natively. `wasm.rs` starts it in the worker and passes commands, events
  and the key across.
- **The key** stays in the browser: IndexedDB database `hubchat-keys`,
  encrypted with an AES-GCM key the browser will not give out
  (`src/web/keys.ts`). This is no defence against script running in the
  page's own origin. Whoever runs the web server controls the code the
  browser runs, so use a server you or your organisation run.
- **One tab owns the core.** A tab takes the Web Lock `hubchat-core`
  before starting the worker. **Use it here** asks the owner on the
  BroadcastChannel `hubchat`; the owner stops its engine, ends its worker
  and releases the lock (`src/web/webApi.ts`).
- **Waking up.** A phone freezes a background page and its timers. When the
  page is visible again, or the network returns, the page calls
  `hc_retry_now` so every hub's sync restarts at once.

## Rules for the code

- **`crate::rt` in `hubchat-core`.** Tasks, timers, clocks and `fs` go
  through `crate::rt` (`crates/hubchat-core/src/rt.rs`): `rt::spawn`,
  `rt::task`, `rt::time`, `rt::Instant`, `rt::system_now()`, `rt::fs`. On
  native targets these are tokio and std, unchanged. In the browser they
  are `spawn_local`, browser timers and `web-time`. Do not call
  `tokio::spawn`, `tokio::time`, `std::time::Instant::now()` or
  `SystemTime::now()` directly in the core: they compile for the browser
  and panic there. Tests may keep using tokio.
- **Native-only code has a web twin.** The hub stream (`stream.rs`, see
  `docs/hub-stream.md`) opens its socket with tokio's network stack, so
  `lib.rs` compiles it for native targets only and gives the browser
  `stream_web.rs` under the same module name: the same `Carrier` and
  `TransportStatus`, always the HTTP long poll. Code that cannot run in a
  page is gated the same way, never left to fail at run time.
- **Commands keep their names.** The page calls the same `src/api.ts`
  table as the apps (`makeApi(transport)`). A command the web shell
  answers has the Tauri command's name, argument names and answer. A
  command it cannot do yet answers "This isn't available in Hubchat Web
  yet.", never a pretend success.
- **The apps' bundle holds no web code.** Hubchat Web has its own entry
  (`web/index.html`, `src/web/main.ts`) and its own Vite config
  (`vite.web.config.ts`). Nothing under `src/` outside `src/web` imports
  from `src/web`.
- **What differs in the UI** hangs on `useWeb()` (`State.shell === "web"`)
  or `isWeb`. `platform` still means the layout: the phone layout below
  900 px wide, the desktop layout from 900 px, chosen once at start.
- **A hub must be HTTPS.** A page served over HTTPS may not call `http://`
  addresses, so the web shell refuses them before any request
  (`page_can_reach`), except `localhost`, `127.0.0.1` and `[::1]`.

## Building

You need:

- the Rust target: `rustup target add wasm32-unknown-unknown`;
- a clang that makes wasm32 objects, because SQLite is C. Either on `PATH`,
  or named by `CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown`
  (`clang` and `llvm-ar`);
- the `wasm-bindgen` command of exactly the version `Cargo.lock` pins for
  the `wasm-bindgen` crate: `cargo install wasm-bindgen-cli --version
  <that version> --locked`. Set `WASM_BINDGEN` if it is not on `PATH`.

Then:

```
npm ci
npm run build:web
```

`tools/web-build.mjs` builds the core (`--profile web`: optimized for
size), runs `wasm-bindgen`, runs Vite, and checks the size budget: what a
first visit downloads, compressed, must be at most 3 MB, or the build
fails. The page lands in `dist-web/`.

## Running it

```
npm run serve:web
```

serves `dist-web/` on `http://127.0.0.1:8787` with the Content Security
Policy the real server will send. A browser treats `http://localhost` as
secure, so this works on the same machine with a hub on that machine.

### Trying it on a phone

A phone needs HTTPS for the page and for the hub. With Tailscale, on the
machine that runs both:

```
tailscale serve --bg --https=443 http://127.0.0.1:8787     (the page)
tailscale serve --bg --https=8443 http://127.0.0.1:7378    (the hub's door)
```

Then tell the hub which page may call it: set `HUB_WEB_ORIGINS` to the
page's address, for example `https://pc.example.ts.net` (Mail hub 2.5.0 or
later; its `docs/OPERATIONS.md`). Without that the web app says "This hub
doesn't allow this web app yet" and shows the address to add.

On an iPhone: open the page's address in Safari, tap **Share**, then **Add
to Home Screen**, and open Hubchat from the Home Screen. In Hubchat, add
the hub as `https://pc.example.ts.net:8443`.

## Tests

- `cargo test -p hubchat-web --lib`: the shell's commands, natively.
- `cargo check -p hubchat-web --target wasm32-unknown-unknown`: the core
  and the shell build for the browser.
- The whole native suite (`hubchat-core`, `hubchat`) must stay green: the
  runtime seam changes no native behaviour.
- `node tools/web-e2e.mjs --dist <built page> --hub <orgtree-mailhub
  executable> --pg <the hub suite's PostgreSQL template> --out <folder>`:
  the built page in a headless Chromium against a real hub on loopback.
  It checks the install screen on an iPhone's user agent, the start from
  the Home Screen and the keyboard handling (with a stand-in for the
  phone's visual viewport), onboarding, the hub refusal message, the
  recovery words download, a message both ways with a scripted peer,
  receipts, Settings, a reload, and the second-tab takeover; and that
  there is no script error, no Content Security Policy violation and no
  missing file. `--width 1280 --height 800` runs the desktop layout. On
  Windows it refuses to run outside Session 0.

## Not there yet

These answer "This isn't available in Hubchat Web yet." or are hidden:

- Files: attaching, receiving, pictures, video and audio; the profile
  picture.
- Linking devices, key files and QR codes. Recovery words are the one way
  to bring an identity for now.
- The hub stream: Hubchat Web long-polls every hub (`stream_web.rs`) until
  it has a carrier over the browser's WebSocket.
- Notifications (Web Push).
- Working offline and the update banner (no service worker yet).
- The container and its server. `tools/web-serve.mjs` is a development
  server.

## Not checked on a real device

Headless Chromium on a PC is not an iPhone. Until someone runs it on one,
these are unverified: the database inside a Home Screen app on iOS, the
keyboard handling (`src/web/viewport.ts`), the safe areas around the notch
and home bar, and start time on a phone. Firefox and Safari on a desktop
have not been run either.
