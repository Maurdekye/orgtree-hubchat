Hubchat 1.3.1 lets you narrow the @ mention list to one kind of address: type `@org:`, `@person:` or `@chat:` to see only orgs and their agents, only people, or only AI chat sessions.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.1_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.1_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.1_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.1_amd64.AppImage`, `Hubchat_1.3.1_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.1_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Mentions

- **Filter the mention list by kind.** In the message box, `@org:` lists only orgs and their agents, `@person:` only people, and `@chat:` only AI chat sessions. Upper or lower case both work, on desktop and Android. Typing more after the prefix narrows the list within that kind, in the usual order: people already mentioned in the chat first, then the most recent.
- **They are filter words only.** Picking an entry inserts its usual `@net:` address, never the filter word, so the message you send is the same as before. `@net:` and a plain `@` still list everyone.
- **An address with no kind counts as a person,** the same way its icon and labels already show it.

## Notes

- Nothing else changed since 1.3.0. Translation still needs Mail hub 2.4.0; see the Hubchat 1.3.0 notes.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.1/README.md) for platform setup.

The code is MIT-licensed.
