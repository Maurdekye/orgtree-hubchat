Hubchat 1.1.0: large files now get through reliably. Uploads and downloads survive a closed app, a lost connection or a hub restart, and continue where they stopped.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.1.0_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer isn't code-signed, so if SmartScreen says "Windows protected your PC", choose **More info › Run anyway**. Hubchat 0.1.1 and later offer this update themselves. |
| `Hubchat_1.1.0_arm64.apk` | Android 7.0 or later on 64-bit ARM, which covers most phones. Allow your browser or file manager to install apps when Android asks. Hubchat 0.1.3 and later offer this update themselves. **Coming from 0.1.2 or earlier, install this one by hand.** |
| `Hubchat-android.apk` | The same Android app under a name that stays the same in every release, so a link to the latest one always works. |
| `Hubchat_1.1.0_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not yet tried on a real Mac. See [First run on macOS and Linux](#first-run-on-macos-and-linux). |
| `Hubchat_1.1.0_amd64.AppImage`, `Hubchat_1.1.0_amd64.deb` | **Prototype** for 64-bit Linux, built on Ubuntu 22.04. Not yet tried on a real Linux PC. |
| `SHA256SUMS.txt` | The SHA-256 checksum of each file. |
| `latest.json`, the `.sig` files and `Hubchat_1.1.0_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## What's new

- **Large files that finish.** Uploads and downloads now survive Hubchat being closed, the connection dropping or the hub restarting. They continue from where they stopped instead of starting over.
  - This works for files up to the size your hub allows: 1 GB on mail hub 2.0.0 or later, unless its owner changed that. With an older hub, an interrupted file starts over.
  - Your other messages keep going out while big files move.
- **A message waits for its files.** When you send files, Hubchat first makes its own copy of each one, shown as **Preparing files**. After that you can change or delete the originals. Your contact gets the message only once all of its files have uploaded.
  - Hubchat needs enough free space for these copies until the message is sent.
- **Progress, cancel and retry.** Each file shows how far it got, also after Hubchat restarts, and you can cancel any upload or download.
  - A cancelled or failed download offers **Retry download**.
  - If a file can't be sent, or you cancel it, the message is held. Choose **Retry**, or choose **Remove file** on that file to send the rest of the message without it.
- **Quiet when it works.** A finished download no longer shows "Saved to Downloads": the file simply appears in your Downloads folder. It appears there only when complete, as a single copy, even if Hubchat was closed while saving it.
- **One notification when a file fails.** If a file can't be sent or downloaded, Hubchat shows one notification for that message. Open the chat to retry. When Hubchat's notifications are off, it shows none.
- **Android: files keep moving in the background.** With **Stay connected** off, Android shows a **Hubchat files** notification while files are sent or saved, and removes it when they're done. If Android doesn't allow that, or stops it after its time limit, files pause while Hubchat is in the background, and Hubchat asks you to keep it open. They continue when you open Hubchat again.
- **Old unfinished transfers start over.** Unfinished transfers are kept for a day. A file that has been stopped for longer, for example because the hub was offline, starts again from the beginning, and a download says so.

## First run on macOS and Linux

- **macOS:** the app isn't signed by Apple or notarized (that needs a paid Apple developer account), so macOS blocks the first open.
  - On macOS 15 or later, try to open Hubchat once. Then go to **System Settings › Privacy & Security** and click **Open Anyway**.
  - On macOS 14 or earlier, right-click the app and choose **Open**.
  - Or, in Terminal: `xattr -dr com.apple.quarantine /Applications/Hubchat.app`
  - After an update, macOS may ask once more whether Hubchat may use its Keychain entry. Choose **Always Allow**.
- **Linux:**
  - **AppImage:** run `chmod +x Hubchat_1.1.0_amd64.AppImage`, then start it. It needs FUSE 2: `sudo apt install libfuse2` on Ubuntu 22.04, or `libfuse2t64` on 24.04 and later. It updates itself.
  - **.deb:** `sudo apt install ./Hubchat_1.1.0_amd64.deb`. Updates ask for your password, because they reinstall the package.
  - Hubchat keeps your identity key in the desktop keyring (GNOME Keyring or KWallet). Without a keyring, creating an identity shows an error instead of keeping the key only in memory.
- On both, Hubchat can't yet save downloads to a Downloads folder on a drive formatted as exFAT.

The code is MIT-licensed.
