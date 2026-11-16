Hubchat 1.3.7 adds Hubchat Web, Hubchat as a web app for an iPhone, an iPad or any browser, and shows on Windows which chats have a question waiting for your answer.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.7_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.7_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.7_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.7_amd64.AppImage`, `Hubchat_1.3.7_amd64.deb` | **Prototype** for 64-bit Linux. |
| `hubchat-web-1.3.7-docker-amd64.tar.gz` | **New.** The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.3.7` and `:latest`. |
| `hubchat-web-1.3.7-static.tar.gz` | **New.** The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.7_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Hubchat Web

There is no Hubchat app for iPhone or iPad. Hubchat Web is Hubchat as a web app instead, and it also works on any computer where nothing can be installed. One person runs a small server; everyone else opens its address in a browser and, on a phone, adds it to the Home Screen.

```
docker run -d --name hubchat-web --read-only --cap-drop ALL \
  -p 127.0.0.1:8080:8080 ghcr.io/maurdekye/hubchat-web:latest
```

- **The server only hands out the app.** It holds no accounts, identities, keys or messages, sets no cookie and needs no storage. Each person's identity and keys are made and kept in their own browser, and the browser talks to the mail hub directly. Whoever runs the server controls the code those browsers run, so use one run by you or by people you trust.
- **What you need to run it:** an `https://` address for the web app and one for your hub (for example with `tailscale serve`), and Mail hub 2.5.0 or later with the web app's address in its `HUB_WEB_ORIGINS` setting. Notifications need Mail hub 2.5.1 or later.
- **Starting:** create a new identity, or link the browser to the identity on your PC or Android phone by scanning its QR code with the camera, typing the code, or with a key file. Recovery words must be saved before the chat list opens: clearing the browser's data deletes the identity, and the words bring it back.
- **A first hub with one tap.** Someone who has just made an identity is offered the hubs the server lists, or a hub that answers at the web app's own address. The server can list up to eight hubs or switch suggestions off. Nothing is added without a tap.
- **Chats:** text both ways with Markdown, replies, receipts, typing, presence, drafts, deleting, the directory and Settings, as in the apps.
- **Files and pictures.** The paperclip opens the browser's file chooser, and a pasted picture attaches too. Pictures show in the chat. A received file is kept in the browser until you open or save it.
- **Notifications.** Off until you turn them on, from a card on the chat list or in Settings, Notifications. Your hubs then send them, also while Hubchat is closed. Each message alerts, also a second one in the same chat. A hub that is too old to send them is named in Settings.
- **Works offline and updates itself.** The app's files are kept in the browser, so Hubchat opens and shows the chats it holds when the server or the network is away. When the server has a newer version, the app says "A new version is ready" and keeps working until you tap **Update**.
- **On an iPhone or iPad:** open the address in Safari, tap **Share**, then **Add to Home Screen**, and open Hubchat from there. In a Safari tab the app only explains this, because an iPhone deletes a web page's data after some days unless it is on the Home Screen.
- **For whoever runs it:** the image is one program, runs as a non-root user, writes nothing and has its own health check. Settings cover the port, its own HTTPS certificate, the suggested hubs, a name on the welcome screen and request logging. Keep the server at least as new as the newest version your people have used: a browser that meets an older server shows "Update the Hubchat Web server" and leaves its data untouched until the newer one is back. Guide: [Running Hubchat Web](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.7/docs/hubchat-web-server.md). The Help screen of every Hubchat app has a new section, **iPhone and other devices: Hubchat Web**.

Not in Hubchat Web yet: video and audio don't play in the chat (they send and download as files), files can't be dropped onto the chat, and there is no screen to see or free what the browser stores.

Not tried: a real iPhone or iPad, Safari and Firefox. It has been used by hand in Chrome on Android and on Windows, including linking by camera and notifications.

## Questions that need your answer

- **The chat list shows which chat is waiting.** A chat with a question card you haven't answered has a bold name and a **?** badge. Clicking it opens the chat, scrolls to the oldest waiting card and highlights it. Desktop and Android.
- **The taskbar says why its dot is on** (Windows). Hovering the Hubchat taskbar button reads, for example, "Hubchat - 2 questions await your answer - 3 unread".
- **Needs attention in the taskbar's right-click list** (Windows). While questions wait, the list shows a **Needs attention** group in place of Recent, one entry per question, newest first, at most ten. Clicking one opens the chat at that card. Recent returns when the questions are settled.

## Notes

- Nothing else changed since 1.3.6.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.7/README.md) for platform setup.

The code is MIT-licensed.
