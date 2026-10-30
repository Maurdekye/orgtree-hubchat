Hubchat 1.2.6 shows who a mention is, lets you open your own Orgtree orgs from Contact info, sorts the mentions menu the way Orgtree does, and makes menus, settings and the composer behave the same as their Orgtree counterparts.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.6_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.6_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.6_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.6_amd64.AppImage`, `Hubchat_1.2.6_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.6_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Mentions

- **A mention in a message now shows who it is.** When one of your hubs lists the person, the mention shows their name with their picture or letters, as in your chat list, and opens your chat with them. Copying the text still gives the mention exactly as written.
- Hovering a mention shows the same details as Orgtree: online or offline, what kind of contact it is, their name, description and address, and **Open chat**.
- **Someone none of your hubs lists gets a small grey dot** and a tooltip saying so. That mention is muted and opens nothing; on Android a tap explains why. Its menu offers **Copy address** only.
- **The mentions menu is sorted like Orgtree's.** People already mentioned in the open chat come first, then everyone else by your most recent chat or mention with them. Typing only narrows the list and never reorders it. The previous ranking by closest name match and by who is online has been removed.
- **The menu opens at the bottom.** The best match sits right above the message box, highlighted, and the menu stays scrolled there as you type. Up moves away from the box and Down moves back toward it; the highlighted name stays in view, and the keys stop at the ends instead of wrapping around. The menu is no longer limited to eight entries: more names load as you scroll or arrow up, without the list jumping.
- Right-clicking an address in a message now also offers **Contact info**, the same menu as the chat list.

## Orgtree orgs on this PC

- On Windows, when Orgtree is installed on the same computer, Contact info shows the **Orgtree org** badge of that computer's own orgs as a link. Click it to open the org in Orgtree; Orgtree starts if it isn't running. If Orgtree can't be opened, Hubchat says so.
- Orgs on other computers, the conversation header and Android keep a plain badge.

## Consistent with Orgtree

- **Settings > General > Enter key** (desktop) lets you choose whether Enter sends a message or inserts a new line. With **Insert new line**, Ctrl+Enter (Command+Enter on macOS) sends. The hint under the message box and the Send button follow your choice.
- **Pasting a file of any kind** into the message box now adds it as an attachment under its own name, not only pictures. Text on the clipboard still pastes as text. Pasted files larger than 64 MiB are refused; use the file picker or drag and drop for those.
- **New chat** shows contacts the same way as the Directory: their description, a chip for each hub, presence and, on desktop, a right-click menu with Message, Contact info and Copy address.
- **Transfer history in Settings** shows who each finished transfer was sent to or received from; click the name to open that chat. More history loads as you scroll toward the end, and **Retry** repeats only the part that failed, keeping what is already shown.
- **Files in Contact info** says when it is still loading, explains why it couldn't read the files with a **Retry** button, and says "No files yet" only once it knows.
- **Settings**: the desktop section list works with the arrow keys, Home and End. An unsaved Profile name or About now survives switching to another section and back. On/off settings toggle when you click their label and say "on" or "off".
- **Hovers and keyboard access:** long chat names, directory addresses and descriptions, and attached file names show their full text on hover. A message's time shows the full date, time and time zone on hover. Unread badges are read out as "N unread". Active transfer rows, quoted messages and the Android profile card can be opened from the keyboard, and the Android theme choices work as proper radio buttons. The header picture says Contact info on hover, and the collapsed hub button announces the hub status.
- The system theme option is called **Match system** on both desktop and Android.

## Notes

- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.6/README.md) for platform setup.

The code is MIT-licensed.
