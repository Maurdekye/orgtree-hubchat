Hubchat 1.0.3: Hubchat now gets through Windows losing its saved sign-ins, never starts over without a word, and on Android the jump-to-newest button stays clear of the reply bar.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.0.3_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and later offer this update themselves. |
| `Hubchat_1.0.3_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. Hubchat 0.1.3 and later offer this update themselves. **Coming from 0.1.2 or earlier, install this one by hand.** |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_1.0.3_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_1.0.3_amd64.AppImage`, `Hubchat_1.0.3_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_1.0.3_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **Hubchat gets through Windows losing its saved sign-ins (Windows).** Hubchat keeps your identity key in Windows' Credential Manager. After a crash, Windows can reset its saved sign-ins, and Hubchat then started over as if it were new. Now Hubchat also keeps a backup of the key in its own folder, protected by Windows for your user account, as Credential Manager is. If the Credential Manager entry is gone, Hubchat puts the key back from the backup by itself and says so once: **Windows had lost Hubchat's saved key. Hubchat put it back from its own backup on this PC, so nothing changed.**
  - The backup is made the first time 1.0.3 starts.
- **No more silent fresh start.** If Hubchat can't find its key anywhere, but this device had an identity, it now says **Hubchat lost its key on this PC** (or phone) instead of showing the welcome screen. Your chats are still there. You can **Link from your other device**, type your **Recovery words**, or **Start over with a new identity** (Hubchat asks first).
  - If Credential Manager isn't answering yet, for example just after you sign in, Hubchat says **Hubchat can't read its key yet** and tries again by itself for a minute.
- **A new identity starts clean.** Making a new identity, or typing the recovery words of another one, no longer keeps the old identity's chats, hubs and settings. Your own recovery words, or linking, keep them.
- **The jump-to-newest button stays clear of the reply bar (Android).** It could cover the X that cancels a reply, and the attachment and edit bars too. It now always sits just above the message box.
- **Linking through a mail hub in Docker.** If whoever runs a standalone mail hub (2.0.2 or later) has written down its outside address, Hubchat now puts that address first in the QR code for linking another device, after checking that it leads to the same hub. With the mail hub built into Orgtree, nothing changes.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_1.0.3_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_1.0.3_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.

The code is MIT-licensed.
