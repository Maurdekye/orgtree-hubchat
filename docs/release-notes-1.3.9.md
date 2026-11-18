Hubchat 1.3.9 opens links to your Orgtree orgs again after Orgtree's data has been moved to another folder or drive.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.9_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.9_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.9_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.9_amd64.AppImage`, `Hubchat_1.3.9_amd64.deb` | **Prototype** for 64-bit Linux. |
| `hubchat-web-1.3.9-docker-amd64.tar.gz` | The Hubchat Web server as a container image file, for `docker load` (64-bit x86). The same image is published as `ghcr.io/maurdekye/hubchat-web:1.3.9` and `:latest`. |
| `hubchat-web-1.3.9-static.tar.gz` | The Hubchat Web app's plain files, for a web server of your own. |
| `build-info-web.json` | SHA-256 record of the two Hubchat Web files and what is inside them. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.9_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Orgtree links after a data move

- **Links open again.** Orgtree 4.6 can move its data to another folder or drive (Settings > Data location). Hubchat looked for Orgtree's list of orgs only in the old default folder, so after a move every Orgtree item, document and agent link showed grey and couldn't be opened, and the Contact info "Orgtree org" badge turned into plain text. Hubchat now follows the moved folder, and falls back to the default one.
- **A clearer hint.** When Orgtree is installed on the PC but its list of orgs can't be read, a link's hint now says "Orgtree on this PC didn't list its orgs" instead of naming another PC.
