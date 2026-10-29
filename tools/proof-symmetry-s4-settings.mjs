// Symmetry S4 #31/#32/#46 (audit ux08 S3/S4/S5): the desktop settings section list is a tab list (selected state,
// arrow/Home/End), unsaved Profile edits survive a section switch until Save or close, and a boolean row toggles from
// its label and says on/off. Mounted real SettingsModal and Privacy section, synthetic API.
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, check, settle, click, render, finish, document, window} = await boot('symmetry-s4-settings-entry.tsx');
const {api, SettingsModal, SettingsSection, PlatformCtx, store, useState} = mod;
let me = {id: 'me', address: 'me.aaaaaa', name: 'Me', about: 'hello'}, receipts = false, receiptCalls = [], saved = [];
api.state = async () => ({me, hubs: [], platform: 'desktop', recovery_saved: true, read_receipts: receipts, file_background_hint: null});
api.chats = async () => []; api.directory = async () => []; api.transfers = async () => ({rows: [], next: null});
api.setProfile = async (name, about) => { saved.push([name, about]); me = {...me, name, about}; };
api.setReadReceipts = async v => { receiptCalls.push(v); receipts = v; };
await store.refreshAll();
const Host = () => { const [tab, setTab] = useState('profile'); return h(SettingsModal, {tab, setTab, onClose() {}}); };
const key = async (el, k, extra = {}) => { if (!el) return; await act(async () => { el.dispatchEvent(new window.KeyboardEvent('keydown', {key: k, bubbles: true, cancelable: true, ...extra})); }); await settle(); };
const tabs = () => [...document.querySelectorAll('[role="tab"]')];
const sel = () => tabs().filter(t => t.getAttribute('aria-selected') === 'true').map(t => t.textContent.trim());
const nameInput = () => document.getElementById('pf-name');
const type = async (el, v) => { await act(async () => { const set = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set; set.call(el, v); el.dispatchEvent(new window.Event('input', {bubbles: true})); }); await settle(); };
// the section list's buttons, tabs or not: #31 must run against the base too, where they are plain buttons
const tabNamed = label => [...document.querySelectorAll('.set-nav button')].find(t => t.textContent.trim().startsWith(label));

// ---- #32: the section list is a tab list
await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(Host)));
check('the section list is a tab list', !!document.querySelector('[role="tablist"]') && tabs().length >= 8);
check('exactly the open section is selected', JSON.stringify(sel()) === '["Profile"]');
check('the tab list has an accessible name', !!document.querySelector('[role="tablist"]')?.getAttribute('aria-label'));
const panel = document.getElementById(tabs()[0]?.getAttribute('aria-controls') || 'none');
check('the selected tab controls a tab panel that holds the section', panel?.getAttribute('role') === 'tabpanel' && panel.getAttribute('aria-labelledby') === tabs()[0].id);
check('one tab stop for the list (roving tabindex)', tabs().filter(t => t.tabIndex === 0).length === 1 && tabs()[0].tabIndex === 0);
await key(tabs()[0], 'ArrowDown');
check('ArrowDown selects and focuses the next section', JSON.stringify(sel()) === '["Hubs"]' && document.activeElement === tabNamed('Hubs'));
await key(tabNamed('Hubs'), 'ArrowUp'); await key(tabNamed('Profile'), 'ArrowUp');
check('ArrowUp from the first wraps to the last', JSON.stringify(sel()) === JSON.stringify([(tabs().at(-1)?.textContent.trim() ?? '')]));
await key(document.activeElement, 'Home');
check('Home selects the first section', JSON.stringify(sel()) === '["Profile"]' && document.activeElement === tabs()[0]);
await key(document.activeElement, 'End');
check('End selects the last section', JSON.stringify(sel()) === JSON.stringify([(tabs().at(-1)?.textContent.trim() ?? '')]));
await key(document.activeElement, 'Home'); await key(document.activeElement, 'ArrowDown', {ctrlKey: true});
check('a modified arrow does nothing', JSON.stringify(sel()) === '["Profile"]');

// ---- #31: unsaved Profile edits survive a section switch, until Save or close
await render(null); await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(Host)));
await type(nameInput(), 'Maya X');
await click(tabNamed('Hubs')); await click(tabNamed('Profile'));
check('an unsaved name is still there after another section', nameInput()?.value === 'Maya X');
check('Save is still on offer for it', !!([...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Save' && !b.disabled)));
await click([...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Save'));
await click(tabNamed('Hubs')); await click(tabNamed('Profile'));
check('Save kept the edit as the saved profile', JSON.stringify(saved) === '[["Maya X","hello"]]' && nameInput()?.value === 'Maya X');
await type(nameInput(), 'Never saved');
await render(null); await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(Host)));
check('closing Settings drops an unsaved edit', nameInput()?.value === 'Maya X');

// ---- #46: a boolean row toggles from its label and says on/off
for (const platform of ['desktop', 'android']) {
  const c = (name, ok) => check(platform + ': ' + name, ok);
  receipts = false; receiptCalls.length = 0; await store.refreshAll();
  await render(h(PlatformCtx.Provider, {value: platform}, h(SettingsSection, {tab: 'privacy'})));
  const sw = () => document.querySelector('[role="switch"][aria-label="Send read receipts"]');
  c('the switch is there', !!sw());
  c('the row says off', /\boff\b/.test((sw()?.parentElement?.textContent || '')));
  const title = [...document.querySelectorAll('.t1')].find(e => e.textContent.trim() === 'Send read receipts');
  await click(title);
  c('clicking the label toggles the switch', JSON.stringify(receiptCalls) === '[true]');
  c('the row now says on', /\bon\b/.test((sw()?.parentElement?.textContent || '')) && sw()?.getAttribute('aria-checked') === 'true');
  const hint = [...document.querySelectorAll('.t2')].find(e => /Off: people and agents/.test(e.textContent));
  await click(hint);
  c('clicking the hint toggles it back', JSON.stringify(receiptCalls) === '[true,false]');
  c('the row is not a second button around the switch', !sw()?.closest('[role="button"]'));
}
await finish('Mounted desktop SettingsModal and desktop/Android Privacy rows with synthetic API; no native or pixel acceptance');
