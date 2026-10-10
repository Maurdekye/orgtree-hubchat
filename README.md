# Hubchat

Hubchat is a chat app for the [orgtree mail hub](https://github.com/Maurdekye/orgtree-mailhub).
With it you can message anyone registered on the same hub: people, Orgtree
organizations, and AI agent sessions such as Claude Code or Codex. It runs on
Windows and Android, and there are first prototype builds for macOS and Linux.

- Chat with anyone on your hubs, and browse each hub's directory of people,
  organizations and agent chats. You can use several hubs at once.
- Pictures show in the chat, and you can send files of any size up to the
  hub's limit.
- Use one identity on your phone and your PC: link them with a QR code, and
  both get every message.
- Recovery words bring your identity back if you lose a device.
- On Android, Hubchat can stay connected for instant notifications, or check
  about every 15 minutes. Optional [UnifiedPush](docs/unifiedpush.md), with
  mail hub 2.1.0 or later, receives wake-ups through a distributor such as
  ntfy, without Hubchat's ongoing connection notification. On Windows it sits
  in the tray, can start with Windows, and offers updates when they come out.

Linking devices and syncing between them need a hub running mail hub v2.0:
the standalone hub, or the one built into Orgtree 4.1.0 and later.

## Screenshots

<p>
  <img src="docs/screenshots/desktop-chat.jpg" alt="Hubchat on Windows: a chat with an Orgtree organization" width="900">
</p>
<p>
  <img src="docs/screenshots/phone-chats.jpg" alt="Hubchat on Android: the chat list" width="260">
  <img src="docs/screenshots/phone-chat.jpg" alt="Hubchat on Android: a chat with an Orgtree organization" width="260">
  <img src="docs/screenshots/phone-directory.jpg" alt="Hubchat on Android: the Directory of everyone on your hubs" width="260">
</p>

## Download

Get the latest release from the
[Releases page](https://github.com/Maurdekye/orgtree-hubchat/releases/latest).
`SHA256SUMS.txt` on that page lists each file's SHA-256 checksum.

- **Windows 10 or 11 (64-bit):** run `Hubchat_<version>_x64-setup.exe`. The
  installer isn't code-signed, so Windows SmartScreen may say "Windows
  protected your PC": choose **More info › Run anyway**. Hubchat then checks
  this page for signed updates and offers **Restart to update** when one is
  out.
- **Android 7.0 or later (64-bit ARM, which covers most phones):** download
  `Hubchat_<version>_arm64.apk` on the phone and open it. Allow your browser
  or file manager to install apps when Android asks. From 0.1.3 on, Hubchat
  offers each update in a banner: tap **Update**, and it downloads, checks and
  installs it, keeping your chats and identity. (Coming from 0.1.2 or earlier,
  install the newer APK over the old one once.)
- **macOS (prototype, Apple Silicon and Intel):** open
  `Hubchat_<version>_universal.dmg` and drag Hubchat to Applications. This
  build hasn't been tried on a real Mac yet. It isn't signed by Apple, so macOS
  blocks the first open. On macOS 15 or later, try to open it once, then go to
  **System Settings › Privacy & Security** and click **Open Anyway**. On macOS
  14 or earlier, right-click the app and choose **Open**.
- **Linux (prototype, 64-bit):** `Hubchat_<version>_amd64.AppImage` (run
  `chmod +x` on it first; it needs FUSE 2, `sudo apt install libfuse2`, or
  `libfuse2t64` on Ubuntu 24.04 and later) or `Hubchat_<version>_amd64.deb`
  (`sudo apt install ./Hubchat_<version>_amd64.deb`). This build hasn't been
  tried on a real Linux PC yet. Hubchat keeps your identity key in the desktop
  keyring (GNOME Keyring or KWallet).

## Setting up

The easiest way needs Orgtree 4.1.0 or later on your PC, and
[Tailscale](https://tailscale.com/download) on your PC and your phone:

1. In Orgtree, open **App settings › Mail hub › Connect your phone** and
   follow the panel.
2. On your phone, scan the panel's download code to get Hubchat. In Hubchat,
   tap **Scan setup code** and scan the setup code.
3. Keep Tailscale on, on your phone: in Android's Settings › VPN, tap the gear
   beside Tailscale and turn on **Always-on VPN**, and set Tailscale's battery
   use to **Unrestricted**. Otherwise Android may switch it off, and Hubchat
   can't reach your PC.

Hubchat checks that it can reach your PC, joins your PC's hub and links you
to your organization, so your agents know that messages from your phone come
from you. To use the same identity on another phone or PC, link it from
**Settings › Devices › Link a device**.

The [setup guide](docs/setup.md) covers every way in: Orgtree with
Tailscale, home Wi-Fi only, your own VPN, someone else's hub, and adding a
phone or a PC. The same guide is in the app under **Settings › Help**.

<p>
  <img src="docs/screenshots/setup-orgtree-panel.jpg" alt="Orgtree's Connect your phone panel: this PC on the Tailscale network, then Turn on phone access" width="430">
  <img src="docs/screenshots/setup-orgtree-setup-code.jpg" alt="Orgtree's panel showing the setup code for Scan setup code" width="430">
</p>
<p>
  <img src="docs/screenshots/setup-phone-welcome.jpg" alt="Hubchat's welcome screen on a phone, with Scan setup code first" width="200">
  <img src="docs/screenshots/setup-phone-name.jpg" alt="Hubchat asks for your name before chatting with Test Org" width="200">
  <img src="docs/screenshots/setup-phone-linked.jpg" alt="Hubchat's chat with Test Org, headed Linked" width="200">
</p>
<p>
  <img src="docs/screenshots/setup-desktop-welcome.jpg" alt="Hubchat's welcome screen on a PC" width="430">
  <img src="docs/screenshots/setup-desktop-link.jpg" alt="Settings › Devices › Link a device on a PC, showing a code for the new device" width="430">
</p>

## Getting a mail hub

Hubchat's messages travel through a mail hub. You can run your own, in
Orgtree or on its own. The app has the same help under **Don't have a mail
hub?**.

### With Orgtree (it has one built in)

1. In Orgtree, open **App settings › Mail hub**.
2. Under **Public access**, turn on **Also serve a relay-only door on port
   7371**, then click **Save hosting settings**. Leave **Hosting › Listen
   on** at **This computer only**.
3. In Hubchat, add the computer's address and port 7371, for example
   `home-pc:7371`. On that computer itself, use `localhost:7371`.

**Why not "This computer and the local network"?** That opens the hub's main
port, whose page shows every message on the hub, so anyone on your Wi-Fi
could read them all. The relay-only door only relays mail: it has no such
page, and each person can read only their own.

### On its own (mail hub v2.0)

Get it from [orgtree-mailhub](https://github.com/Maurdekye/orgtree-mailhub).
In its folder, copy `.env.example` to `.env` and set `HUB_DB_PASSWORD`. For
the same reason as above, also set `HUB_PUBLIC=1` (the relay-only door) and
`HUB_BIND=127.0.0.1` (the main port stays on that computer). Then run
`docker compose up -d --build`, and in Hubchat add the computer's address and
port 7378, for example `home-pc:7378`.

### From outside your network: use Tailscale

[Tailscale](https://tailscale.com/download) is the safer, simpler choice:
nothing is opened to the internet, only devices in your tailnet can reach the
hub, and Tailscale encrypts the traffic (the hub has no encryption of its
own).

1. Install Tailscale on the hub's computer and on each phone or PC that runs
   Hubchat.
2. In Hubchat, add the hub by the computer's Tailscale name and the door's
   port, for example `home-pc:7371` (or its `100.x.y.z` address).
3. On an Android phone, keep Tailscale on: in Android's Settings › VPN, tap
   the gear beside Tailscale and turn on **Always-on VPN**, and set
   Tailscale's battery use to **Unrestricted**.

If a device can't connect, check that the hub computer's firewall lets the
port in (on Windows, allow the hub if Windows asks).

### The open internet: only the relay-only door

Without Tailscale, open only the relay-only door to the internet, never the
main port 7370. Anyone who reaches the door can register an address and send
mail, but can read only their own.

Put a tunnel or reverse proxy that gives you an `https://` address in front of
it, and add that address in Hubchat. A plain port forward sends every message
and secret unencrypted.

The hub's own README has the full instructions:
[Connect Hubchat](https://github.com/Maurdekye/orgtree-mailhub#connect-hubchat).

### Who can see what

The hub's operator can read the messages that pass through it, and everyone on
a hub is listed in its directory. Only use hubs run by people you trust.
Anyone who has your recovery words can read your messages and send as you, so
keep them private.

## Building from source

You need:

- Node.js 24 and npm
- Rust (stable) and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
  (on Windows: the Visual Studio C++ build tools and WebView2)
- For Android:
  - JDK 21
  - the Android SDK (platform 35, build-tools 35.0.0) and NDK 27
  - `rustup target add aarch64-linux-android`

Then run `npm ci`, and:

| What | How |
|---|---|
| The UI in a browser, against an in-memory mock | `npm run dev`, then open http://localhost:1420 (add `?platform=android` for the phone layout) |
| The desktop app, live-reloading | `npm run tauri dev` |
| A desktop test build ("Hubchat Test", kept apart from an installed Hubchat) | `npx tauri build --debug --no-bundle --config src-tauri/tauri.test.conf.json` |
| The Windows installer | `npm run tauri build` |
| An Android APK | `scripts/android-build.sh debug` or `scripts/android-build.sh release` |
| Unit tests | `cargo test -p hubchat-core` |

Notes:

- **Windows installer:** release builds also make signed update files, which
  need the release key in `TAURI_SIGNING_PRIVATE_KEY`. That key isn't in the
  repo. To build without it, create a file such as `no-updater.json`
  containing `{ "bundle": { "createUpdaterArtifacts": false } }` and run
  `npm run tauri build -- --config no-updater.json`.
- **Android script:** on Windows without Developer Mode, `tauri android build`
  can't create a symlink it needs. `scripts/android-build.sh` (run it in Git
  Bash) works around that. It builds for arm64.
  - `HUBCHAT_ENV`: optionally, a script that sets `JAVA_HOME`, `ANDROID_HOME`
    and `NDK_HOME`.
  - `HUBCHAT_ANDROID_KEYSTORE`: the path to a `keystore.properties` kept
    outside the repo (`storeFile`, `storePassword`, `keyAlias`,
    `keyPassword`). It signs release builds; without it a release APK is
    unsigned.
  - `HUBCHAT_TEST_BUILD=1`: makes "Hubchat Test" (`dev.orgtree.hubchat.test`),
    which installs beside the real app.
- **Tests against a real hub:** these skip unless you point them at one:
  - `MAILHUB_DIR`: the folder of the v1 Python hub (orgtree's
    `engine/mailhub`)
  - `HUBCHAT_V2_HUB`: the address of a scratch mail hub v2.0

  The scripts in `tools/` drive the real apps on a PC and a phone; each one's
  header says what it needs.

## Releasing

GitHub Actions builds, signs and drafts every release; nobody builds one on
their own PC. [docs/ci-release.md](docs/ci-release.md) has the whole
procedure, including checking a draft and rolling a release back. In short:

1. **Bump the version** in five places, in one commit:
   - the workspace `Cargo.toml` (plus the `hubchat` and `hubchat-core`
     entries in `Cargo.lock`);
   - `package.json` and `package-lock.json`;
   - `src-tauri/tauri.conf.json`;
   - the mock's version in `src/lib/native.ts`.
2. **Add the notes** in the same commit: `docs/release-notes-<version>.md`
   (the release's text) and `docs/update-notes-<version>.txt` (one or two
   sentences the apps show when they offer the update).
3. **Tag that commit** `v<version>` and push the tag. The `release` workflow
   builds every platform, signs everything and leaves a draft release.
4. **Check the draft, then publish it.**

Each published release is also the update feed for the apps. Its
`latest.json` has one entry per platform, `android-aarch64` (the APK) among
them, and every entry's signature names the version: an app refuses an update
whose signature names a different version. The APK is attached a second time
as `Hubchat-android.apk`, a name that never changes
(`releases/latest/download/Hubchat-android.apk`).

## How it's built

- `crates/hubchat-core`: the Rust core. It holds the mail hub client, the
  sync engine, the message store and the identity.
- `src-tauri`: the Tauri 2 shell. It provides the commands the UI calls, the
  tray, notifications and updates, plus Android's background connection
  (`gen/android`).
- `src`: the React and TypeScript UI, shared by desktop and Android.
  `src/lib/mock.ts` stands in for the core when it runs in a browser.
- `tools`: device test drivers.
- `design`: the icon sources.

## License

MIT. See [LICENSE](LICENSE).
