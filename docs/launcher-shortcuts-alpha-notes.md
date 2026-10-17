# Hubchat 1.2.2 launcher shortcut alpha notes

Windows: right-click the taskbar icon for open DMs, Settings, Directory and New
chat. Android: long-press the launcher icon for the last four recently messaged
contacts. Each contact entry opens that conversation.

For alpha.2, Windows contacts are grouped under **Recent**, with Settings,
Directory and New chat separately at the bottom. Contact icons use their Hubchat
profile pictures, cached as ICO files in app data and refreshed on picture
changes. For alpha.3, contacts without pictures use their client-list initials
avatar, including the same kind color, shape and initials. Android is
unchanged.

Fallback avatars render from the desktop client's existing functions and CSS;
name, kind and theme changes refresh their ICO. During startup the executable
icon is temporary until the WebView supplies its first avatar, even when hidden.
Settings, Directory and New chat also use their in-app glyphs, rendered from
the shared SVG icon source to the same ICO cache.

Known startup limit: the first Jump List click immediately after an update may
only show the restored app. Later clicks work normally. Existing first-version
argument suppression is preserved to avoid replaying an old setup/chat URL;
restart-argument cleanup is separate work.

Author verification uses contained native/SDK/unit checks. No installed-app or
device actions are performed by the author. Full APK build and interactive
taskbar/launcher smoke belong to the integrated Hubchat alpha.
