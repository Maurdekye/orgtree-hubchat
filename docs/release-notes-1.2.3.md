Hubchat 1.2.3 fixes the first message in a new conversation staying hidden and adds a Windows taskbar dot for unread messages and unanswered questions.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.3_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.3_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.3_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.3_amd64.AppImage`, `Hubchat_1.2.3_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.3_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## New conversations

- **Your first message now appears after you send it.** It stays visible when the conversation syncs, on desktop and Android.
- **The new-chat intro disappears as soon as you send your first message**, without waiting for the other person to reply. It still appears in a genuinely empty conversation.

## Windows taskbar attention

- **A dot on Hubchat's taskbar icon shows when something needs your attention:** unread messages or incoming questions you have not answered.
- Reading messages clears their unread notice. A read question keeps the dot until you answer or skip it; the dot clears when nothing needs attention.
- The dot updates while Hubchat is running, including when its window is hidden in the tray.

## Notes

- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.3/README.md) for platform setup.

The code is MIT-licensed.
