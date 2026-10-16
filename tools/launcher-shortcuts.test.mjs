// Small source contract controls complement native SQLite and Kotlin checks.
import {readFileSync, existsSync} from 'node:fs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
const root = new URL('../', import.meta.url);
const read = p => readFileSync(new URL(p, root), 'utf8');
test('Windows publishes native Jump List tasks with direct executable activation', () => {
  const file = 'src-tauri/src/jumplist.rs';
  assert.ok(existsSync(new URL(file, root)), 'native Jump List publisher exists');
  const src = read(file);
  for (const part of ['ICustomDestinationList', 'SetAppID', 'AddUserTasks', 'CommitList', 'SetArguments']) assert.ok(src.includes(part), part);
});
test('Android publishes four dynamic contacts, no desktop menu actions', () => {
  const file = 'src-tauri/gen/android/app/src/main/java/dev/orgtree/hubchat/LauncherShortcuts.kt';
  assert.ok(existsSync(new URL(file, root)), 'Android shortcut publisher exists');
  const src = read(file);
  for (const part of ['ShortcutManager', 'ShortcutInfo.Builder', 'setDynamicShortcuts', 'MainActivity::class.java', '"owner"', 'take(4)']) assert.ok(src.includes(part), part);
  assert.ok(!/"Settings"|"Directory"|"New chat"/.test(src));
});
test('shortcut view links are consumed before device linking', () => {
  const src = read('src/components/Desktop.tsx');
  assert.ok(src.indexOf('shortcutView(input)') >= 0);
  assert.ok(src.indexOf('shortcutView(input)') < src.indexOf('routeLink(input, "approve")'));
});
