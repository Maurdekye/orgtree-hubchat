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
reads its title-bar icon via WM_GETICON or the native class-icon fallback and
attempts a separate window PNG capture,
then closes without advancing installation. It checks the test ProductName,
requires absent disposable install/uninstall keys, tracks descendants with
stable process handles and lifetime checks, and records any forced cleanup.
It sends only Close and Yes-to-quit, never Next. Run in the rig's outer Job.
Add/Remove Programs routing is source-checked against
the bundled app resource; do not install or create an uninstall registry key
to exercise the control-panel entry.

## Measured contained proof (2026-10-15)

Rig-sol-b built the contained setup through the real Tauri NSIS bundler:
green exits0 in6s. Final proof helper source is `1b54f69`, with the same
two-field product configuration as `25b88bd`. Red control `9d086a3` removes
only those settings to restore the base configuration; instrumentation and
the app-icon stand-in payload remain the same.

Native resource comparisons pass6/6: setup, exported uninstaller and stand-in
app each match the existing app ICO at16 and32px. The old-config control
fails the four setup/uninstaller comparisons while both app controls pass.
The corresponding shell icon PNGs were viewed by rig-sol-b. Its initial
ExtractIconEx count assertion was a helper defect (returned2 with both
handles valid); the corrected positive-count/both-handles check preserves
the exact pixel comparisons and cleanup.

The actual disposable Welcome window is identified by its title and real
controls. Its `GCLP_HICONSM` native small-icon binding matches the app's16px
pixels exactly (SHA256 prefix `c94ba9cb`). The old-config Welcome icon differs
(`d0b9f8b`), showing the default NSIS icon. This measures the native class-icon
fallback, not a nonzero WM_GETICON response. Author and rig viewed the four
icon comparison PNGs.

The separate full-window capture **fails**. In Session0, IsWindowVisible is
false despite WS_VISIBLE=true (`0x94ca084c`). PrintWindow flags2 returns true
but yields a503x390, single-color black frame, saved as evidence and viewed
by author and rig. The green helper therefore exits1 solely on the retained
blank-capture assertion after recording the matching icon; it is not an
all-tests-green result. The red helper exits1 on the expected native-icon
mismatch before capture. Both runs close by Cancel with setup exit1, no
forced termination, no remaining processes, and disposable install/uninstall
keys absent before and after.

Raw receipts live under the rig's `batch-installer-icon/logs` packet and its
`installer-icon` evidence: `resources-green-3`, `resources-red-1`,
`welcome-green3` and `welcome-red`, each with raw JSON and PNGs. These prove
contained PE/shell resources and the actual Welcome native small-icon binding.
Full wizard rendering, Explorer UI and the installed Add/Remove Programs UI
remain unmeasured. The ARP DisplayIcon routing is source-checked against the
correct app resource; no install or registry write was performed. No production
installer, release, installation or publication result is claimed.
