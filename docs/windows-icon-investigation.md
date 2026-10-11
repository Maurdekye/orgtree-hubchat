# Windows child-process icon investigation

Status: investigation in progress; no cause or fix established (2026-10-11).

The reported Task Manager view shows a normal Hubchat group icon and a corrupted
indented Hubchat entry. A comparison screenshot shows normal Orgtree child icons
in the same Task Manager view. These observations establish the symptom only.

## Source inspection

- `src-tauri/tauri.conf.json` bundles the existing Hubchat PNG, ICO and ICNS assets.
  The single main window has no explicit icon override.
- `src-tauri/src/lib.rs` supplies `app.default_window_icon()` to the tray. No
  application window `set_icon` call was found.
- `Cargo.lock` pins Tauri 2.12.1, tauri-codegen 2.7.1 and Tao 0.37.1.
- The locally available tauri-codegen 2.7.1 source generates
  `default_window_icon_from_app_icon_resource()` on Windows. Tauri 2.12.1 loads
  the executable icon resource using `LoadImageW`, reads its color bitmap using
  `GetDIBits`, and converts BGRA to RGBA. Tao converts RGBA back into a Windows
  icon using `CreateIcon`. The installed executable's actual path through these
  operations has not yet been verified.
- The checked-in `src-tauri/icons/icon.ico` contains six 32-bit entries at
  16, 24, 32, 48, 64 and 256 pixels. Every entry starts with a PNG signature;
  offsets and lengths remain within the file. This does not establish that native
  decoding or the runtime conversion is correct.
- The assets were introduced with the existing signed-off Hubchat branding.
  No artwork changes are proposed.

## Next evidence

Compare native Windows decoding of the checked-in ICO and executable resource
at small sizes against the runtime RGBA-to-icon conversion. If necessary, inspect
existing window icons read-only after the requested reboot. A reproduction must
distinguish resource decoding, conversion, and window assignment before choosing
a minimal fix. App builds require a separate assignment; no install, restart,
cache clearing, or live data changes form part of this investigation.
