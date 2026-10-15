Hubchat 1.2.0 adds profile pictures, question cards, typing indicators and replies to several messages at once. It also improves hub controls, the Windows tray and title bar, and desktop keyboard navigation.

**Profile pictures, question cards and typing indicators require mail hub 2.2.0 or later.** Ordinary mail and files continue to work with older compatible hubs; those hubs do not provide the new features.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.0_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.0_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.0_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.0_amd64.AppImage`, `Hubchat_1.2.0_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.0_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

### Messages and profiles

- **Profile pictures for people and organizations.** Set a picture from Profile and position the crop square directly over the image. Eligible pictures appear in conversations and the directory; older hubs keep initials and glyphs.
- **Question cards from agents.** Choose one or several preset answers, or use Reply for text. Picking an option does not send it: press Send when ready. A question's first answer or skip settles it; later replies do not replace that answer.
- **Reply to several messages together.** Reply adds a target instead of replacing the previous one, and selection Reply merges its targets into the draft. Combine incoming and outgoing messages from the same chat, remove individual targets, and send one shared text reply. One send can include picked answers for some questions, typed replies for others, and files. Different typed answers still need separate replies.
- **Typing indicators** show when a person or agent is writing. They expire when fresh heartbeats stop, including after a lost connection, and do not create notifications.
- **Close DM without deleting it.** Hide a direct conversation on this device while keeping its history, files, draft and unread state. A new incoming message brings it back; opening the same contact restores the existing conversation.
- **Reply from Android notifications** without opening the conversation. Replies use the normal durable outgoing queue.
- Only questions show a message-kind badge; other messages read as ordinary chat. Unsent reply targets and picked answers stay with their conversation across restarts.

### Hubs, Windows and navigation

- **Enable or disable a saved hub on this device** without removing its identity, history or drafts. Messages pinned to a disabled hub wait until it is enabled again. Lost-key linking and removal/re-addition handle saved hub state more reliably.
- **A Windows tray recent-chats popup:** single-click for up to six recent open chats, double-click for the main window. The icon shows unread activity. Right-click menu placement follows the cursor, including when it is on the taskbar, and flips near screen edges.
- **Native Windows title-bar dragging**, with snapping and the standard system menu on right-click. The version label retains its tooltip, and window controls remain separate from the drag area.
- **Refresh and desktop shortcuts:** F1 Settings, F2 Directory, F3 New chat, F4 Hubs and F5 Refresh. Alt+Left/Right cycles All, Agents and People. Refresh retains the current draft and conversation position and waits for file preparation instead of discarding it.
- **Windows installer improvements:** an Upgrade option for an existing installation, preserved identity/history/settings, and the Hubchat icon on setup and uninstaller resources. Start with Windows defaults on while respecting an earlier explicit off choice. An unresponsive app may require forced closure as the last upgrade step.
- A tabbed Guide, transfers that start collapsed, local diagnostic logging and a tray entry for local crash reports.

## Fixes

- Hubs that connect during startup no longer leave the window stuck showing Connecting or an unknown hub version.
- Newly sent messages remain visible when older history is loaded. Stale navigation, identity changes and late attachment preparation no longer replace newer conversation or draft choices.
- A window hidden to the tray no longer marks messages read. An untouched desktop window stops marking new messages read after its idle period.
- More reliable picture ownership and cache updates, disabled-hub cancellation, and Windows icon handling.

## Notes

- **Hub operators:** mail hub 2.2.0 migrates the database to schema 10. Back up both database and attachment-data volumes first; a 2.1.x hub refuses the migrated database. See [Upgrading](https://github.com/Maurdekye/orgtree-mailhub/blob/v2.2.0/docs/OPERATIONS.md#upgrades).
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.0/README.md) for platform setup. Android background delivery still depends on system permissions and background-execution limits.
- Automated checks verified native icon bindings and tray geometry, but could not fully verify visible Windows installer appearance, title-bar system-menu clicks or tray placement on real scaled/multiple displays. The full installer-window capture remained black in the headless environment; it is not a passing visual check.

The code is MIT-licensed.
