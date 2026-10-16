Hubchat 1.2.1 fixes two Android problems: returning from Contact info no longer freezes the app, and tapping a finished download card opens the file.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.1_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.1_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.1_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.1_amd64.AppImage`, `Hubchat_1.2.1_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.1_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Fixes

- **Back from Contact info works again on Android.** Use the on-screen Back button, Android's Back key or the back gesture to return to a responsive conversation, including while its messages are still loading.
- **Finished download cards open their files on Android.** Tap a completed download to open it with a suitable app. Android app files (`.apk`) open Android's installer, which may first ask for permission to install apps from Hubchat.

## Notes

- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.1/README.md) for platform setup.

The code is MIT-licensed.
