import fs from 'node:fs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
test('Directory rows have no redundant Message/Open chat button or button-only CSS', () => {
  const source=fs.readFileSync(new URL('../src/components/Directory.tsx',import.meta.url),'utf8');
  const css=fs.readFileSync(new URL('../src/styles/desktop.css',import.meta.url),'utf8');
  assert.ok(!source.includes('className="btn"'), 'no per-row message/open-chat button');
  assert.ok(!css.includes('.dir-row .r .btn'), 'remove obsolete row-button CSS');
});
