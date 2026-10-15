# Question kind tags

User ruling 2026-10-15 00:25Z: keep kind tags that change how a message is
presented or used, namely Question. Message, Status, Request, Decision and
unknown kinds have no tag in Hubchat. This is a shared-renderer display change;
message kind data, storage and hub transport remain as before. The subsequent
00:27Z user ruling extends this to every kind-specific presentation: all
non-question kinds are treated exactly as plain messages.

`MessageView` uses the existing question-card header as its only kind tag.
Its screen-reader prefix likewise names only Question. Question choices,
pick state, Reply hints, settled/skip state and jump actions are unchanged.

## Kind presentation inventory and disposition

- `Conversation.tsx` now starts a new incoming message group by kind only for
  Question. Request/Decision/Status use the same spacing rules as plain messages;
  time gaps and direction changes still start groups for every message.
- `MessageInfo.tsx` drops the Kind row. This includes questions: their transcript
  header already identifies them and the details row added no interaction or
  information. The coordinator explicitly allowed dropping this row for questions.
- `ChatList.tsx` already prefixes only incoming Question previews. Its filters
  and the org/session chips describe contact types, not message kinds.
- `base.css` removes the now-unused decision/status tag color classes.
- Question kind checks in `questions.ts`, `answers.ts` and `Conversation.tsx`
  control card settlement/answers; they remain unchanged.

## Focused proof

`tools/proof-question-tags-renderer.mjs` bundles the real `MessageView` and
mounts it under each `PlatformCtx` value. It covers message/status/request/
decision/unknown/null/question, both incoming and outgoing. Boolean absence
checks avoid reporting React objects. Controls check body/data preservation,
question buttons, current picks, pick/Reply actions, answered/skipped locks,
jump-to-answer actions and free-response questions. It mounts the real
Conversation with synthetic state to verify grouping and MessageInfoBody to
verify removal of the Kind row. It also checks removal of the unused CSS.

Run inside Rig's shared lock and kernel Job, with existing dependencies read
in place and all output/TEMP under its assigned C build scope. Set
`HUBCHAT_DEPENDENCY_ROOT`, `HUBCHAT_DOM_DEPENDENCY_ROOT`,
`HUBCHAT_RENDERER_OUT`, and `HUBCHAT_JOB_CONTAINED=1`, then run:

```powershell
node tools/proof-question-tags-renderer.mjs
```

Green leaves `HUBCHAT_KIND_TAG_BASE` unset. Red sets it to `c2c5f6c`;
`MessageView.tsx`, `Conversation.tsx` and `MessageInfo.tsx` are substituted
through the bundler, with no source mutation; the CSS check reads the base blob.
Expected: green 147 checks pass; red exactly 45 failures (24 removed tag/prefix
checks, 14 redundant info rows, 6 grouping checks, 1 CSS check), with 102 controls
passing. These counts and runtime behavior are **pending Rig's
execution**, not measured in the source-only author task. Typecheck is also
delegated to Rig. No native build, install, transport or device claim.
