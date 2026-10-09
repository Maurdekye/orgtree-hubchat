Hubchat 1.0.1: fixes. On Android the always-on notification no longer puts a "1" on Hubchat's icon, Check for updates stays in one place, the Directory starts with everyone on your hubs, and on Windows an update no longer reopens an old link.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.0.1_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and later offer this update themselves. |
| `Hubchat_1.0.1_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. Hubchat 0.1.3 and later offer this update themselves. **Coming from 0.1.2 or earlier, install this one by hand.** |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_1.0.1_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_1.0.1_amd64.AppImage`, `Hubchat_1.0.1_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_1.0.1_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **No more "1" on Hubchat's icon on Android.** The always-on notification that keeps Hubchat connected no longer counts toward the icon's badge, so the number (or dot) on the icon shows only messages you haven't read.
  - The update fixes it by itself; there is nothing to change in Android's settings.
  - Android treats that notification as a new category with the same name, **Background connection**. If you had changed Android's settings for it, for example to hide it, set them again.
- **Check for updates stays in one place.** In **Settings › About**, the button now sits on its own line with the update status under it, so it no longer moves while Hubchat checks. On Android it spans the screen.
- **The Directory starts with everyone on your hubs.** The card about yourself at the top is gone. Your address is in **Settings › Profile**.
- **On Windows, an update no longer reopens an old link.** If Hubchat had been started by opening a hubchat:// link, **Restart to update** opened that link again. Now the first start of a new version leaves it alone, and the chat you were in comes back as usual.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_1.0.1_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_1.0.1_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.

The code is MIT-licensed.
