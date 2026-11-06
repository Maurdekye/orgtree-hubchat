Hubchat 1.3.2 makes sending faster: a message you send shows in the chat at once, and sending no longer slows down as a chat's history grows.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.2_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.2_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.2_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.2_amd64.AppImage`, `Hubchat_1.3.2_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.2_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Sending

- **Your message shows at once.** When you press Send, the message appears in the chat straight away and the message box empties, while Hubchat hands it to the mail hub in the background.
- **A failed send stays in the chat** with **Retry** and **Discard**. Discard puts its text, reply and image back in the message box, unless you have already started typing something newer.
- **Sending no longer slows down in long chats.** After a send, Hubchat re-reads only the newest messages of the chat instead of everything loaded, so a long history no longer makes each send take longer.

## Notes

- Nothing else changed since 1.3.1. Translation still needs Mail hub 2.4.0; see the Hubchat 1.3.0 notes.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.2/README.md) for platform setup.

The code is MIT-licensed.
