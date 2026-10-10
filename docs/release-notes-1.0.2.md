Hubchat 1.0.2: on Android, Hubchat now says when Tailscale is off and how to keep it on, Settings › About can allow updates, and "last seen" keeps counting.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.0.2_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and later offer this update themselves. |
| `Hubchat_1.0.2_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. Hubchat 0.1.3 and later offer this update themselves. **Coming from 0.1.2 or earlier, install this one by hand.** |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_1.0.2_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_1.0.2_amd64.AppImage`, `Hubchat_1.0.2_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_1.0.2_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **Hubchat says when Tailscale is off (Android).** Android sometimes switches Tailscale off by itself, and a hub that is only reachable through it then just looked down. Now, when such a hub can't be reached and no VPN is on, the notice above your chats says **Tailscale seems to be off. Your hub is only reachable through it.** Its **Open Tailscale** button opens the Tailscale app, or its Google Play page if it isn't installed.
  - The notice goes away by itself once Tailscale is back on and the hub can be reached again.
  - A hub that this phone has reached without a VPN never gets it.
- **How to keep Tailscale on.** The setup guide (**Settings › Help**), the README and the last screen of **Scan setup code** now say how: in Android's **Settings › VPN**, tap the gear beside Tailscale and turn on **Always-on VPN**. Then set Tailscale's battery use to **Unrestricted**.
- **Settings › About can allow updates (Android).** The first in-app update needs Android's permission to install apps. Until now, Settings › About only said so and offered no way there. Its button now reads **Allow** and opens that setting, like the update notice above your chats does. When you come back, the update carries on by itself.
  - Updating from 1.0.1 or earlier: if Settings › About asks you to allow updates, use **Update** in the notice above your chats instead. It opens the setting.
- **"Last seen" keeps counting.** While a contact is offline, their "last seen" now moves on by itself ("just now", then "1 min ago" and so on) in the chat, the contact's details, the Directory and New chat. Before, it stayed as it was when the screen opened.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_1.0.2_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_1.0.2_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.

The code is MIT-licensed.
