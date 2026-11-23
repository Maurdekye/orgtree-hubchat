# Releasing Hubchat with GitHub Actions

Hubchat's published builds come from GitHub Actions, not from anyone's PC. Pushing a
version tag builds every platform on GitHub's free runners and leaves a **draft**
release. A person checks the draft and publishes it. Nothing is ever published
automatically.

| Platform | Workflow | Runner | Release assets |
|---|---|---|---|
| Windows | `build-windows.yml` | windows-2025 | `Hubchat_<v>_x64-setup.exe` (+ `.sig`) |
| macOS (Apple Silicon and Intel) | `build-macos.yml` | macos-15 | `Hubchat_<v>_universal.dmg`, `Hubchat_<v>_universal.app.tar.gz` (+ `.sig`) |
| Linux (x86_64) | `build-linux.yml` | ubuntu-22.04 | `Hubchat_<v>_amd64.AppImage` (+ `.sig`), `Hubchat_<v>_amd64.deb` (+ `.sig`) |
| Linux (arm64) | `build-linux.yml` | ubuntu-22.04-arm | `Hubchat_<v>_aarch64.AppImage` (+ `.sig`), `Hubchat_<v>_arm64.deb` (+ `.sig`) |
| Android (arm64) | `build-android.yml` | ubuntu-24.04 | `Hubchat_<v>_arm64.apk` (+ `.sig`) and the same file as `Hubchat-android.apk` |
| Hubchat Web | `build-web.yml` | ubuntu-24.04 | `hubchat-web-<v>-static.tar.gz`, `hubchat-web-<v>-docker-amd64.tar.gz`, `build-info-web.json` |
| all | `release.yml` | ubuntu-24.04 | `latest.json` (the updater feed), `SHA256SUMS.txt` |

