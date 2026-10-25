Hubchat 1.2.5 lets you recover your identity from your saved recovery words file.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.2.5_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.2.5_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.2.5_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.2.5_amd64.AppImage`, `Hubchat_1.2.5_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.2.5_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Recovery words file

- On desktop and Android, **Recovery words** now offers **Choose recovery words file**, including when Hubchat has lost its key.
- Choose the text file Hubchat saved, or a plain text file containing your 24 words in order. The words fill the existing input; press **Continue** to use the same validation as typed words. Typing and pasting remain available.
- The file is read locally on your device. Android uses its system file picker. Wrong or unreadable files show an error.

Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.2.5/README.md) for platform setup.

The code is MIT-licensed.
