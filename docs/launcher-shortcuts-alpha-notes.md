# Hubchat 1.2.2 launcher shortcut alpha notes

Windows: right-click the taskbar icon for open DMs, Settings, Directory and New
chat. Android: long-press the launcher icon for the last four recently messaged
contacts. Each contact entry opens that conversation.

Known startup limit: the first Jump List click immediately after an update may
only show the restored app. Later clicks work normally. Existing first-version
argument suppression is preserved to avoid replaying an old setup/chat URL;
restart-argument cleanup is separate work.

Author verification uses contained native/SDK/unit checks. No installed-app or
device actions are performed by the author. Full APK build and interactive
taskbar/launcher smoke belong to the integrated Hubchat alpha.