`release.yml` runs the four builds and the Hubchat Web build
([below](#hubchat-web)), then:

1. **sign**: the only job that ever sees the real keys. It runs in the protected
   `release` environment and executes no project code; it installs the pinned Tauri
   CLI with `npm ci --ignore-scripts`, and uses apksigner from the Android SDK.
   - It signs the APK with the release keystore, v2 scheme only, like every
     published APK.
   - It then signs every updater file with the Tauri updater key, binding the
     version (`--app-version`, because `tauri.conf.json` sets `requireSignedVersion`).
     The APK is one of them, signed after apksigner, so its `.sig` covers the APK as
     shipped.
   - The builds themselves only ever hold throwaway keys.
2. **stage**: writes `latest.json` from the platforms' updater entries, and
   `SHA256SUMS.txt`. It refuses a release without its three Hubchat Web files.
3. **draft**: `gh release create --draft` with every asset.

## Cutting a release

1. Bump the release version inputs in one commit:
   - `Cargo.toml` (`[workspace.package] version`) and the `hubchat`,
     `hubchat-core`, `hubchat-web` and `hubchat-web-server` entries in
     `Cargo.lock` (the Hubchat Web builds run `cargo --locked`);
   - `package.json` and both version fields of `package-lock.json`;
   - `src-tauri/tauri.conf.json`;
   - the browser mock's version in `src/lib/native.ts`.
   - Android's `bundle.android.versionCode` in `src-tauri/tauri.conf.json`
     (`major * 1000000 + minor * 1000 + patch`; 1.3.13 uses `1003013`) and the
     matching versionCode / versionName fallbacks in
     `src-tauri/gen/android/app/build.gradle.kts`.
     Tauri regenerates the ignored `app/tauri.properties` during an Android
     build; do not edit or commit that generated file as release source.
2. In the same commit, add the two notes files:
   - `docs/release-notes-<v>.md`: the GitHub release's text;
   - `docs/update-notes-<v>.txt`: one or two sentences the app shows when it offers
     the update (`latest.json`'s `notes`).
3. Push that commit to `main`, then tag it and push the tag:
   ```
   git tag v<v> <commit>
   git push origin refs/tags/v<v>
   ```
4. The **release** workflow starts. It refuses at once if the tag differs from any
   version above, if either notes file is missing, or if `Cargo.lock` is incomplete.
5. When the four builds finish, the `sign` job signs everything with the real keys
   from the `release` environment (see [Secrets](#secrets-and-the-release-environment)).
   It doesn't wait for an approval: pushing the tag is the decision to release.
6. A draft release `v<v>` appears with every asset. The run's summary lists each
   file's size and SHA-256.

The pipeline is the only way a release is made. Don't also run the old local publish
script for the tag: the workflow would then fail at the draft step, because the
release already exists.

## Checking and publishing the draft

```
gh release view v<v> -R Maurdekye/orgtree-hubchat
gh release download v<v> -R Maurdekye/orgtree-hubchat -D check-v<v>
cd check-v<v> && sha256sum -c SHA256SUMS.txt
```

- `latest.json` has one entry per platform: `windows-x86_64`, `darwin-aarch64`,
  `darwin-x86_64`, `linux-x86_64`, `linux-x86_64-deb`, `linux-aarch64`,
  `linux-aarch64-deb` and `android-aarch64` (the versioned APK; desktop updaters ignore it). Each URL points at this release, and
  each signature equals the matching `.sig` file.
- Install the Windows setup and the APK on a test machine or phone if the release
  changes anything risky.

Publish it, which makes it the release every installed Hubchat updates from:

```
gh release edit v<v> -R Maurdekye/orgtree-hubchat --draft=false --latest
```

Publishing also starts **publish-web-image**, which pushes the release's
Hubchat Web image to the registry ([below](#hubchat-web)).

Publishing starts the **verify-release** workflow. It downloads every public asset as an installed app would and checks it against `SHA256SUMS.txt`, the updater feed and the tag, and it checks that GitHub's "latest" release is the right one. A red run means: roll back (below). To check a release again: `gh workflow run verify-release.yml -f tag=<tag>`.

## Rolling back

- **Before publishing**: delete the draft and the tag, fix, and tag again.
  ```
  gh release delete v<v> -R Maurdekye/orgtree-hubchat --yes
  git push origin :refs/tags/v<v>
  ```
- **After publishing**: installed apps read
  `releases/latest/download/latest.json`. Marking the previous release as latest
  stops new update offers at once:
  ```
  gh release edit v<previous> -R Maurdekye/orgtree-hubchat --latest
  ```
  Apps that already updated stay on the bad version; the updater only moves
  forward. The fix for them is a new patch release.
- Never replace an asset of a published release: its `.sig`, `latest.json` and
  `SHA256SUMS.txt` would no longer match it.
- **Hubchat Web's image**: marking the previous release as latest does not move
  the registry's `latest` tag. Push the previous image again:
  `gh workflow run publish-web-image.yml -R Maurdekye/orgtree-hubchat -f
  tag=v<previous>` (when the previous release has a Hubchat Web image). People
  with the web app open are offered whatever build their server serves.

## Test runs

A push to the branch `ci-test/hubchat-release` runs the whole pipeline as a test.
Throwaway keys stand in for the real ones, and no release is created; the staged
assets are the run's artifact. Each `build-<platform>.yml` also runs on its own
from a push to `ci-test/hubchat-<platform>` (`ci-test/hubchat-web` for
`build-web.yml`). Once the workflows are on `main`, a
manual run can test any tag or commit:

```
gh workflow run release.yml -R Maurdekye/orgtree-hubchat -f ref=v<v>
```

A manual run is always a test, even one started from a tag: only a pushed tag
makes a release.

Throwaway-signed files can't update or install over a real Hubchat. Never publish
them.

`device-trust.yml` is a check, not a build. A push to `ci-test/hubchat-trust` (or a
manual run) proves on Windows, macOS and Linux runners that the apps accept a hub
whose certificate is from a certificate authority installed on the machine
(`crates/hubchat-core/src/tls.rs`). It runs the trust tests, then two tests that read
the runner's REAL certificate store: one before the throwaway test CA
(`crates/hubchat-core/tests/tls/ca.pem`) is installed in that store, which must be
refused, and one after, which must connect. It installs a CA, so it belongs on
runners that are thrown away, never on a machine someone uses. The same check for
Android runs on an emulator: `tools/e2e-android-trust.mjs`.

The first full test (run 37927908831, 2026-10-09, building v0.1.2) took about 18
minutes from a cold cache. The macOS build is the slowest.

## Secrets and the release environment

The real keys live only in the GitHub Environment `release`, as environment
secrets. Only the `sign` job uses that environment.

| Secret | Holds |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the updater private key (the contents of the `.key` file) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password; leave it unset, the current key has none |
| `ANDROID_KEYSTORE_B64` | the release keystore (PKCS#12), base64-encoded |
| `ANDROID_KEYSTORE_PASSWORD` | the keystore's password |
| `ANDROID_KEY_ALIAS` | the key's alias in the keystore |
| `ANDROID_KEY_PASSWORD` | the key's password |

How the environment is protected (Settings › Environments › release; set up
2026-10-09):
- **Deployment branches and tags**: selected tags only, `v*`. A workflow on any
  other branch or tag can't read the keys, test branches included.
- **No required reviewers** (the maintainer's choice): a pushed `v*` tag signs at
  once. Only people who can push tags can start a signed build, and its output is
  still only a draft. To require a click before every signing, add a reviewer there.

If a secret is missing, `sign` stops with an error naming it; nothing is drafted.
The keys never leave the `release` environment: the build jobs, test runs and pull
requests can't read them.

## If GitHub disappears

Everything here also works on your own machines; nothing depends on GitHub except
hosting the files. With the same versions as the pins below:

- **Windows**: `npm ci`, then
  `TAURI_SIGNING_PRIVATE_KEY="$(cat hubchat-updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npx tauri build --bundles nsis`.
- **macOS** (on a Mac): `npm ci`, `rustup target add aarch64-apple-darwin x86_64-apple-darwin`, then
  `APPLE_SIGNING_IDENTITY=- npx tauri build --target universal-apple-darwin --bundles app,dmg`
  with the same key variables.
- **Linux** (Ubuntu 22.04): install the packages listed in `build-linux.yml`, `npm ci`, then
  `npx tauri build --bundles appimage,deb` with the key variables.
- **Android**:
  - Linux: `npx tauri android build --target aarch64 --apk --split-per-abi` (unsigned),
    then the same `apksigner sign` command as `.github/ci/release/sign.sh`.
  - Windows: `scripts/android-build.sh release` with `HUBCHAT_ANDROID_KEYSTORE`
    pointing at a keystore.properties file.
  - Either way, `npx tauri signer sign --app-version <v>` over the signed APK
    makes its `.sig`.
- Then `latest.json` and `SHA256SUMS.txt`: put each platform's `updater-*.json` in
  the folder (the build workflows show their shape) and run
  `MODE=release VERSION=<v> node .github/ci/release/assemble.mjs <folder>` from a
  checkout of the tag.

## Toolchain pins

These are the versions the hand-made 0.1.x releases were built with. The CI uses
them exactly:

| Tool | Version | Where it's pinned |
|---|---|---|
| Rust | nightly-2025-12-12 (rustc 1.94.0-nightly f52090008) | each `build-*.yml` |
| Node / npm | 24.12.0 / 11.6.2 | each `build-*.yml` |
| Tauri CLI and crate | 2.12.1 | `package-lock.json`, `Cargo.lock` |
| JDK | Temurin 21 | `build-android.yml` |
| Android NDK / build-tools | 27.3.13750724 / 35.0.0 (apksigner) | `build-android.yml`, `sign.sh` |
| Android platform | android-37.0 | `build-android.yml` |
| GitHub Actions | official `actions/*` only, pinned by commit SHA | every workflow |

Runner images are pinned by name (`windows-2025`, `macos-15`, `ubuntu-22.04`,
`ubuntu-24.04`). GitHub updates their contents weekly; the toolchains above are
installed explicitly, so those updates don't change what we build with.

## How CI builds compare with the local builds

The first CI builds were checked against the published v0.1.1, which was built by
hand. These are measured comparisons, not byte-identical rebuilds:

- **Windows**: the installer has the same 7 files. 6 are byte-identical, including
  every NSIS plugin. `hubchat.exe` differs only in embedded build paths and the PE
  timestamp. Both builds used MSVC 14.51.36231.
- **Android**: the same badging (package, versionCode 1001, SDK levels), the same
  signature schemes (v2 only), and the same 16 KB alignment. The native library
  differs only in embedded build paths. The CI APK also carries
  `assets/tauri.conf.json`, which a normal Tauri build writes; the Windows
  shortcut script stopped before writing it.

The CI binaries don't embed the builder's account name or folder layout, which the
hand-made ones did.

## Installing

- **Windows**: the installer isn't code-signed. If SmartScreen warns, choose
  **More info › Run anyway**.
- **macOS**: the app isn't signed by Apple or notarized (that needs a paid Apple
  developer account), so macOS blocks the first open.
  - On macOS 15 or later: try to open Hubchat once, then go to
    **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier: right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`.
  - After an update, macOS may ask once more whether Hubchat may use its Keychain
    entry; choose **Always Allow**.
- **Linux (Ubuntu)**:
  - AppImage: `chmod +x Hubchat_<v>_amd64.AppImage` and run it. It needs FUSE 2
    (`sudo apt install libfuse2` on 22.04, `libfuse2t64` on 24.04 and later). It
    updates itself. The arm64 AppImage is `Hubchat_<v>_aarch64.AppImage`. On a
    system without FUSE, such as SteamOS on a Steam Frame, start it with
    `APPIMAGE_EXTRACT_AND_RUN=1` in its environment. In Steam, set the launch
    options to `APPIMAGE_EXTRACT_AND_RUN=1 %command%`.
  - .deb: `sudo apt install ./Hubchat_<v>_amd64.deb`. Updates ask for your
    password, because they reinstall the package.
  - Hubchat keeps its identity key in the desktop keyring (GNOME Keyring or
    KWallet) whenever a Secret Service is present or activatable. In a session
    with no Secret Service (such as a SteamOS / Steam Frame VR session), it
    falls back to a permissions-only key file
    (`~/.local/share/dev.orgtree.hubchat/identity-store/identity.key`, `0700`
    directory / `0600` file, no passphrase; see `docs/steamos-steam-frame.md`).
- **Android**: allow your browser or file manager to install apps when Android
  asks.

## Hubchat Web

Hubchat Web (the web app for an iPhone or any browser, and the small server
that hands it out: `docs/hubchat-web-server.md`) ships with every release
and has Hubchat's version. It is not an installed app: it has no updater
entry in `latest.json` and nothing of it is signed.

**`build-web.yml`** (job `web` of `release.yml`; on its own from a push to
`ci-test/hubchat-web`), on one ubuntu-24.04 runner:

1. Runs the server's own tests (`cargo test -p hubchat-web-server`).
2. Builds the web app with `tools/web-build.mjs`: the core to WebAssembly
   (Rust target `wasm32-unknown-unknown`, `clang` for SQLite), the
   `wasm-bindgen` command at exactly the version `Cargo.lock` pins, the
   page, and the size budget (a first visit downloads at most 3 MB).
3. Runs `.github/ci/web/package.sh`, which
   - packs the page as `hubchat-web-<v>-static.tar.gz`, reproducibly (sorted,
     no times, no owners);
   - builds the server program with the page inside it, static, in the
     official `rust:1.89-alpine` image (`--locked`);
   - runs `.github/ci/web/smoke.sh` on the program;
   - builds the image from that same program
     (`crates/hubchat-web-server/Dockerfile`: FROM scratch, one file), runs
     the smoke on the image, and checks the image holds that program;
   - saves the image as `hubchat-web-<v>-docker-amd64.tar.gz` (`docker load`
     gives `hubchat-web:<v>`);
   - writes `build-info-web.json`: the version, the commit, both files'
     SHA-256, every page file's SHA-256, the program's SHA-256 and size, and
     the image's id.

The three files travel with the other platforms' files through `sign`
(untouched) into `stage`, which refuses the release unless all three are
there and `build-info-web.json` records exactly those two files; they get
their lines in `SHA256SUMS.txt` like every asset. `verify-release` checks
them again once published.

**`publish-web-image.yml`** runs when a release is published (never for a
draft). It downloads the release's own image file, checks it against
`SHA256SUMS.txt` and `build-info-web.json`, loads it, and pushes it with the
run's own token as `ghcr.io/maurdekye/hubchat-web:<v>`, and as `:latest` for
a stable release only. Then it pulls each tag back and checks that the
program inside is the recorded one. What is pushed is the file the release
carries, never a rebuild. A release from before Hubchat Web has no image:
the workflow says so and does nothing. To run it again for a published
release:

```
gh workflow run publish-web-image.yml -R Maurdekye/orgtree-hubchat -f tag=v<v>
```

The package page's description comes from the image's labels
(`org.opencontainers.image.description`, `source`, `url`, `documentation`,
`licenses`; `package.sh` refuses an image without them). GHCR reads labels
only for a one-platform image, which is what is pushed, and the workflow
checks that each pushed tag is a single image manifest. If an arm64 image
is ever added, the description must move to annotations on an OCI index:
push the per-platform images and the index in OCI format with index
annotations, or the page is blank (the mail hub's package showed this on
2026-11-11).

The first push creates the package `hubchat-web` as **private**. Its owner
makes it public once: on GitHub, the package's page, **Package settings**,
**Change visibility**, Public. Until then `docker pull` needs a login.

Checking a draft's Hubchat Web by hand:

```
docker load -i hubchat-web-<v>-docker-amd64.tar.gz
# prints: hubchat-web <v>
docker run --rm hubchat-web:<v> version
bash .github/ci/web/smoke.sh image hubchat-web:<v> <v> smoke-logs
```

Not in the workflow: an arm64 image, and a browser opening the app from
the image (`tools/web-e2e.mjs --server` does that locally; it needs a mail
hub and PostgreSQL).
