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
- Linking: **Link from your other device** shows a code for the other
  device, or scans the QR code the Hubchat app on a PC or phone shows
  (Settings, Link a device), with this device's camera. A signed-in browser
  can link another device the same way. The key QR code and the
  passphrase-locked key file work too.
- Suggested hubs: when someone makes a new identity, the Add hub step
  offers the hubs the server lists, or with none listed a hub that answers
  at the page's own address. The server can switch this off. A hub is added
  only when they tap **Add**. See [The suggested hub](#the-suggested-hub).
- Adding hubs, the chat list, text messages both ways with Markdown,
  replies, receipts, typing, presence, drafts, deleting, the directory,
  Settings.
- Saving the recovery words: Copy, or Download (the share sheet on an
  iPhone). On the web the words must be saved before the chat list opens.
- Opening without a network: the app's files are kept in the browser, so
  Hubchat starts and shows the chats it holds when the server or the
  network is away. A message written then waits and goes when a hub is
  reachable, as in the apps.
- Updates: when the server has a newer build, the browser fetches it in the
  background and the apps' update banner says "A new version of Hubchat is
  ready." **Update** reloads into it. Until then the build that is running
  stays whole. See [Updates and offline](#updates-and-offline).
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
  (`page_can_reach`), except `localhost`, `127.0.0.1` and `[::1]`. A link,
  key QR code or key file brings only the hubs a page can reach; with none
  it says so in one sentence.
- **What only a page can do goes through `WebNative`**
  (`src/lib/native.ts`): the camera scan, choosing and saving a key file,
  the suggested hubs. `src/web/webApi.ts` fills it in at start, so the
  components keep calling `scanQr()`, `pickKeyFile()` and the rest, and
  nothing outside `src/web` imports from it.
- **A browser with an identity does not take another by link.** Scanning a
  link or key code while signed in answers "This browser already has an
  identity."; sign out of it first.

## Linking and the camera

The web shell's link commands are in `crates/hubchat-web/src/link.rs`, with
the apps' names and answers (`hc_link_start`, `hc_link_confirm`,
`hc_link_offer`, `hc_link_approve`, `hc_key_qr`, `hc_restore_qr`,
`hc_key_file_import` and the rest; events arrive as `hc-link`). The key is
saved in the browser before the identity is adopted, as for a new identity.
A key file is read and written as text: the page hands the chosen file's
text to the core, and saves the one the core makes through the browser's
download or share sheet.

The page shows the camera full screen (`src/web/scan.ts`, `getUserMedia`
with the back camera at 1080p, continuous focus where the camera takes the
setting). Where the browser reads QR codes itself (`BarcodeDetector`, Chrome
on Android) it is asked first. An iPhone's browser has no such reader, so
every frame also goes to the worker as grey pixels, at the camera's own size,
and the core reads it (`scan.rs`, the `rqrr` crate without its default
features, licence `(MIT OR Apache-2.0) AND ISC`).

A code on a monitor is a hard picture: small in the frame, soft, tilted, on
a dark window with glare. `scan.rs` therefore reads a frame up to five ways,
cheapest first: as it is; smoothed, with black decided against the square
around each pixel; the middle two thirds enlarged twice and sharpened; the
same sharpened hard (a picture well out of focus); the same without
sharpening. Measured on drawn photo-like frames (1080 x 1920,
noise, blur): it reads from about 5 pixels a module up, and often at 4; the
reader alone needed about 8 once the picture was soft. The ignored test
`scan::tests::what_the_reader_manages` prints that table. The first real
camera try (2026-11-11) failed because frames were scaled to 800 pixels,
which left the code under 3 pixels a module. Linking and the reader add about 143 KB
to the compressed first load. The camera needs a secure page (HTTPS or
localhost) and the person's permission; refused, the scan says so and the
typed code still works.

## Updates and offline

A service worker keeps the app's files (`web/sw.template.js`). The build
writes `sw.js` from that template with the list of the build's files and a
hash of their content (`tools/web-sw.mjs`, called by `tools/web-build.mjs`
after Vite); the cache is named after the hash.

- It answers the page's own files from its cache, every navigation with
  `index.html`, and `/config.json` from the server when it answers, else
  from the last copy. Requests to any other address (the hubs) are not
  touched: the worker never sees the identity and never calls a hub.
- The page registers it as the app starts (`src/web/update.ts`), not on the
  iPhone's "Add to Home Screen" screen, where no app runs.
- A newer `sw.js` is fetched and its files kept, then it WAITS: it does not
  take over a running page. The apps' update check (`src/lib/updates.ts`:
  at start and every 6 hours) asks `update.ts`, which asks the browser to
  look for a newer worker; with one waiting the banner shows. **Update**
  tells the waiting worker to take over and reloads the page when it has.
  The old cache is deleted then. Closing every Hubchat tab and opening it
  again loads the newer build too.
- On the phone layout the banner is on the chat list, as in the Android
  app.
- The first visit downloads everything once more than the app itself needs
  at start (the parts loaded later, the icons): about 1.6 MB compressed
  against 1.5 MB. The core is not fetched twice.
- A server that serves the plain files must send `sw.js` and `index.html`
  with `Cache-Control: no-cache`, or browsers keep an old build.
  `tools/web-static.mjs` does.

## An older server than the browser last used

