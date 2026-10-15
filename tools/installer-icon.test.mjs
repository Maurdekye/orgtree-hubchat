import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url)));
const template = readFileSync(new URL('../src-tauri/windows/installer.nsi', import.meta.url), 'utf8');

test('setup and uninstaller explicitly use the existing Hubchat app ICO', () => {
  for (const field of ['installerIcon', 'uninstallerIcon']) {
    assert.equal(config.bundle.windows.nsis[field], 'icons/icon.ico', field);
    assert.ok(config.bundle.icon.includes(config.bundle.windows.nsis[field]));
    const ico = readFileSync(new URL('../src-tauri/' + config.bundle.windows.nsis[field], import.meta.url));
    assert.equal(ico.readUInt32LE(0), 0x00010000, 'actual ICO header');
    assert.ok(ico.readUInt16LE(4) > 0, 'ICO contains frames');
  }
});

test('vendored wizard consumes configured icon paths before creating its pages', () => {
  for (const [variable, setting, macro] of [
    ['INSTALLERICON', 'installer_icon', 'MUI_ICON'],
    ['UNINSTALLERICON', 'uninstaller_icon', 'MUI_UNICON'],
  ]) {
    assert.ok(template.includes(`!define ${variable} "{{${setting}}}"`));
    const binding = `!define ${macro} "` + '${' + variable + '}"';
    const at = template.indexOf(binding);
    assert.ok(at >= 0 && at < template.indexOf('!insertmacro MUI_PAGE_WELCOME'), macro);
  }
});

test('Add/Remove Programs keeps the installed app resource as its display icon', () => {
  assert.ok(template.includes('WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\\"$INSTDIR\\${MAINBINARYNAME}.exe$\\""'));
});
