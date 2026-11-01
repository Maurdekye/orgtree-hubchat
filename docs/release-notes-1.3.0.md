Hubchat 1.3.0 translates messages between people who write in different languages: pick the language you read in, and a hub that translates shows you each message from other people in it, with the original one click away. Contact info now shows the language each person reads in.

## Downloads

| File | For |
|---|---|
| `Hubchat_1.3.0_x64-setup.exe` | Windows 10 or 11, 64-bit. The installer is unsigned; SmartScreen may require **More info > Run anyway**. |
| `Hubchat_1.3.0_arm64.apk` | Android 7.0 or later on 64-bit ARM. Allow installation from your browser or file manager when Android asks. |
| `Hubchat-android.apk` | The same Android app under a stable download name. |
| `Hubchat_1.3.0_universal.dmg` | **Prototype** for macOS, Apple Silicon and Intel. Not signed by Apple or notarized. |
| `Hubchat_1.3.0_amd64.AppImage`, `Hubchat_1.3.0_amd64.deb` | **Prototype** for 64-bit Linux. |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files. |
| `latest.json`, the `.sig` files and `Hubchat_1.3.0_universal.app.tar.gz` | Used by Hubchat's built-in updater. |

## Before you upgrade

- **Translation needs Mail hub 2.4.0, and its owner has to turn it on.** On older hubs Hubchat works as before; the language picker names a hub that can't translate yet. Orgtree 4.5.4 bundles Mail hub 2.4.0.
- **Mail hub 2.4.0's upgrade is one-way.** If you run your hub from Orgtree on your computer, going back to Orgtree 4.5.3 or earlier leaves the hub unable to start. See the Orgtree 4.5.4 and Mail hub 2.4.0 release notes for what to do.
- **Translation needs an Anthropic API key on the hub.** The hub's owner adds a key from the Anthropic Console (it starts with `sk-ant-api`): in Orgtree, App settings > Mail hub > Hosting > Translation; on a standalone hub, see the Mail hub 2.4.0 notes. Anthropic bills that key's account; the hub stops translating for the month at the limit its owner sets.
- **Message text leaves the hub.** When a message to you is translated, its text is sent from the hub's computer to Anthropic. Pick a language only if you're fine with that.

## Translation

- **You opt in by choosing a language.** In Settings > Profile, **Translate messages into** offers Off and 32 languages, each named in its own language; it saves with the profile. It is yours on every device. The caption under it says whether your hubs translate ("Translated by your hub using Claude. Messages to you leave the hub to be translated.", "Your hub doesn't translate.", or which hub can't translate yet).
- **People to people by default.** Messages from other people written in another language are translated for you. Messages from agents and organizations arrive as written, since agents write to you in your language themselves, unless the hub's owner turns on **Translate messages from agents**. Agents and organizations always get the original, and so do your own messages. Senders aren't told.
- **The original goes to the translator exactly as written,** with nothing added, hidden or left out. Every @mention must come back unchanged; if one doesn't, you get the original and the line says "Couldn't translate".
- **Show original.** A translated message shows the translation with one small grey line under it, "Translated from Hebrew · **Show original**", which swaps the text in place ("Original · **Show translation**"). On desktop the message menu has the same toggle after Copy text. Copy text copies what shows.
- **When a message isn't translated, the line says why:** "Not translated: too long", "Not translated: the hub's monthly translation limit is reached" or "Couldn't translate". There's no line when no translation was needed, while one is on its way, or on your own messages.
- **Reply quotes and the chat list show the translation.** Message info has a Translation section with the languages and both versions.
- **Notifications wait up to 4 seconds for the translation,** then show it, or the original. The chat shows the original at once and swaps in the translation when it arrives.
- **Your language shows on your profile** to everyone on your hubs, people and agents, so agents can write to you in it. Contact info has a **Language** row, in the language's own name, for each person who chose one (desktop and Android).

## Notes

- **Not yet tested with real services.** Every test used a fake translation service and synthetic data. Translation with a real Anthropic key end to end, on a real phone, and in real system notifications (the 4-second wait) has not been tried yet.
- **Several hubs:** your language is told to every hub that translates, and Contact info shows the language from the first of your hubs that lists one.
- Windows remains unsigned; macOS and Linux remain prototypes. See the [installation guide](https://github.com/Maurdekye/orgtree-hubchat/blob/v1.3.0/README.md) for platform setup.

The code is MIT-licensed.
