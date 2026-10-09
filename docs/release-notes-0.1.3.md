Hubchat 0.1.3: Android now updates itself and takes pictures from the keyboard, you can choose which hub a chat goes through, several hubs work together better, accessibility improvements, and the first prototype builds for macOS and Linux.

## Downloads

| File | For |
|---|---|
| `Hubchat_0.1.3_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and 0.1.2 offer this update themselves. |
| `Hubchat_0.1.3_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. **Coming from 0.1.2, install this one by hand.** From 0.1.3 on, Hubchat offers its own updates. |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_0.1.3_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_0.1.3_amd64.AppImage`, `Hubchat_0.1.3_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_0.1.3_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **Android updates itself.** When a new version is out, a banner offers it: tap **Update**, and Hubchat downloads it, checks it and installs it.
  - The first time, Android asks you to let Hubchat install apps. Hubchat carries on by itself once you allow it.
  - Each download is checked against Hubchat's signature and the version it claims to be. A damaged or mislabelled download is refused, and nothing changes.
  - Your identity, chats and settings stay.
  - On Android 12 and later, the updates install without Android asking you to confirm each one.
  - **Settings › About** shows the version, checks for updates on request, and has a switch for automatic checks.
- **Choose which hub a chat goes through.** When someone is reachable through more than one of your hubs, you pick which one carries your messages.
  - On Windows, click the "via …" label in the chat's header. On Android, open **⋮ › Send through…**.
  - **Automatic** is the default. It uses a hub where they're online, and otherwise the hub the chat used last.
  - If the hub you picked can't reach them, the chat says so and your messages wait for that hub. **Use Automatic** lets them go another way.
- **Several hubs work together better:**
  - A chat keeps sending through the same hub, instead of switching from message to message.
  - **Delete for me** and **Delete chat** reach every hub. A hub that's down at the time gets the delete when it's back.
  - A hub that starts its history over no longer removes messages that another hub still holds.
- **Accessibility and long text:**
  - Right-to-left text, such as Arabic or Hebrew, lines up correctly, paragraph by paragraph.
  - Screen readers get names for buttons and areas, and hear new messages.
  - Every link and list row can be reached with the keyboard. On Windows, the Menu key or Shift+F10 opens the highlighted message's menu.
  - Small text, and the time inside your own messages, are easier to read in both themes.
- **Pictures you send from one of your devices** now show in the chat on your other devices, and their files can be downloaded there.
- **Adding a hub is quicker** when you type its address without a port: Hubchat no longer waits for a port that never answers.
- **Pictures from the Android keyboard:** GIFs, stickers and a picture copied to the keyboard's clipboard, for example in Gboard, go straight into the message box.
- **First prototype builds for macOS and Linux.** GitHub builds and checks them automatically, but nobody has tried them on a real Mac or Linux PC yet. Windows and Android are the tested platforms. Please report what you find.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_0.1.3_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_0.1.3_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.

The code is MIT-licensed.
