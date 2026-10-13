# Help section tabs

The setup guide stays in `docs/setup.md`, shared by GitHub and the application.
`splitGuide` strips the app-excluded screenshot blocks and document title, then
uses Markdown heading tokens to split at each level-two heading. Code fences do
not create tabs. The short introduction remains above the tab strip; every
section retains its heading and complete content in document order.

Settings Help and both `openGuide` dialogs use the same `GuideBody`. Its tabs have
linked tab/panel roles, one tab stop, arrow-key wraparound and Home/End navigation.
The strip scrolls horizontally and long labels wrap inside a tab bounded by the
strip width. Dialog Escape and Android back handlers remain unchanged.

Bundled guide fragment links are preserved through the existing sanitizer using
private non-navigating targets, then handled locally: heading anchors select the
owning section and reveal its heading. External links still use `openLink` and
the system browser. Message markdown's http(s)-only link policy is unchanged.
