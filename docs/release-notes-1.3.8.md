Hubchat 1.3.8 lets the apps and Hubchat Web use a mail hub on a home or office network with a certificate from your own authority, and writes more to the desktop log when an update check fails or the window stops answering.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.8_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.8_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.8_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.8_amd64.AppImage`, `Hubchat_1.3.8_amd64.deb` | **Prototype** for 64-bit Linux. |
| `hubchat-web-1.3.8-docker-amd64.tar.gz` | The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.3.8` and `:latest`. |
| `hubchat-web-1.3.8-static.tar.gz` | The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.8_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## A hub on your own network

- **Your own certificate authority now works.** Hub connections trust the certificates the device trusts, beside the built-in public list, the way a browser does. Install your authority's certificate on the PC or Android phone and Hubchat accepts a hub certificate it issued, with no restart and no setting. A certificate removed from the device stops counting within ten minutes.
- **A plain-words error for a wrong certificate.** A single self-signed certificate used for everything is refused, as before, but the message now says what a hub needs: the authority's certificate on the device, and a separate certificate for the hub issued by it.
- **Links from an app reach a browser.** An app knows its hub by its `http://` address, which a web page can't use. Linking Hubchat Web from an app now finds the same hub's `https://` address instead of stopping.
- **Guide:** [Hubchat Web on a LAN-only hub](https://github.com/Maurdekye/orgtree-hubchat/blob/main/docs/hubchat-web-server.md#on-a-lan-only) shows how to make your own authority and put an HTTPS proxy in front of the hub's relay-only door. Mail hub's own guide covers the hub side.

## Better diagnostics on the desktop

On Windows, macOS and Linux, the diagnostics log (Settings > Developer > Open logs folder) now records:

- **Why an update check failed:** no network, a server error, a bad update file, a bad signature, or other.
- **A window that stops answering:** when it has not answered for 15 seconds, 1, 5 and 15 minutes, and when it answers again. A hidden window or a sleeping PC doesn't count.

The new lines contain no addresses, file paths, chat content or other programs' error text. Android still keeps no log file.
