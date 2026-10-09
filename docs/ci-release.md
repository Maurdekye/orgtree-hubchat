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
| Android (arm64) | `build-android.yml` | ubuntu-24.04 | `Hubchat_<v>_arm64.apk` and the same file as `Hubchat-android.apk` |
| all | `release.yml` | ubuntu-24.04 | `latest.json` (the updater feed), `SHA256SUMS.txt` |

`release.yml` runs the four builds, then:

1. **sign**: the only job that ever sees the real keys. It runs in the protected
   `release` environment and executes no project code; it installs the pinned Tauri
   CLI with `npm ci --ignore-scripts`, and uses apksigner from the Android SDK.
   - It signs the APK with the release keystore, v2 scheme only, like every
     published APK.
   - It then signs every updater file with the Tauri updater key, binding the
     version (`--app-version`, because `tauri.conf.json` sets `requireSignedVersion`).
   - The builds themselves only ever hold throwaway keys.
2. **stage**: writes `latest.json` from the platforms' updater entries, and
   `SHA256SUMS.txt`.
3. **draft**: `gh release create --draft` with every asset.

## Cutting a release

1. Bump the version in all five places, in one commit:
   - `Cargo.toml` (`[workspace.package] version`) and the `hubchat` and
     `hubchat-core` entries in `Cargo.lock`;
   - `package.json` and both version fields of `package-lock.json`;
   - `src-tauri/tauri.conf.json`;
   - the browser mock's version in `src/lib/native.ts`.
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
5. When the four builds finish, the `sign` job waits for approval of the `release`
   environment (see [Secrets](#secrets-and-the-release-environment)). Approve it on
   the run's page with **Review deployments**.
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

- `latest.json` has one entry per desktop platform: `windows-x86_64`,
  `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64` and `linux-x86_64-deb`. Each URL
  points at this release, and each signature equals the matching `.sig` file.
- Install the Windows setup and the APK on a test machine or phone if the release
  changes anything risky.

Publish it, which makes it the release every installed Hubchat updates from:

```
gh release edit v<v> -R Maurdekye/orgtree-hubchat --draft=false --latest
```

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

## Test runs

A push to the branch `ci-test/hubchat-release` runs the whole pipeline as a test.
Throwaway keys stand in for the real ones, and no release is created; the staged
assets are the run's artifact. Each `build-<platform>.yml` also runs on its own
from a push to `ci-test/hubchat-<platform>`. Once the workflows are on `main`, a
manual run can test any tag or commit:

```
gh workflow run release.yml -R Maurdekye/orgtree-hubchat -f ref=v<v>
```

Throwaway-signed files can't update or install over a real Hubchat. Never publish
them.

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

Recommended protection for the environment (Settings › Environments › release):
- **Required reviewers**: the maintainer. Each release then waits for one click
  before any key is used.
- **Deployment branches and tags**: selected tags only, `v*`. A workflow on any
  other branch or tag can't read the keys, test branches included.

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
    updates itself.
  - .deb: `sudo apt install ./Hubchat_<v>_amd64.deb`. Updates ask for your
    password, because they reinstall the package.
  - Hubchat keeps its identity key in the desktop keyring (GNOME Keyring or
    KWallet). Without a keyring, creating an identity shows an error rather than
    keeping the key only in memory.
- **Android**: allow your browser or file manager to install apps when Android
  asks.
