Hubchat 1.2.2 adds conversation shortcuts to the Windows taskbar and Android launcher, with contact pictures and initials. It also opens tray recent chats on the first click and fixes Android notification replies that lost their typing field or stayed in a sending state.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.2_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.2_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.2_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.2_amd64.AppImage`, `Hubchat_1.2.2_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.2_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Windows shortcuts

- **Recent chats open on the first tray click**, without waiting out the double-click interval. Double-click still shows the main window.
- **Right-click Hubchat on the taskbar** for a Recent category of open direct conversations, ordered by the latest message in either direction. Settings, Directory and New chat have their own shortcuts below the contacts.
- Contact shortcuts use profile pictures or the same initials avatars as the client. Picture changes refresh the icons, transparent pictures retain the client's avatar background, and the fixed task icons use Hubchat's dark app tile.

## Android shortcuts and notification replies

- **Long-press the app icon for recent contacts** on Android 7.1 or later, where supported by the launcher. Up to four recently messaged contacts appear, with profile pictures or initials; groups are excluded. Closing a conversation does not remove it from recent-contact history.
- Pinned contact shortcuts remain usable when a contact leaves the recent four. Shortcuts are tied to the identity that created them, so switching identities does not open a conversation under the wrong account.
- **Incoming messages no longer replace the active notification Reply action**, helping keep the typing field open. A completed reply clears the notification as it stood when you pressed Send; newer messages arriving afterward keep their notification.
- **Completed inline replies finish Android's sending feedback and dismiss the matching notification.** Unrelated queued messages no longer keep a sent reply waiting, and failures retain a visible retry or error notice instead of being mistaken for success.

## Directory

- Click or tap a contact row to open its conversation; keyboard Enter and Space also work. The redundant per-row Message button is removed.

## Notes

- After an update, the first Windows taskbar shortcut activation may only restore Hubchat. Select the shortcut again to open its target.
- Android launchers decide how many shortcuts they display and how they mask shortcut icons. Android 7.0 can run Hubchat but does not support these launcher shortcuts.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.2/README.md) for platform setup.

The code is MIT-licensed.
