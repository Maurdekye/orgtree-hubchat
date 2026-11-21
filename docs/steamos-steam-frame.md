# SteamOS and Steam Frame

This guide covers installing and running **Hubchat** on SteamOS systems without FUSE, including the **Steam Frame** (`aarch64` SteamOS Holo 0.6.0, SteamVR Gamescope session). Run `uname -m` to pick the matching architecture: the Steam Frame is `aarch64`, while the Steam Deck is `x86_64` (untested on Deck; the AppImage extract-and-run and Gamescope steps apply where the same environment exists).

The official `aarch64` AppImage bundles WebKitGTK, so the older bare native build with a bubblewrap `/usr/lib` overlay is no longer needed. Linux builds remain prototypes.

## What you still do by hand

1. **Download and verify the AppImage.**
   Get **Hubchat 1.3.12 or later** for your architecture from [Hubchat releases](https://github.com/Maurdekye/orgtree-hubchat/releases/latest) (`Hubchat_<version>_aarch64.AppImage` on Steam Frame, `Hubchat_<version>_amd64.AppImage` on `x86_64`) and check its SHA-256 digest against `SHA256SUMS.txt` from the same release:

   ```sh
   hubchat_download="$HOME/Downloads/Hubchat_1.3.12_aarch64.AppImage"
   file "$hubchat_download"
   sha256sum "$hubchat_download"
   ```

2. **Install it at a fixed, writable, space-free path.**
   In-place updates overwrite the AppImage at its original path, and Gamescope's `xdg-open` needs a plain path without spaces or shell metacharacters. If replacing an existing file by hand, quit Hubchat first and keep a backup.

   ```sh
   mkdir -p "$HOME/.local/opt/hubchat"
   install -m 755 "$hubchat_download" "$HOME/.local/opt/hubchat/hubchat"
   ```

3. **Run the first launch in extract-and-run mode.**
   Start Hubchat once with `APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1` in its environment (do **not** use `--appimage-extract-and-run`, which is stripped on updater relaunch; `NO_CLEANUP=1` prevents a second launch from deleting the live instance's extraction folder):

   ```sh
   APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 "$HOME/.local/opt/hubchat/hubchat"
   ```

   If launching from Steam instead of a terminal, add the AppImage as a non-Steam game and set its launch options to `APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 %command%`.

## What Hubchat automates after first launch

- **App-list entry and icon:** when running from its own AppImage (verified inside `$APPDIR`) or when `HUBCHAT_LAUNCHER` is exported, Hubchat creates `~/.local/share/applications/dev.orgtree.hubchat.desktop` and installs its icon (`~/.local/share/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png`) if none exists (skipping creation when a system `dev.orgtree.hubchat.desktop` already exists under `$XDG_DATA_DIRS`). Generated entries include `env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1` when running extracted (including for `HUBCHAT_LAUNCHER`) and use a bare unquoted path when plain. Hubchat publishes new files with no-replace semantics, marks entries it wrote with `X-Hubchat-Version=`, and only repairs unchanged managed entries; any hand-written `dev.orgtree.hubchat.desktop` or custom icon is left untouched.
- **`hubchat://` link handler:** writes `~/.local/share/applications/dev.orgtree.hubchat.link.desktop` and registers `x-scheme-handler/hubchat` in `~/.config/mimeapps.list` when no other default is set (see [Linux scheme handler](linux-scheme-handler.md)). If you run Hubchat through a custom wrapper that exports `HUBCHAT_LAUNCHER`, both generated entries honor that launcher.
- **Gamescope graphics workaround:** when `XDG_CURRENT_DESKTOP=gamescope`, Hubchat re-execs itself once before GTK/WebKit or single-instance setup runs with `DISABLE_GAMESCOPE_WSI=1` plus `GDK_BACKEND=x11` (unsetting `EGL_PLATFORM`) when both X11 (`DISPLAY`) and Wayland (`WAYLAND_DISPLAY`) exist, or `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland alone. Hubchat preserves those overrides for WebKit and deliberate restarts, and restores the original launching environment when spawning external openers (browsers, Orgtree, files, or log folders). Outside `XDG_CURRENT_DESKTOP=gamescope`, or when those variables are already set (for example by a custom launcher), Hubchat does not re-exec.
- **Identity key storage without a Secret Service:** see the security boundary below.

After the first launch, you can open **Hubchat** directly from the SteamVR overlay or desktop app list.

## Identity key storage without a Secret Service

On standard Linux desktops, Hubchat stores your identity key in the desktop Secret Service keyring (`org.freedesktop.secrets`, such as KWallet or GNOME Keyring). When a Secret Service is running or activatable on the session D-Bus (or when session-bus discovery is inconclusive), Hubchat uses it **exclusively** — an empty, locked, denied, timed-out, or erroring keyring never falls back to a file.

In sessions where Hubchat confirms no Secret Service is owned or activatable (or session discovery definitively finds no session D-Bus), such as the stock SteamOS VR Gamescope session:

- Hubchat stores the identity key in `~/.local/share/dev.orgtree.hubchat/identity-store/identity.key` with no passphrase, inside an effective-user-owned `0700` directory with `0600` file permissions, atomic replacement, and symlink/hard-link/non-regular/foreign-`HOME` checks.
- **Security boundary (same-account access):** file permissions protect the key from other OS user accounts, **not** from other processes running under your own user account — anything running as your user can read that file.
- Hubchat shows this state in **Settings › Privacy** (**Identity key storage**) and on **Settings › Devices** ("this device"). Keep your recovery words saved in **Settings › Recovery words**.

### What happens if a Secret Service appears later

If a Secret Service later becomes available (for example, booting into SteamOS Desktop Mode where KWallet runs, or starting a private `ksecretd`):

- Hubchat signs in from the Secret Service exclusively and never uses or auto-migrates the permissions-only key file to sign in.
- Before starting or cleaning up any fallback file, Hubchat compares the keyring identity against this device's recorded local owner:
  - **Keyring has no key, or holds a different identity than this device's local chats:** Hubchat deletes **neither** key, keeps your local chats intact, and shows the lost-key screen so you decide.
    - **Restoring or linking your identity** (recovery words, Link a device, key QR, or key file) saves the key into the Secret Service first, commits the new owner, and then removes the old permissions-only file on a best-effort basis.
    - **Starting over** or **signing out** wipes local data and records a durable forget tombstone in the local store so an older keyring identity from any wallet cannot silently sign back in until you explicitly adopt it again (once a wallet holding the forgotten identity is reachable, Hubchat removes that entry from it).
    - **Closing Hubchat without making changes** leaves both the keyring entry and the permissions-only file untouched so VR mode continues working.
  - **Keyring holds the matching identity for this device's local chats:** Hubchat starts with the keyring and checks under an exclusive directory lock whether a leftover permissions-only file matches that same identity before removing it (never deleting a file belonging to a different identity).
- **If removing a stale permissions-only file fails:** the committed keyring save, startup, or sign-out still finishes, Hubchat notes the remaining plaintext copy in **Settings › Privacy** and **Settings › Devices**, and cleanup retries on the next start (including signed-out starts).

### Optional private KWallet wrapper

If you prefer passphrase-encrypted storage in VR mode rather than the permissions-only file, you can launch Hubchat from a custom wrapper script that starts an isolated `ksecretd` (providing `org.freedesktop.secrets`) before executing the AppImage and exports `HUBCHAT_LAUNCHER=/path/to/your/launcher`. While that provider is present on the session bus, Hubchat uses it exclusively.

## Checking cross-app links

When pairing Hubchat (**1.3.12 or later**) with Orgtree (**4.6.9 or later**) on the same machine, verify that both URI defaults are set and valid:

```sh
xdg-mime query default x-scheme-handler/hubchat
xdg-mime query default x-scheme-handler/orgtree
desktop-file-validate \
  "$HOME/.local/share/applications/dev.orgtree.hubchat.desktop" \
  "$HOME/.local/share/applications/dev.orgtree.hubchat.link.desktop"
```
