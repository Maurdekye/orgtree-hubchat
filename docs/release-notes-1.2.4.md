Hubchat 1.2.4 moves Transfers into Settings, makes files in Contact info actionable, and improves Windows taskbar and tray behavior.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.4_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.4_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.4_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.4_amd64.AppImage`, `Hubchat_1.2.4_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.4_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Transfers and shared files

- **Transfers has moved to Settings** on desktop and Android, clearing the chat-list header. In-progress transfers and finished history are still available there, even when nothing is running. Progress inside chats stays the same.
- **Files in Contact info can now be opened, downloaded or found in their chat.** Use a file's menu to save it or jump to its message; desktop also offers Show in folder for local files.
- File rows show download progress, retry options and unavailable files. Android presents file actions in a bottom sheet.

## Windows taskbar and tray

- **The taskbar attention dot clears earlier questions after you send a later message in the same chat**, without requiring an explicit reply to each question. Unread messages and other waiting questions still keep the dot visible.
- **The tray's recent-chat menu shows up to ten rows at a time**, with scrolling to reach the rest of your open chats. Dark mode uses dark scrollbars.
- Unread badges in the tray have enough room for large counts, including `99+`, without overlapping nearby text.

## Notes

- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.4/README.md) for platform setup.

The code is MIT-licensed.
