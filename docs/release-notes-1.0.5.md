Hubchat 1.0.5: on Android, Hubchat can now be woken through UnifiedPush, for example with ntfy, so it no longer needs its own ongoing notification.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.0.5_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and later offer this update themselves. |
| `Hubchat_1.0.5_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. Hubchat 0.1.3 and later offer this update themselves. **Coming from 0.1.2 or earlier, install this one by hand.** |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_1.0.5_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_1.0.5_amd64.AppImage`, `Hubchat_1.0.5_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_1.0.5_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **Optional UnifiedPush (Android).** Hubchat can now be woken through a UnifiedPush distributor app such as ntfy, with a public or self-hosted server, instead of keeping its own connection open. With push on, Hubchat's **Hubchat is connected** notification goes away. Push is off unless you turn it on; **Stay connected** and the 15-minute check work as before.
  - To use it, install ntfy. Then in **Settings › Notifications**, select it and turn on **Use UnifiedPush**, and wait for **Push is on**.
  - Every hub you use needs mail hub 2.1.0 or later. Until then, Hubchat says a hub needs an update and keeps its usual connection.
  - Let ntfy run in the background: in Android's app settings for ntfy, set battery use to **Unrestricted**. Otherwise Android may pause it. As a backup, Hubchat still checks about every 15 minutes, so a missed wake-up makes a message late, not lost.
  - ntfy may show its own quiet notification while it waits for wake-ups. To hide it, turn off ntfy's **Background service** notifications in Android's settings.
  - Your hub sends the distributor's server only a wake-up signal, with no message content. Hubchat then gets your messages from your hub as usual. A hub you reach only privately, for example over Tailscale, must still be able to reach the distributor's server.
  - If ntfy is uninstalled, Hubchat notices and goes back to its usual connection, and Settings says why.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_1.0.5_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_1.0.5_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.

The code is MIT-licensed.
