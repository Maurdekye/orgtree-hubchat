Hubchat 1.3.13 keeps long chats fast: typing and new messages no longer slow down as a chat's history grows.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.13_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.13_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.13_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.13_amd64.AppImage`, `Hubchat_1.3.13_amd64.deb` | **Prototype** for 64-bit Linux. |
| `Hubchat_1.3.13_aarch64.AppImage`, `Hubchat_1.3.13_arm64.deb` | **Prototype** for 64-bit ARM Linux, such as SteamOS on a Steam Frame. |
| `hubchat-web-1.3.13-docker-amd64.tar.gz` | The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.3.13` and `:latest`. |
| `hubchat-web-1.3.13-static.tar.gz` | The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.13_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Long chats

- **Typing stays quick in long chats.** Typing, new messages and switching back to Hubchat no longer redraw every loaded message, so a chat with a long history responds as quickly as a short one. This was most noticeable on the Steam Frame.
- **Older history is let go.** When you scroll back down to the newest message, Hubchat keeps only the most recent messages on screen; older ones load again when you scroll up.
- **Jumping to a reply stays put.** Jumping to an earlier message no longer snaps back to the bottom of the chat.
