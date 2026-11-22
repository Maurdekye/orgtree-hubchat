Hubchat 1.3.12 makes Hubchat easier to install and run on Linux and the Steam Frame, and uses the right wording on each system.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.12_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.12_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.12_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.12_amd64.AppImage`, `Hubchat_1.3.12_amd64.deb` | **Prototype** for 64-bit Linux. |
| `Hubchat_1.3.12_aarch64.AppImage`, `Hubchat_1.3.12_arm64.deb` | **Prototype** for 64-bit ARM Linux, such as SteamOS on a Steam Frame. |
| `hubchat-web-1.3.12-docker-amd64.tar.gz` | The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.3.12` and `:latest`. |
| `hubchat-web-1.3.12-static.tar.gz` | The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.12_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Linux and Steam Frame

- **Hubchat adds itself to the app menu.** Started as an AppImage, including in extract-and-run mode without FUSE, Hubchat installs its own menu entry and icon, so it appears in the menu and the SteamVR overlay. It checks that the entry points to its own AppImage and never shadows a system entry.
- **Works in the SteamVR (Gamescope) session.** Hubchat sets the graphics settings it needs there by itself, so it no longer needs hand-written environment variables to open.
- **Works without a keyring.** Where no system keyring (Secret Service) exists, as in the SteamVR session, Hubchat keeps your identity key in a private file only you can read. Where a keyring exists or may exist, it keeps using the keyring. If the key you use in one session is different from the one in another, Hubchat shows the lost-key screen and asks which identity to use, instead of creating a new one.
- **New guide** for installing Hubchat and Orgtree on SteamOS and the Steam Frame.

## Wording on every system

- Settings, setup and help name the system you are on, and default device names follow it. Windows looks exactly as before.
