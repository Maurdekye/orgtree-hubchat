# Linux desktop entries and `hubchat://` scheme handler

On Windows, Hubchat registers `hubchat://` in `HKCU\Software\Classes\hubchat`
at start (`src-tauri/src/lib.rs` `register_scheme`). Linux has no equivalent
call for an AppImage, so at each desktop start on Linux Hubchat manages its
per-user desktop integration (`src-tauri/src/linux_scheme.rs`, mirroring
Orgtree's `apps/desktop/main/orgtree-link-linux.ts`; user rulings 2026-11-21
`m705e9611b564`, `mcfbe866f5243`, and docket
`steamos-steam-frame-automate-the-install-workaro`):

- `$XDG_DATA_HOME/applications/dev.orgtree.hubchat.link.desktop` (default
  `~/.local/share/applications`):
  - `Type=Application`, `Name=Hubchat link`, `NoDisplay=true`,
    `MimeType=x-scheme-handler/hubchat;`, `X-Hubchat-Version=<version>`.
  - `Exec=<program> %u`, where `<program>` is `$APPIMAGE` else the running
    executable. A plain path matching `^/[A-Za-z0-9/._+-]+$` is written bare
    (SteamOS gamescope's `xdg-open` keeps quotes as part of the name); any
    other absolute path without control characters is quoted and escaped per
    the Desktop Entry specification.
  - When running from an AppImage without FUSE (`APPIMAGE` set and `APPDIR`'s
    basename starts with `appimage_extracted_`), `Exec` is prefixed with
    `env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1 ` so the AppImage runtime
    (type2-runtime `fedfad5`) does not delete the running app's extraction
    folder when the link-forwarding second launch exits.
  - Byte-compared before writing and rewritten atomically only when changed.
- `$XDG_DATA_HOME/applications/dev.orgtree.hubchat.desktop` and
  `$XDG_DATA_HOME/icons/hicolor/128x128/apps/dev.orgtree.hubchat.png` (AppImage
  or `HUBCHAT_LAUNCHER` launches only; `.deb` packages supply their own
  system desktop file):
  - Created if absent, and updated/repaired only when the existing file is a
    regular file marked with `X-Hubchat-Version=` under `[Desktop Entry]`.
  - Uses the same extract-mode `env APPIMAGE_EXTRACT_AND_RUN=1 NO_CLEANUP=1`
    and bare-path rules (or `HUBCHAT_LAUNCHER` when valid).
  - Hand-written `dev.orgtree.hubchat.desktop` files without
    `X-Hubchat-Version=`, entries under other filenames, and existing icons are
    never overwritten. An icon write failure never blocks link registration.
- `$XDG_CONFIG_HOME/mimeapps.list` (default `~/.config/mimeapps.list`):
  - Sets `x-scheme-handler/hubchat=dev.orgtree.hubchat.link.desktop;` under
    `[Default Applications]` only when no default exists for the scheme or it
    is already ours. Any other default is left untouched byte for byte.

See [SteamOS and Steam Frame](steamos-steam-frame.md) for the Gamescope re-exec
and permissions-only key fallback used on SteamOS systems without FUSE or
Secret Service.

## Launcher opt-in (`HUBCHAT_LAUNCHER`)

When a custom wrapper script launches Hubchat (for example to set desktop/wallet
environment variables before invoking the AppImage), the wrapper can export its
own absolute path:

```sh
export HUBCHAT_LAUNCHER=/home/steamos/.local/opt/hubchat/launch-hubchat
```

When `HUBCHAT_LAUNCHER` is set, absolute, valid under the Desktop Entry path
rules (bare when plain, quoted+escaped otherwise), and points to an existing
executable regular file, `dev.orgtree.hubchat.link.desktop` uses:

```ini
Exec=env NO_CLEANUP=1 <launcher> %u
```

If `HUBCHAT_LAUNCHER` is unset, relative, contains control characters, missing,
a directory, or not executable, Hubchat falls back to the `$APPIMAGE` /
running-executable form.

## Guards

Hubchat writes nothing when:
- the app identifier ends with `.test`;
- the account's real home (read via `libc::geteuid` + `libc::getpwuid_r`) is
  unavailable, or `$HOME` is set to a different folder (such as CI smoke runs);
- the resolved program path is relative or contains control characters.

Hubchat never modifies or deletes any other `.desktop` file, nor any
hand-written `dev.orgtree.hubchat.desktop` that does not carry
`X-Hubchat-Version=`.

## Verification

- `cargo test --locked -p hubchat --lib linux_scheme`: bare/quoted `Exec`,
  mounted vs extracted (`NO_CLEANUP=1`), `HUBCHAT_LAUNCHER` opt-in and
  fallbacks, guards, `mimeapps.list` section handling, and untouched foreign
  defaults / `dev.orgtree.hubchat.desktop`.
- CI `.github/workflows/build-linux.yml` (`keyring` compiles the Linux-gated
  code and tests; `build` builds and launch-smokes x86_64 and aarch64 AppImage
  and `.deb` packages).
