Hubchat 1.4.0 shows when your contacts were last around, lets you block contacts, shows which contacts are in lockdown, and adds a Changelog to Settings. Blocking, lockdown and last seen need a hub on Mail hub 2.6.0 or later.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.4.0_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.4.0_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.4.0_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.4.0_amd64.AppImage`, `Hubchat_1.4.0_amd64.deb` | **Prototype** for 64-bit Linux. |
| `Hubchat_1.4.0_aarch64.AppImage`, `Hubchat_1.4.0_arm64.deb` | **Prototype** for 64-bit ARM Linux, such as SteamOS on a Steam Frame. |
| `hubchat-web-1.4.0-docker-amd64.tar.gz` | The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.4.0` and `:latest`. |
| `hubchat-web-1.4.0-static.tar.gz` | The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.4.0_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Last seen

- **See when a contact was last around**, in the chat header, the picture's tooltip, Contact info, the Directory and New chat: for example "Online · last seen 12 min ago" or "Offline · last seen yesterday at 18:02". For a person it is the last time they had Hubchat open in front of them; for an organization, the last time one of its agents in charge of its mail finished a turn, or "active now" while one is working; for an agent's own chat, the last time it sent a message.
- The time is read on the hub's clock, so a device whose clock is off doesn't shift it. A hub older than Mail hub 2.6.0 gives no time, and Hubchat shows the status word alone.

## Blocking

- **Block a contact** from the end of any contact menu. Hubchat asks first. You and that contact then can't message each other, both ways, until you unblock them; the hub enforces it.
- A blocked contact's status is a red cancel sign in the chat header, Contact info and its picture in the chat list, which also says **Blocked**. In place of the message box, the chat says the contact is blocked.
- If someone blocks you, their chat says "You have been blocked by @name and cannot message them" straight away.
- On a hub older than Mail hub 2.6.0, a block waits and takes effect once the hub is updated.

## Lockdown

- **Contacts in lockdown** (an organization that only takes mail from its own list) are marked. If you're not on the list, the contact's status is a lock, you can't message it, and its open questions have no answer buttons. If you are on the list, its status dot stays, with a small lock in the corner.
- Settings shows whether your own address is in lockdown on each hub.

## Also new

- **Settings › Changelog** lists every version's notes, newest first, with the version you're running marked. It works offline and never opens by itself. On desktop, Android and Hubchat Web.
- **Copy image** in a picture's menu puts the picture itself on the clipboard. The zoomed picture now has the same menu, and Escape or Back closes that menu before the viewer. In Hubchat Web, Copy image needs a browser that allows copying images.
- **Orgtree ticket mentions written `@ticket:`** show as ticket chips, as `@item:` mentions do.
- **Menus stay open** while new messages, typing dots, read checks or status changes arrive.
- **Long chats:** loading older messages keeps the message you're looking at where it is, and jumping far back ends with that message in view.
- **Message info**'s delivery steps no longer include a separate Fetched step.

## Fixes

- Windows: choosing a contact from the tray icon's recent-chats list no longer freezes Hubchat. The freeze had been there since 1.2.3, so 1.3.x had it too.
- **Dropping files on a chat works on Linux and macOS at any display scale**, including when Linux reports a fractional scale as 2: files land where the pointer is.
