Hubchat 1.3.6 gets new mail from the mail hub over a live connection when the hub offers one.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.6_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.6_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.6_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.6_amd64.AppImage`, `Hubchat_1.3.6_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.6_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Mail

- **Live new-mail connection.** Hubchat asks each hub for new mail over one WebSocket stream when the hub offers it (Mail hub 2.5.0 and later), and over the usual long poll otherwise. If the stream drops or isn't offered, Hubchat falls back to the long poll by itself and tries the stream again later. Nothing to set up, and delivery is unchanged: mail stays on the hub until Hubchat has stored it.

## Notes

- Hubchat's local message store now uses a newer SQLite library (rusqlite 0.42); existing stores keep working.
- Nothing else changed since 1.3.5.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.6/README.md) for platform setup.

The code is MIT-licensed.
