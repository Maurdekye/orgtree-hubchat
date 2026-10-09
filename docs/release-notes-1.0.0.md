Hubchat 1.0.0: connect your phone to Orgtree by scanning a code, a new device is ready at once and loads older messages as you scroll back, and messages stay in order across hubs. It goes with Orgtree 4.1.0.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.0.0_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and later offer this update themselves. |
| `Hubchat_1.0.0_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. Hubchat 0.1.3 and later offer this update themselves. **Coming from 0.1.2 or earlier, install this one by hand.** |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_1.0.0_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_1.0.0_amd64.AppImage`, `Hubchat_1.0.0_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_1.0.0_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **Connect your phone to Orgtree by scanning a code.** In Orgtree 4.1.0, open **App settings › Mail hub › Connect your phone** on your PC: it shows a setup code. Scan it with the phone's camera, or tap **Scan setup code** on Hubchat's first screen.
  - Hubchat checks what the phone needs and says only what's missing: the Tailscale app, signed in as the same account as your PC, or a PC it can't reach, each with what to do.
  - Type your name and tap **Continue**. Hubchat makes your identity, joins your PC's hub and messages your org. When the org confirms, the chat says **Linked — your org knows this address is you.**
  - Already use Hubchat? **Add** puts your PC's hub beside your others.
  - A code lasts 10 minutes and works once. An old one says so and offers **Scan again**.
- **A new device is ready at once.** On hubs running mail hub 2.0.1 or later, a new device starts from now instead of first downloading every message the hub holds.
  - The chat list and unread counts come straight from the hubs.
  - Older messages load as you scroll back.
  - If a hub is offline, a line at the top of the chat says its older messages aren't loaded yet. They fill in when it's back.
  - Older hubs still send the whole history, as before.
- **Messages stay in order across hubs.** Hubchat corrects for each hub's clock, so a reply never shows above the message it answers, and a message held on two hubs sits in one place on every device.
- **Delete chat, on a hub that was down at the time,** now also removes the older messages in it that this device never loaded (mail hub 2.0.1 or later). Messages that arrive after you deleted the chat stay.
- **Link a device** shows a code with the address your other devices can reach (your PC's phone access), not one only the PC itself can use.
- **The setup guide** is in the app: every way to connect, from Orgtree with Tailscale to a hub of your own, and what to check when it stops working.
- **Hubchat opens on the chat you were in.** After a restart, an update or closing the app, the chat that was open comes back, on Windows and Android. On Android, Back from it leads to the chat list, and if you went back to the list before closing, Hubchat opens there.
- **On Windows, the window stays up after Restart to update.** If Hubchat had started with Windows, in the tray, the restart after an update hid the window again. The first start of a new version now always shows it, starting with this update.
- **The version** sits next to "Hubchat" in the title bar on Windows, instead of beside "Chats".
- **On Android, a tapped notification opens its chat at once** while Hubchat is open, instead of after you leave the app and come back. The same goes for a setup code scanned with the phone's camera. And Back from a chat opened by a notification leads to the chat list.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_1.0.0_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_1.0.0_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.

The code is MIT-licensed.