A server can go back to an older build (a rollback, or two servers behind
one address). The browser's database may then have been written by a newer
build, with a schema the older code does not know. Hubchat Web does not
open it: `Store::open_refusing_newer` (hubchat-core) reads the schema
version before it writes anything and refuses a newer one, leaving the file
as it is. The start screen then reads **Update the Hubchat Web server**,
names the address that serves the older build and its version, says that
the identity and messages are untouched, and offers **Reload**
(`src/web/startError.ts`; the page knows the refusal by the core's words
`store::NEWER_STORE`, which the two files must keep the same). Once the
server has the newer build again, a reload opens the data as before.

The apps are not changed: they open their database with `Store::open`, as
always.

Tests: `store::tests::a_store_from_a_newer_build_is_refused_and_left_alone`
(hubchat-core) and `node --test tools/web-start-error.test.ts`. The screen
itself has not been produced in a browser: the test has no way to put a
newer database into the browser's private file system.

## The suggested hub

Someone who has just made a new identity and has no hub yet is offered hubs
at "Add your hub". The server decides which, in `/config.json`
(`src/web/suggest.ts` reads it; the cards are the Add hub step's own):

| The server says | The page offers |
|---|---|
| `"hub_suggestions": false` | nothing, and it checks no address. This wins over a list. |
| `"suggested_hubs": [...]` (not empty) | exactly those hubs, in that order, each as its own card. The server's own address is not added to the list. |
| neither | the address the page is served from, then `https://` plus its host; a card only if a hub answers there, otherwise nothing |

- Every address is checked the way a typed address is (the hub must allow
  this web app's origin). A hub that answers shows as "Found <hub>" with
  **Add <hub>**.
- A LISTED hub that does not answer is shown too, with the card a typed
  check gives and its address: "<address> can't be reached right now", with
  **Try again** and **Add anyway and keep trying**; or "This hub doesn't
  allow this web app yet" with the address its owner must add. A plain Add
  never adds a hub that did not answer.
- Nothing is added without a tap. Typing an address takes the suggestions
  away; after one hub is added the others stay offered.
- People who bring an identity back (words, a link, a key file) are offered
  nothing: their hubs come with it.
- A listed entry is `https://host[:port]`, or a bare `host[:port]`, taken as
  https. At most 8 are offered. A server with no `/config.json` counts as
  "neither".

The server sets the two from `HUBCHAT_WEB_SUGGEST_HUBS` (a comma-separated
list) and `HUBCHAT_WEB_HUB_SUGGESTIONS` (`true` by default; `false`
switches suggesting off): `docs/hubchat-web-server.md`. The development
server takes them as flags:

```
npm run serve:web -- --suggest-hubs https://hub.example,other.example
npm run serve:web -- --hub-suggestions false
```

`/config.json` also carries `name`, the label the server's operator gave
it (`HUBCHAT_WEB_NAME`, `--name` on the development server): the welcome
screen shows it under the title (`serverName()` in `src/web/suggest.ts`).

A hub on the same machine as the page but on another port is offered only
when it is listed: a page cannot guess ports.

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

The real server is the program `hubchat-web` (`crates/hubchat-web-server`),
with the built page inside it, and its container image: how to run it, its
settings and what it answers are in `docs/hubchat-web-server.md`. For
development:

```
npm run serve:web
```

serves `dist-web/` on `http://127.0.0.1:8787` with the Content Security
Policy the real server sends (a test of the server keeps the two equal). A browser treats `http://localhost` as
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
  suggested hubs (one that answers, one listed that does not, none with
  suggestions switched off), the recovery words download, a message both ways with a
  scripted peer, receipts, Settings, a second browser that links by
  scanning the first one's QR code (its camera replaced by a picture of
  that code), a reload, the app's files kept by the service worker, a
  start with the server gone, a newer build put on the server (the banner,
  Update, the reload into it, the old cache gone), and the second-tab
  takeover; and that
  there is no script error, no Content Security Policy violation and no
  missing file. `--width 1280 --height 800` runs the desktop layout. On
  Windows it refuses to run outside Session 0. Every browser it starts has
  both clipboard permissions denied and the page's clipboard calls replaced
  by ones that refuse, before any page loads; it prints what it measured
  (`receipt: clipboard ...`) and stops if a browser is not guarded.

The server has its own tests: `cargo test -p hubchat-web-server` (settings,
every answer, and the built program asked over real sockets, HTTPS
included), `.github/ci/web/smoke.sh` (the program or the image), and
`tools/web-e2e.mjs --server <the hubchat-web program>`, which at its end
opens the app from the program itself: it starts, shows the server's name,
keeps its camera and nothing else (Permissions-Policy), and registers its
service worker, with no policy violation.

## Not there yet

These answer "This isn't available in Hubchat Web yet." or are hidden:

- Files: attaching, receiving, pictures, video and audio; the profile
  picture.
- The hub stream: Hubchat Web long-polls every hub (`stream_web.rs`) until
  it has a carrier over the browser's WebSocket.
- Notifications (Web Push).
- Settings, About has no "Check for updates" button on the web: the check
  runs by itself.
- The release workflow that builds and publishes the server's image.

## Not checked on a real device

Headless Chromium on a PC is not an iPhone. Until someone runs it on one,
these are unverified: the database inside a Home Screen app on iOS, the
keyboard handling (`src/web/viewport.ts`), the safe areas around the notch
and home bar, and start time on a phone. Firefox and Safari on a desktop
have not been run either. No real camera has scanned a code: the browser
test feeds the scanner a picture. The key file and the key QR code are
covered by the shell's tests only, not in a browser.
