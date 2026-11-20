Hubchat 1.3.10 adds a Linux build for 64-bit ARM (such as a Steam Frame), and Orgtree links now open on Linux.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.10_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.10_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.10_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.10_amd64.AppImage`, `Hubchat_1.3.10_amd64.deb` | **Prototype** for 64-bit Linux. |
| `Hubchat_1.3.10_aarch64.AppImage`, `Hubchat_1.3.10_arm64.deb` | **Prototype** for 64-bit ARM Linux, such as SteamOS on a Steam Frame. |
| `hubchat-web-1.3.10-docker-amd64.tar.gz` | The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.3.10` and `:latest`. |
| `hubchat-web-1.3.10-static.tar.gz` | The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.10_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Linux on ARM

- **A build for 64-bit ARM.** Hubchat now ships an AppImage and a .deb for 64-bit ARM Linux, and they update themselves like the other desktop builds.
- **Without FUSE.** On a system without FUSE, such as SteamOS on a Steam Frame, start the AppImage with `APPIMAGE_EXTRACT_AND_RUN=1` in its environment. In Steam, set the launch options to `APPIMAGE_EXTRACT_AND_RUN=1 %command%`.

## Orgtree links on Linux

- **Links open on Linux.** With Orgtree 4.6.6 or later on the same computer, Orgtree mentions in messages light up and open in Orgtree, and Contact info's Orgtree org badge opens the org. Before, they always showed grey on Linux.
