import {test} from 'node:test';
import assert from 'node:assert/strict';
import {chatLinkTarget, shortcutView} from '../src/lib/chatlink.ts';

test('native menu views are exact and do not accept arbitrary routes', () => {
  for (const view of ['settings', 'directory', 'newchat']) assert.equal(shortcutView('hubchat://view?name='+view), view);
  for (const link of ['https://view?name=settings','hubchat://chat?name=settings','hubchat://view/else?name=settings','hubchat://view?name=unknown']) assert.equal(shortcutView(link), null);
});
test('old account shortcuts cannot open their contact under another account', () => {
  const link = 'hubchat://chat?to=peer.123456&owner=me.123456';
  assert.equal(chatLinkTarget(link, 'me.123456'), 'peer.123456');
  assert.equal(chatLinkTarget(link, 'other.123456'), '');
  assert.equal(chatLinkTarget('hubchat://chat?to=peer.123456','other.123456'), 'peer.123456');
});
