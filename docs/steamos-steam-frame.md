# SteamOS and Steam Frame

This guide covers installing and running **Hubchat** on SteamOS systems without FUSE, including the **Steam Frame** (`aarch64` SteamOS Holo 0.6.0, SteamVR Gamescope session). Run `uname -m` to pick the matching architecture: the Steam Frame is `aarch64`, while the Steam Deck is `x86_64` (untested on Deck; the AppImage extract-and-run and Gamescope steps apply where the same environment exists).

The official `aarch64` AppImage bundles WebKitGTK, so the older bare native build with a bubblewrap `/usr/lib` overlay is no longer needed. Linux builds remain prototypes.

## What you still do by hand

1. **Download and verify the AppImage.**
   Get the Linux AppImage for your architecture from [Hubchat releases](https://github.com/Maurdekye/orgtree-hubchat/releases/latest) (`Hubchat_<version>_aarch64.AppImage` on Steam Frame, `Hubchat_<version>_amd64.AppImage` on `x86_64`) and check its SHA-256 digest against `SHA256SUMS.txt` from the same release:

   ```sh
   hubchat_download="$HOME/Downloads/Hubchat_1.3.11_aarch64.AppImage"
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
   Start Hubchat once with `APPIMAGE_EXTRACT_AND_RUN=1` in its environment (do **not** use `--appimage-extract-and-run`, which is stripped on updater relaunch):

   ```sh
   APPIMAGE_EXTRACT_AND_RUN=1 "$HOME/.local/opt/hubchat/hubchat"
   ```

   If launching from Steam instead of a terminal, add the AppImage as a non-Steam game and set its launch options to `APPIMAGE_EXTRACT_AND_RUN=1 %command%`.

## What Hubchat automates after first launch

- **App-list entry and icon:** in AppImage / extract mode (or when `HUBCHAT_LAUNCHER` is exported), Hubchat creates `~/.local/share/applications/dev.orgtree.hubchat.desktop` and installs its icon (`~/.local/share/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png`) if none exists. Generated entries include `env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1` when running extracted and use a bare unquoted path when plain. Hubchat marks entries it wrote with `X-Hubchat-Version=` and only updates or repairs those; any hand-written `dev.orgtree.hubchat.desktop` or custom icon is left untouched.
- **`hubchat://` link handler:** writes `~/.local/share/applications/dev.orgtree.hubchat.link.desktop` and registers `x-scheme-handler/hubchat` in `~/.config/mimeapps.list` when no other default is set (see [Linux scheme handler](linux-scheme-handler.md)). If you run Hubchat through a custom wrapper that exports `HUBCHAT_LAUNCHER`, both generated entries honor that launcher.
- **Gamescope graphics workaround:** when `XDG_CURRENT_DESKTOP=gamescope`, Hubchat re-execs itself once before GTK/WebKit or single-instance setup runs with `DISABLE_GAMESCOPE_WSI=1` plus `GDK_BACKEND=x11` (unsetting `EGL_PLATFORM`) when both X11 (`DISPLAY`) and Wayland (`WAYLAND_DISPLAY`) exist, or `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland alone. Outside `XDG_CURRENT_DESKTOP=gamescope`, or when those variables are already set (for example by a custom launcher), Hubchat does not re-exec.
- **Identity key storage without a Secret Service:** see the security boundary below.

After the first launch, you can open **Hubchat** directly from the SteamVR overlay or desktop app list.

## Identity key storage without a Secret Service

On standard Linux desktops, Hubchat stores your identity key in the desktop Secret Service keyring (`org.freedesktop.secrets`, such as KWallet or GNOME Keyring). When a Secret Service is running or activatable on the session D-Bus, Hubchat uses it **exclusively** — an empty, locked, denied, timed-out, or erroring keyring never falls back to a file.

In sessions where Hubchat confirms no Secret Service is owned or activatable (or there is no session D-Bus at all), such as the stock SteamOS VR Gamescope session:

- Hubchat stores the identity key in `~/.local/share/dev.orgtree.hubchat/identity-store/identity.key` with no passphrase, inside an effective-user-owned `0700` directory with `0600` file permissions, atomic replacement, and symlink/hard-link/non-regular/foreign-`HOME` checks.
- **Security boundary (same-account access):** file permissions protect the key from other OS user accounts, **not** from other processes running under your own user account — anything running as your user can read that file.
- Hubchat shows this state in **Settings › Privacy** (**Identity key storage**) and on **Settings › Devices** ("this device"). Keep your recovery words saved in **Settings › Recovery words**.

### What happens if a Secret Service appears later

If a Secret Service later becomes available (for example, booting into SteamOS Desktop Mode where KWallet runs, or starting a private `ksecretd`):

- Hubchat uses the Secret Service exclusively and does **not** silently read or auto-migrate the permissions-only key file.
- Hubchat keeps your local chats intact and shows the lost-key screen.
  - **Restoring or linking your identity** (recovery words, Link a device, key QR, or key file) saves the key into the Secret Service and removes the old permissions-only file.
  - **Starting over** or **signing out** removes both the keyring entry and the permissions-only file.
  - **Closing Hubchat without making changes** leaves the permissions-only file untouched so VR mode continues working.

### Optional private KWallet wrapper

If you prefer passphrase-encrypted storage in VR mode rather than the permissions-only file, you can launch Hubchat from a custom wrapper script that starts an isolated `ksecretd` (providing `org.freedesktop.secrets`) before executing the AppImage and exports `HUBCHAT_LAUNCHER=/path/to/your/launcher`. While that provider is present on the session bus, Hubchat uses it exclusively.

## Checking cross-app links

When pairing Hubchat with Orgtree on the same machine, verify that both URI defaults are set and valid:

```sh
xdg-mime query default x-scheme-handler/hubchat
xdg-mime query default x-scheme-handler/orgtree
desktop-file-validate \
  "$HOME/.local/share/applications/dev.orgtree.hubchat.desktop" \
  "$HOME/.local/share/applications/dev.orgtree.hubchat.link.desktop"
```
