Hubchat 1.3.4 adds **Copy mention** to every contact and mention menu, and shows audio attachments' player right away, like video.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.4_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.4_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.4_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.4_amd64.AppImage`, `Hubchat_1.3.4_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.4_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Copy mention

- **Every menu on a contact or mention offers Copy mention**, and leads with what a left-click on it does: chat list and Directory rows, New chat rows, contact mentions and Orgtree chips in messages, and a new menu on the chat header's avatar and name. On Android it is in the conversation's ⋮ menu, and a long-press on a mention or chip in a message opens a sheet with it.
- **What is copied:** a contact's full `@net:` address, or an Orgtree mention as written (a bare agent or project name whose org runs on this PC becomes `@agent:<org>/<name>`). The old "Copy address" menu entry is now Copy mention; the copy buttons next to a shown address keep "Copy address".

## Audio attachments

- Audio files (MP3, WAV, Ogg, M4A, AAC, FLAC) show their player as soon as the message appears, paused, like videos. The **Play audio** button is gone. Nothing plays until you press play.

## Notes

- Nothing else changed since 1.3.3.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.4/README.md) for platform setup.

The code is MIT-licensed.
