# Translation between people

Hubchat 1.3.0 with Mail hub 2.4.0 (feature `translate`). Orgtree docket
mail-hub-hubchat-automatic-translation-between-u; design
hubchat-translation-design-1-3-0; the user's answers 2026-11-01 12:15Z (a
per-hub default language for senders who set none, notifications wait up to
4 s, Anthropic only, senders are not told) and "Build it" 12:16Z.

## What a person sees

- **Settings › Profile › Translate messages into.** Off, or one of 32
  languages, each named in its own language (the same list as Orgtree's
  Hosting tab). It is saved with the profile's Save. It is yours on every
  device, and only your hubs are told it: nobody else can see it.
- **The caption under it**, from the connected hubs:
  - a hub that translates: "Translated by your hub using Claude. Messages to
    you leave the hub to be translated.";
  - a hub with the feature but translation off: "Your hub doesn't
    translate.";
  - a hub reached through its relay-only door (which doesn't say): "Translated
    by your hub when its owner turns translation on. …";
  - a connected hub without the feature is named: "… · Hub B can't translate
    yet". When none can, the row is disabled (unless a language is already set,
    so it can be turned off).
- **A translated message** shows the translation, with one small grey line
  under it: "Translated from Hebrew · **Show original**". The link swaps the
  text in place ("Original · **Show translation**"), in this window, like Show
  more. The desktop menu has the same toggle right after Copy text. Copy text
  (desktop menu, hover bar, Android selection bar) copies what shows.
- **A message that wasn't translated** says why on the same line: "Not
  translated: too long", "Not translated: the hub's monthly translation limit
  is reached" or "Couldn't translate". No line when no translation was needed,
  while it is on its way, or on your own messages.
- **Reply quotes and the chat list** show your translation.
- **Message info** has a Translation section: the languages ("From Hebrew into
  English, by your hub") and both versions.
- **Notifications** wait for the translation up to 4 s after the hub received
  the message, then show it (or the original). A phone woken by a push gets
  the hub's own 4 s wait, so it isn't held again. The chat shows the original
  at once and swaps the translation in when it comes.

Agents and organizations always get the original (the hub's rule); so do
messages you send.

## How it works

- **The language** (`engine_translation.rs`, kept like your status): the
  person's pick is `language.mine` ("" for none). It is owed to every saved
  hub (`language.owed.<url>`); each hub's sync tells it before its next
  request (`POST /api/profile {"language": ...}`, the language alone), only to
  a hub with the feature `translate`; a 4xx or a hub without it ends the debt.
  Each hub's word, `language` in its sync answers, is kept
  (`language.said.<url>`): a changed word is a pick on another of your
  devices, and is taken; while a hub is owed your pick only its own word ends
  the wait. The keys are in `forget_hub_sync`. The shell reads
  `chosen_language` into `State.language`; `hc_set_language` sets it.
- **A message's translation** comes only in your own sync and history, as
  `translation: {lang, status, source_lang?, body?}` on the message
  (`hub_v2::Translation`). It is untrusted: an unknown status, a language that
  is not a tag, a translated one without text, or text over 200,000 bytes is
  left out (never the message). When its outcome is known the hub sends the
  message again; the store keeps the last translation told (store migration
  23, `messages.translation`, written by its own statement only when one is
  told, so an answer without it keeps the last). It is stored as shown: a
  reply's one-line quote off, like the original; a message whose plain-text
  part stands for a question card keeps none (that part can't be cut from a
  translation), so it shows untranslated.
- **Notifications** (`notice_incoming`, `release_held`, `held_loop`): new mail
  whose translation is pending is held until `received_at` (made ours by the
  hub's clock offset) plus 4 s, or until its outcome comes in a sync
  (`translation_came`); then it is notified with the translation's text, or
  the original. Mail deleted meanwhile is not notified.
- **The hub's word** on translating is `/healthz` `translation.state` (off,
  ok, no_key, key_refused) on its main port, read only from a hub with the
  feature, into `HubStatus.translation`.
- **The UI** is `src/lib/translation.ts` (the list, `translatedText`,
  `shownText`, `untranslatedReason`, `translationCaption`, the per-window
  original toggles), `MessageView`'s `tr-line`, `MessageInfo`'s Translation
  section and `Settings` › Profile.

## Proofs

- Core: `cargo test -p hubchat-core --lib translation_tests` (10 tests: the
  store, the checks, a reply, the hub's word, the language owed/told/heard,
  and the notification waits; a fake hub on loopback answers
  `/api/profile`).
- UI: `node tools/proof-translation.mjs` (mounted App, synthetic data,
  desktop and Android; Copy is not clicked, as the capture guard refuses
  every clipboard attempt).

Not measured by any test: a real hub, a real provider or key, a real phone.
