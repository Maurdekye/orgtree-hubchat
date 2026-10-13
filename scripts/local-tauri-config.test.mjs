import test from 'node:test';
import assert from 'node:assert/strict';
import {windowsFrontendDirectory} from './local-tauri-config.mjs';

test('forward-slash drive paths cannot silently become an external frontend URL', () => {
  assert.equal(new URL('C:/build/frontend').protocol, 'c:');
  const fixed = windowsFrontendDirectory('C:/build/frontend');
  assert.equal(fixed, '\\\\?\\C:\\build\\frontend');
  assert.throws(() => new URL(fixed));
});
test('backslash and spaced drive directories retain their directory identity', () => {
  assert.equal(windowsFrontendDirectory('D:\\build space\\frontend'), '\\\\?\\D:\\build space\\frontend');
});
test('URLs and drive-relative values are refused', () => {
  for (const input of ['https://example.invalid', 'file:///C:/build', 'C:build', '../dist']) assert.throws(() => windowsFrontendDirectory(input));
});
