// A scripted chat partner for device tests on a scratch hub, kept across
// runs (tools/peer2.json): registers once, then sends text or an image.
// PEER=<name> is another partner, with its own identity (tools/peer2-<name>.json).
// PEER_STATE=<file> keeps the identity in that file instead, for a runner that
// must not write into the checkout.
//   node peer2.mjs <hub> whoami
//   node peer2.mjs <hub> send <to-address> "<text>" [image-file]
//   node peer2.mjs <hub> recv <from-address> [timeout-s]   the files of <from>'s next message with files:
//                                                          each one's name, size and SHA-256, as one JSON line
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { randomBytes, createHash, randomUUID } from 'node:crypto';
import { basename } from 'node:path';

const NAME = process.env.PEER || 'Pat Peer';
const STATE = process.env.PEER_STATE || new URL(process.env.PEER ? `./peer2-${NAME.replace(/[^A-Za-z0-9]+/g, '-').toLowerCase()}.json` : './peer2.json', import.meta.url);
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
await post('/api/register', JSON.stringify({ slug: me.slug, kind: 'person', username: NAME, org_name: NAME }), { 'Content-Type': 'application/json' });
if (cmd === 'whoami') { console.log(me.slug); process.exit(0); }
if (cmd === 'recv') {
  const json = { 'Content-Type': 'application/json' };
  const end = Date.now() + 1000 * Number(text || 40), received = [];
  while (Date.now() < end && !received.length) {
    const p = await post('/api/poll?wait=5', '{}', json);
    for (const m of p.messages) {
      if (m.from !== to) continue;
      for (const a of m.attachments || []) {
        const r = await fetch(hub + '/api/attachments/' + a.id, { headers: auth });
        const data = Buffer.from(await r.arrayBuffer());
        received.push({ name: a.name, bytes: data.length, sha256: createHash('sha256').update(data).digest('hex'), status: r.status });
      }
    }
    if (p.messages.length) await post('/api/ack', JSON.stringify({ ids: p.messages.map((m) => m.id) }), json);
  }
  console.log(JSON.stringify({ received }));
}
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
