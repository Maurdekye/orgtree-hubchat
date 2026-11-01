Hubchat 1.2.7 lets you set your own status, including Do not disturb and Invisible, shows when a contact is busy or on Do not disturb, plays videos and audio right in the chat, and fixes the image viewer leaving the app zoomed in on Android.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.7_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.7_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.7_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.7_amd64.AppImage`, `Hubchat_1.2.7_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.7_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Before you upgrade

- **Status, busy, Do not disturb and Invisible need Mail hub 2.3.0.** On older hubs Hubchat works as before, and busy or Do not disturb contacts show online. Orgtree 4.5.3 bundles Mail hub 2.3.0.
- **Mail hub 2.3.0's upgrade is one-way.** If you run your hub from Orgtree on your computer, going back to Orgtree 4.5.2 or earlier leaves the hub unable to start. See the Orgtree 4.5.3 and Mail hub 2.3.0 release notes for what to do.
- **Phone link and older Orgtree.** If your phone is linked to Orgtree and you later go back to Orgtree 4.5.2 or earlier, link your phone again from Orgtree.

## Your status

- **Pick how you show to others.** Click or tap your own avatar (desktop: the top of the chat list; Android: the Chats screen's top bar) and choose **Automatic**, **Online**, **Busy**, **Do not disturb** or **Invisible**. Your avatar wears the dot others see. **Profile** is in the same menu.
- **Your status is yours, not your device's.** Your hubs keep it until you change it, so it is the same on all your devices, including ones you set up later. Others see it only while one of your devices is connected; when none is, you show offline.
- **Automatic** shows you online while connected, and busy once Hubchat hasn't been in front on any of your devices for an hour. **Online** keeps you online even while you're away.
- **Do not disturb turns off notifications and sounds on all your devices.** Messages still arrive and unread counts and badges still update. Settings > Notifications says it is on, with **Turn it off**.
- **Invisible shows you as offline to everyone else**, with your last seen held at the moment you chose it, while everything keeps working for you. It hides your presence, not what you do: notifications still show, and what you send, your typing and read receipts still reach people.

## Busy and Do not disturb

- **Busy contacts wear an amber dot.** Busy means connected, but no one can answer right now: an Orgtree org none of whose agents that answer outside mail can start a turn, or a person who hasn't had Hubchat in front on any device for an hour, or who chose Busy.
- **Do not disturb wears a red dot.** For an org, its outside mail is on hold: kept, but no agent sees it until one checks. For a person, they've turned off notifications on all their devices; your messages still arrive, and they'll see them when they look.
- Both show wherever presence shows: the chat list, the open chat's header, the hub picker, a mention's hover, Contact info (which explains what each means), New chat and the Directory, which has its own **Busy now** and **Do not disturb** sections after Online now. Offline outranks Do not disturb, which outranks busy.
- The chat header's status line now ends with an ellipsis when it is too long, never mid-word, and never ends in a stray separator.

## Video and audio

- **Videos and audio play in the chat.** A video shows its first frame, paused, as soon as its card is on screen, and only its own controls play it. Its fullscreen button fills the screen; Escape (desktop) or Back (Android) leaves fullscreen and does nothing else. Audio shows its controls when you click **Play audio**.
- **Nothing is downloaded to play.** The player reads only the parts of the file it needs from the hub. Scrolling away, hiding the window or changing chats stops it; it comes back paused.
- Files over 256 MiB, or in a format that can't be played here, show a note and a download instead, as in Orgtree.

## Image viewer

- **Zooming in the viewer zooms only the picture.** On Android, pinching an opened image used to zoom the whole app, which stayed zoomed after closing. Now pinch (or Ctrl and the mouse wheel on desktop) zooms the picture, drag moves it without leaving the screen, and double-tap or double-click zooms in on that point. Letting go of a pinch or drag doesn't close the viewer, and closing it always leaves the app at its normal size.

## Notes

- **Several hubs:** when a contact is on more than one of your hubs, Hubchat shows the most available presence any of them reports. If one of your connected hubs is older than Mail hub 2.3.0, it can't hide you; the Invisible choice names that hub ("can't hide you yet").
- **Fullscreen video on Android needs Android 8.0 or later.** On Android 7, videos play in the chat but cannot go fullscreen.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.7/README.md) for platform setup.

The code is MIT-licensed.
