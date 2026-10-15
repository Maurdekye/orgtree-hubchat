# Windows installer icon

The app's existing bundle icon list does not select the NSIS installer icon.
Set both `bundle.windows.nsis.installerIcon` and `uninstallerIcon` to the
existing `icons/icon.ico`. The vendored template already binds those paths
to `MUI_ICON` and `MUI_UNICON` before creating the wizard pages. No template,
runtime, install-flow or artwork change is needed.

Add/Remove Programs already uses `"$INSTDIR\${MAINBINARYNAME}.exe"` as its
DisplayIcon; the main executable continues to use the existing bundle ICO.
Coordinator 2026-10-15 10:09Z ruled to retain stock header/sidebar artwork.
Those options require BMPs and are separate from the requested icon surfaces.

## Verification

Author wrote and committed the regression before changing config:
`node --test tools/installer-icon.test.mjs` measured2 passes and1 expected
failure on missing installerIcon at `8833176`. With the two config fields,
the same suite measures3 passes,0 failures/skips/cancellations. This checks
source wiring and valid existing ICO input; it is not a built-resource claim.

Run contained packaging only under the shared rig lock/Job, with an own C
output directory and existing dependencies read in place:

```
node tools/proof-installer-icon.mjs <new-own-C-output> <existing-tauri.js>
python tools/installer-icon-resources.py --output <new-own-C-resource-output> --exe <setup.exe> --exe <captured-uninstaller.exe> --exe <standin-hubchat.exe>
```

The build helper uses the real Tauri `bundle` command and exact vendored
template. It compiles a never-run C# stand-in payload with the same existing
ICO and packages it under `Hubchat Icon Test` / a `.test` identifier.
It performs no Cargo, frontend or application build. A test-only custom signing
callback copies the generated uninstaller for offline resource inspection;
it does not sign or execute it. The override supplies neither icon setting,
so the checked-in settings remain the inputs under test. Compare a base
config with those two settings absent using the identical helper.

The resource helper loads PEs only as data, extracts RT_GROUP_ICON/RT_ICON
into standalone ICOs and compares Windows shell-extracted16/32px pixels with
native decoding of the source app ICO. Preserve red and green resource
receipts. `tools/installer-icon-welcome.py --setup <contained-setup.exe>
--output <new-own-C-output>` opens only the disposable setup Welcome page,
captures its title-bar icon via WM_GETICON/native pixels and the window PNG,
then closes without advancing installation. It checks the test ProductName,
requires absent disposable install/uninstall keys, tracks descendants with
stable process handles and lifetime checks, and records any forced cleanup.
It sends only Close and Yes-to-quit, never Next. Run in the rig's outer Job.
Add/Remove Programs routing is source-checked against
the bundled app resource; do not install or create an uninstall registry key
to exercise the control-panel entry.

Rig-sol-b's first contained NSIS green bundle passed in6s. Its first offline
helper stopped before pixel comparisons: ExtractIconEx returned2 with both
valid handles, while the helper incorrectly required1. The helper correction
requires a positive count and both handles, retaining exact pixel comparisons
and cleanup. Resource and Welcome pixel results remain pending rig-sol-b.
No Explorer UI, installed-program entry, production installer,
installation or release result is claimed by the source tests.
