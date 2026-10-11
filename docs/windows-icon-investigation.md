# Windows child-process icon investigation

Status: offline conversion defect reproduced; native-resource workaround prepared
(2026-10-11). The changed Hubchat binary has not been built or launched.

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

## Measured offline reproduction

`tools/windows-icon-proof.py` loads the checked-in ICO and an optional executable
as a data/resource file (never executes it). It uses the same native APIs and
byte conversions as the pinned Tauri/Tao sources, and writes a comparison sheet
and JSON results. Run on Windows with Python and Pillow:

```powershell
python tools/windows-icon-proof.py --output <scratch-directory> --exe <existing-hubchat-executable>
```

For the checked-in ICO and an existing Hubchat 1.1.0 executable:

- Native decoding at 16, 20, 24, 32, 40, 48 and 64 pixels renders cleanly. The
  ICO and executable resources produce identical pixels at each size.
- Alpha-aware `DrawIconEx` drawing survives the Tauri/Tao conversion, including
  scaling 32-pixel icons to 16, 20 and 24 pixels.
- The native and converted icons have different AND masks. Tao builds one mask
  byte per RGBA pixel, but `CreateIcon` requires packed one-bit mask rows. Its
  converted mask contains unrelated white patterns.
- A mask-based 24-bit Windows image list shows white corruption for the converted
  icon; the native icon renders cleanly. A 32-bit alpha-aware image list is the
  control and renders both correctly.
- A hidden test window receives icons using `WM_SETICON` and returns them using
  `WM_GETICON`. Its converted small icon reproduces the image-list corruption.
  Replacing both small and big icons with native resource handles produces the
  expected clean icon. The test destroys all its windows, icons and GDI objects.

This establishes a defect in the runtime conversion and a working native
assignment at the Windows API boundary. The exact drawing path used by the
reported Task Manager instance has not been traced; matching that symptom and
verification in a newly built Hubchat remain integration checks.

## Source workaround

`src-tauri/src/windows_icon.rs` loads the same executable icon resource directly
at Windows' small and large system icon sizes and assigns both native handles to
the main window during setup. Managed app state retains the owned handles until
teardown. This avoids the faulty RGBA-to-icon conversion for window/shell icons
and preserves the existing artwork, executable resource and tray setup. Failure
is reported without preventing the app from starting.

No icon-cache clearing, install, restart, or live data changes were used. App
builds and live integration verification require a separate assignment.
