Hubchat 1.3.3 shows Orgtree's mentions (tickets, documents, apps, panels, mails, agents and projects) as chips, and on a PC that runs that org's Orgtree a click opens the target in Orgtree.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.3_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.3_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.3_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.3_amd64.AppImage`, `Hubchat_1.3.3_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.3_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Orgtree mentions

- **Mentions are chips.** `@item:`, `@doc:`, `@app:`, `@panel:`, `@mail:`, `@agent:` and `@project:` mentions in any message show as chips with Orgtree's mark for their kind and the target's id. Agents and projects share one mark. In a conversation with an org, a bare `@name` that isn't one of your contacts is that org's agent or project. Hubchat fetches nothing from Orgtree, so a chip has no title, read state or model. Copying a chip copies the mention as written.
- **A click opens it in Orgtree** on a Windows PC whose Orgtree runs that org, with Orgtree 4.5.9 or later installed. Right-click a chip for **Open <org> in Orgtree**, which opens just the org.
- **Anywhere else the chip is display-only.** Its hover says which PC it opens on. On a phone, a tap shows that hint. Mentions in other people's messages between people never open.

## Notes

- Hubchat now says "org mailbox" where it said "org inbox", matching Orgtree 4.5.8.
- Nothing else changed since 1.3.2.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.3/README.md) for platform setup.

The code is MIT-licensed.
