# Copy mention and the left-click action in context menus

Every Hubchat menu on an addressable thing leads with what a left-click on it does, and offers **Copy mention**:
the exact text that mentions it (USER 2026-11-10). The old menu entry "Copy address" became "Copy mention"
(it already copied the mention); the icon buttons beside a shown address keep "Copy address".

Proof: `tools/proof-copy-mention.mjs` (jsdom, recording clipboard stub; checks C1-C13, controls CT1-CT4).

## Surfaces

| Surface | Platform | Left-click / tap | Menu |
| --- | --- | --- | --- |
| Chat list row, collapsed rail row | desktop | opens the chat | Open, Contact info, **Copy mention**, Close DM |
| Directory row, New chat row | desktop | opens the chat | Message, Contact info, **Copy mention** |
| Contact mention in a message | desktop | opens its chat (nothing when no hub lists it) | Open chat, Contact info, **Copy mention**; only **Copy mention** when no hub lists it |
| Orgtree chip in a message | desktop | opens the item in Orgtree when it can | Open <kind> in Orgtree (only when it can open), Open <org> in Orgtree (disabled when it cannot), **Copy mention** |
| Chat header, avatar or name | desktop | Contact info | Contact info, **Copy mention**, Close DM (new menu) |
| Conversation menu (⋮) | Android | | Contact info, Send through…, **Copy mention**, Close DM |
| Contact mention in a message | Android | opens its chat | long-press sheet: Open chat, Contact info, **Copy mention** (only Copy mention when no hub lists it) |
| Orgtree chip in a message | Android | a toast with its hint | long-press sheet: **Copy mention** |
| Message info preview | both | nothing | only **Copy mention** for the target |

Android chat, Directory and New chat rows have no long-press menu; a phone contact's mention is copied from the
conversation's ⋮ menu. There is no per-message sender name or avatar (chats are one-to-one).

## What is copied (no trailing space)

| Target | Copied |
| --- | --- |
| A contact of any kind (person, org, chat) | `@net:<full address>` (`mentionWireToken`), also for a mention written as `@maya` |
| An Orgtree token chip | the token as written, e.g. `@item:alpha/fix-login` |
| A bare-name chip whose org is on this PC | `@agent:<org>/<name>` |
| A bare-name chip whose org is not on this PC | `@<name>` as written |

Orgs are mentioned as `@net:` too: Hubchat never sends `@org:`. The toast is "Mention copied".
