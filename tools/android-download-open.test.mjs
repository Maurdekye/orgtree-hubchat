// Source wiring regression. Runtime intent behavior is covered separately by
// DownloadOpenerTest on Android; this does not claim to launch a viewer.
import {readFileSync, existsSync} from 'node:fs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
const read = p => readFileSync(new URL('../' + p, import.meta.url), 'utf8');
const native = 'src-tauri/gen/android/app/src/main/';
test('download command selects Android bridge before desktop opener', () => {
  const fn = read('src-tauri/src/commands.rs').split('pub fn hc_open_attachment')[1].split('/// The recovery')[0];
  assert.match(fn, /#\[cfg\(target_os = "android"\)\][\s\S]*crate::android::open_download\(&path, &a.name\)/);
  assert.match(fn, /#\[cfg\(not\(target_os = "android"\)\)\]/);
});
test('JNI bridge reaches the Android file opener and propagates errors', () => {
  assert.match(read('src-tauri/src/android.rs'), /pub fn open_download[\s\S]*"openDownload"/);
  assert.match(read(native+'java/dev/orgtree/hubchat/ConnectionService.kt'), /fun openDownload[\s\S]*DownloadOpener.open/);
});
test('Android opener grants content access and explicitly types APKs', () => {
  const p = native+'java/dev/orgtree/hubchat/DownloadOpener.kt';
  assert.ok(existsSync(new URL('../'+p, import.meta.url)), 'Android download opener is present');
  const kt = read(p);
  for (const contract of ['Intent.ACTION_VIEW','FLAG_GRANT_READ_URI_PERMISSION','setDataAndType','application/vnd.android.package-archive','FileProvider.getUriForFile','context.packageName','ActivityNotFoundException'])
    assert.ok(kt.includes(contract), 'native contract: '+contract);
});
test('real and TEST application IDs share install permission and isolated provider authority', () => {
  const manifest=read(native+'AndroidManifest.xml');
  assert.match(manifest, /android.permission.REQUEST_INSTALL_PACKAGES/);
  assert.match(manifest, /android:authorities="\$\{applicationId\}\.fileprovider"/);
  assert.match(manifest, /android:grantUriPermissions="true"/);
  assert.match(read('src-tauri/gen/android/app/build.gradle.kts'), /applicationIdSuffix = "\.test"/);
});
test('private pre-Android29 download directory can be shared through FileProvider', () => {
  assert.match(read(native+'res/xml/file_paths.xml'), /<files-path[^>]*path="downloads\/"/);
});
