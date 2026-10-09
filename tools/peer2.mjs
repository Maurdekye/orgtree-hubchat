// A scripted chat partner for device tests on a scratch hub, kept across
// runs (tools/peer2.json): registers once, then sends text or an image.
//   node peer2.mjs <hub> whoami
//   node peer2.mjs <hub> send <to-address> "<text>" [image-file]
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { randomBytes, createHash, randomUUID } from 'node:crypto';
import { basename } from 'node:path';

const STATE = new URL('./peer2.json', import.meta.url);
const [hub, cmd, to, text, image] = process.argv.slice(2).map((a, i) => (i === 0 ? a.replace(/\/$/, '') : a));
let me = existsSync(STATE) ? JSON.parse(readFileSync(STATE, 'utf8')) : null;
if (!me) {
  const secret = randomBytes(16).toString('hex');
  me = { secret, slug: 'testpeer.' + createHash('sha256').update(secret).digest('hex').slice(0, 6) };
  writeFileSync(STATE, JSON.stringify(me));
}
const auth = { 'X-Org-Auth': `${me.slug}:${me.secret}` };
const post = async (path, body, headers = {}) => {
  const r = await fetch(hub + path, { method: 'POST', headers: { ...auth, ...headers }, body });
  if (!r.ok) throw new Error(path + ' ' + r.status + ' ' + (await r.text()));
  return r.json();
};
await post('/api/register', JSON.stringify({ slug: me.slug, kind: 'person', username: 'Pat Peer', org_name: 'Pat Peer' }), { 'Content-Type': 'application/json' });
if (cmd === 'whoami') { console.log(me.slug); process.exit(0); }
if (cmd === 'send') {
  const attachments = [];
  if (image) {
    const data = readFileSync(image);
    const a = await post('/api/attachments?name=' + encodeURIComponent(basename(image)), data, { 'Content-Type': 'application/octet-stream', 'Content-Length': String(data.length) });
    attachments.push(a.id);
  }
  const id = randomUUID().replace(/-/g, '');
  await post('/api/send', JSON.stringify({ from: me.slug, id, to, body: text || '', attachments }), { 'Content-Type': 'application/json' });
  console.log(JSON.stringify({ sent: id, from: me.slug }));
}
