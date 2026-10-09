// In-memory stand-in for the Rust core, used when the UI runs in a plain
// browser (`npm run dev`). Same shapes as src/api.ts; a believable world:
// two hubs (one down and retrying), an Orgtree org, an agent session,
// people, markdown, a file, a failed message and climbing receipts.
// URL parameters: ?platform=android  ?onboarding=1  ?update=1 (an update is
// out)  ?scan=TEXT (what the fake camera reads)  ?pending=ADDRESS (a tapped
// notification)  ?link=TEXT (the hubchat:// link Android opened the app with)
// ?approve=S (the other device approves a link after S seconds)  ?lab=up (the
// lab hub starts connected: the hubchat-ui session is then on two hubs, for
// the hub picker). Pat Peer's chat is lazy history: this device holds only
// its newest messages; older ones load from office as you scroll back, and
// the lab hub's (down unless ?lab=up) fill in when it is back (?labback=S:
// it comes back after S seconds).
// Scan setup code: ?setupfail=update|damaged|no-ts|unreach|unreach-wifi|join
// fails that check; ?setupreply=expired|none (the org's answer; default
// linked, after 2.5 s). ?dooroff=1: Link a device's code says phone access
// is off on this PC.
// Linking: a waiting device shows up on the third lookup; a device joining a
// code is approved after 45 s and reviews LINK_HUBS (a signed-in device
// joining one: after 2 s the link turns out to be another identity,
// maya.e71f2b, or with a code starting "SAME" this device's own); nothing is
// adopted before linkConfirm. A key file opens with any passphrase but
// "wrong". Hubs on 127.0.0.1 / localhost can't be reached from the "phone".
import type { Api, SetupLink, SetupState, Attachment, ChatSummary, SendRoute, Contact, HcEvent, HubRow, HubStatus, LinkEvent, LinkLookup, LinkRole, Message, NewOutgoing, OlderPage, ParsedLink, Probe, Resolved, State } from "../api";
import { toast } from "./toast";

const params = new URLSearchParams(location.search);
const GB = 1073741824;
const iso = (t: number) => new Date(t).toISOString();
const now = () => Date.now();
const uid = () => Math.random().toString(16).slice(2, 10) + Math.random().toString(16).slice(2, 10);
const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
const sleep = (n: number) => new Promise((r) => setTimeout(r, n));

/** A time `days` ago at hh:mm local. */
function at(days: number, hm: string): number {
  const d = new Date(); d.setDate(d.getDate() - days);
  const [h, m] = hm.split(":").map(Number); d.setHours(h, m, 0, 0);
  return Math.min(d.getTime(), now() - 60000);
}

const OFFICE = "http://hub.office.lan:7370";
const LAB = "http://10.0.0.7:7370";
const onboarding = params.get("onboarding") === "1";

const st: State = {
  me: onboarding ? null : { id: "alex", address: "alex.3be2c9", name: "Alex Rivera", about: "Platform team" },
  recovery_saved: false,
  read_receipts: true,
  notifications: { enabled: true, preview: true, sound: false },
  stay_connected: new URLSearchParams(location.search).get("platform") === "android" ? true : null,
  device_name: new URLSearchParams(location.search).get("platform") === "android" ? "Pixel 8" : "Home-PC",
  platform: params.get("platform") === "android" ? "android" : "desktop",
  hubs: onboarding ? [] : [
    { url: OFFICE, name: "office", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: GB, features: ["v2"], version: "1.4.0" },
    params.get("lab") === "up"
      ? { url: LAB, name: "lab", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 25 * 1048576, features: [] }
      : { url: LAB, name: "lab", state: "disconnected", error: "connection refused (os error 10061)", retry_at_ms: now() + 14000, max_attachment_bytes: 25 * 1048576, features: [] },
  ],
};

function contact(address: string, kind: string, name: { org?: string; user?: string }, blurb: string, online: boolean, seen: number | null, hubs: string[]): Contact {
  return { address, kind, org_name: name.org || "", username: name.user || "", blurb, online, last_seen: seen ? iso(seen) : null, hubs };
}
const SEED_DIR: Contact[] = [
  contact("orgtree.alex.7c41d2", "org", { org: "Orgtree" }, "Alex's Orgtree: the platform org", true, now() - 30000, [OFFICE]),
  contact("hubchat-ui.alex.a3f9c1", "chat", { user: "hubchat-ui" }, "Claude Code · C:\\Users\\alex\\code\\hubchat", true, now() - 10000, [OFFICE, LAB]),
  contact("maya.e71f2b", "person", { user: "Maya Lin" }, "Design · Android", false, now() - 3 * 3600e3, [OFFICE]),
  contact("jonas.0b44d1", "person", { user: "Jonas Park" }, "Runs the lab hub", false, now() - 26 * 3600e3, [LAB]),
  contact("research.alex.5b0e9a", "org", { org: "Research" }, "Literature and benchmarks", false, now() - 2 * 864e5, [OFFICE]),
  contact("nightly-ci.alex.8d21e0", "chat", { user: "nightly-ci" }, "Builds every night at 02:00", true, now() - 5000, [OFFICE]),
  contact("kim.19ac02", "person", { user: "Kim Okafor" }, "", false, now() - 12 * 864e5, [OFFICE]),
  contact("pat.4d5e6f", "person", { user: "Pat Peer" }, "Started on this device today", true, now() - 60000, [OFFICE, LAB]),
  // devices' throwaway link addresses (src-tauri/src/link.rs registers them): a
  // leftover and one waiting now; the Directory and New chat leave both out
  contact("link.19356f", "chat", { org: "Pixel 7a", user: "link" }, "Waiting to be linked to a Hubchat identity", false, now() - 5 * 3600e3, [OFFICE]),
  contact("link.8b02aa", "chat", { org: "New device", user: "link" }, "Waiting to be linked to a Hubchat identity", true, now() - 20000, [OFFICE]),
  // a real person whose id is "link": listed
  contact("link.c40e17", "person", { user: "Lincoln Kerr" }, "Goes by Link · hardware lab", false, now() - 4 * 864e5, [LAB]),
];
let dir: Contact[] = onboarding ? [] : clone(SEED_DIR);

const msgs: Message[] = [];
const drafts: Record<string, string> = {};
/** Lazy history: per chat, per hub, the messages this device hasn't loaded
 *  (oldest first), and how many of them are unread. */
const hubOld = new Map<string, Map<string, Message[]>>();
const oldUnread = new Map<string, number>();
/** How far back a chat may show (no pop-ins); it never rises again. */
const shownFloor = new Map<string, string>();

function att(name: string, b: number, state: string, extra?: Partial<Attachment>): Attachment {
  return { local_id: uid(), hub_id: "h" + uid().slice(0, 6), name, bytes: b, source: null, local_path: null, state, error: null, ...extra };
}
function add(peer: string, outgoing: boolean, t: number, body: string, o: Partial<Message> = {}): Message {
  const m: Message = {
    id: uid(), peer, outgoing, hub: OFFICE, body, kind: null, reply_to: null,
    sent_at: iso(t), received_at: iso(t + 400), created_at: iso(t), state: outgoing ? "read" : "received",
    fetched_at: outgoing ? iso(t + 2000) : null, delivered_at: outgoing ? iso(t + 2600) : null, read_at: outgoing ? iso(t + 60000) : o.seen === false ? null : iso(t + 30000),
    error: null, seen: true, attachments: [], ...o,
  };
  msgs.push(m);
  return m;
}

const LONG_REVIEW = `## Review: transfer code in \`engine.rs\`

Overall the structure is sound. A few things worth fixing before rc4:

1. **Upload retries restart from zero.** The hub can't resume, so that is expected, but the UI should say so; right now the progress bar jumps back without a word.
2. **Cancel races the final POST.** If the user cancels after the last chunk is written but before \`/api/send\`, the message is still sent. Check the cancel flag once more right before sending.
3. **Progress events are too chatty.** Every 64 KB chunk emits an event; on a 1 GB file that is 16,384 events. Throttle to ~10 per second.

| Area | Severity | Suggested fix |
| --- | --- | --- |
| Retry from zero | low | copy in the failed line |
| Cancel race | **high** | re-check before send |
| Event volume | medium | throttle to 100 ms |

\`\`\`rust
// before sending, after the last upload
if self.cancelled(&msg.id) {
    return Err(Error::Cancelled);
}
\`\`\`

Smaller notes:
- \`download()\` writes straight into the final path; write to \`.part\` and rename when done, so a crash never leaves a half file that looks complete.
- The backoff constant (8, 16, 32 s) matches the design. Good.
- \`hubs_reaching\` orders online hubs first: good, keep it.
- Consider logging the hub's \`max_attachment_bytes\` once per connect; it helps when someone asks why a file was refused.

I ran the test suite locally: 212 passed, 0 failed. The two ignored tests are the network ones that need a live hub.

Let me know if you want me to make these changes on a branch; I can have them ready in about twenty minutes.`;

if (!onboarding) {
  const OG = "orgtree.alex.7c41d2";
  add(OG, false, at(1, "17:40"), "rc3 is ready: **1,286 tests passed**, 0 failed.\n\n- installer signed\n- Android build green\n- one flaky test quarantined (`net::retry_backoff`)", { kind: "status" });
  add(OG, true, at(1, "17:42"), "Great. Ship it to the beta channel.");
  const q = add(OG, false, at(0, "09:12"), "Keep the old log location as a fallback, or switch cleanly?", { kind: "question" });
  add(OG, true, at(0, "09:15"), "Switch cleanly, and put it in the release notes.", { reply_to: q.id });
  add(OG, false, at(0, "09:20"), "Done. Here is the change:\n```diff\n- let dir = legacy_log_dir();\n+ let dir = app_log_dir();\n```\nSummary attached.", { attachments: [att("rc3-test-summary.txt", 24 * 1024, "remote")] });
  add(OG, true, at(0, "09:31"), "Thanks! Can you also check the updater manifest?", { state: "delivered", read_at: null });

  const CC = "hubchat-ui.alex.a3f9c1";
  add(CC, true, at(0, "08:02"), "Please review the transfer code in engine.rs", { attachments: [att("engine.rs", 48 * 1024, "uploaded", { source: "E:\\src\\engine.rs" })] });
  add(CC, false, at(0, "08:09"), LONG_REVIEW, { seen: false });
  add(CC, false, at(0, "08:10"), "Also: @net:research.alex.5b0e9a has benchmark numbers for the throttle, if you want a second opinion.", { seen: false });

  const MAYA = "maya.e71f2b";
  add(MAYA, false, at(1, "16:05"), "Are we still on for the demo on Friday?");
  add(MAYA, true, at(1, "16:20"), "Yes, 14:00 in the lab.");
  add(MAYA, true, at(0, "10:02"), "I'll bring the Pixel for the Android build.", { state: "sent", fetched_at: null, delivered_at: null, read_at: null });

  add("jonas.0b44d1", true, at(2, "11:30"), "Can you send me the hub logs from the lab?", {
    state: "failed", hub: null, received_at: null, sent_at: null, fetched_at: null, delivered_at: null, read_at: null,
    error: "hub lab is unreachable and no other hub lists jonas.0b44d1",
  });
  add("nightly-ci.alex.8d21e0", false, at(3, "02:14"), "Nightly build **#418** passed in 41 min.", { kind: "status" });
  // a download already in progress (the transfers chip / strip)
  const big = add("nightly-ci.alex.8d21e0", false, at(0, "02:16"), "Full logs for #418.", { attachments: [att("nightly-logs-418.zip", 182 * 1048576, "downloading")] });
  setTimeout(() => { void slowDownload(big); }, 1200);

  // images (user 23:29Z): a received screenshot, a sent photo, one too big
  // for a preview, one the hub no longer has
  const KIM = "kim.19ac02";
  add(KIM, false, at(0, "07:40"), "The new onboarding, on my phone:", { attachments: [att("Screenshot_onboarding.png", 412 * 1024, "remote")] });
  add(KIM, true, at(0, "07:44"), "", { attachments: [att("desk-photo.jpg", 2 * 1048576, "uploaded", { source: "C:\\Users\\alex\\Pictures\\desk-photo.jpg" })] });
  add(KIM, false, at(0, "07:50"), "And the full-resolution scan, if you need it.", { attachments: [att("poster-scan.png", 46 * 1048576, "remote")] });
  add(KIM, false, at(2, "18:02"), "Old one", { attachments: [att("whiteboard.jpg", 900 * 1024, "expired")] });

  // a long chat, for lazy history (user 23:46Z): 150 lab logs, the newest
  // message replying to the very first
  const JONAS = "jonas.0b44d1";
  const t0 = Date.now() - 6 * 864e5;
  let firstLog: Message | null = null;
  for (let i = 1; i <= 150; i++) {
    const m = add(JONAS, i % 3 === 0, t0 + i * 45 * 60000, "Lab log " + i + (i % 10 === 0 ? ": rack temperature normal, every hub green." : "."));
    if (i === 1) firstLog = m;
  }
  add(JONAS, false, Date.now() - 3 * 60000, "About that first log entry: can you check it again?", { reply_to: firstLog!.id });

  // lazy history: this device started today; office holds 70 older
  // messages of Pat's chat, the lab hub 12 (two of them office's too)
  const PAT = "pat.4d5e6f";
  const p0 = Date.now() - 4 * 864e5;
  const office: Message[] = [], lab: Message[] = [];
  for (let i = 1; i <= 80; i++) {
    const m = add(PAT, i % 4 === 0, p0 + i * 55 * 60000, "Pat's note " + i + (i % 9 === 0 ? ": the release checklist is up to date." : "."), { seen: i < 74, read_at: i < 74 ? iso(p0 + i * 55 * 60000 + 30000) : null });
    msgs.pop();
    if (i % 8 === 3) { lab.push({ ...m, hub: LAB }); if (i === 19 || i === 51) office.push(m); }
    else office.push(m);
  }
  hubOld.set(PAT, new Map([[OFFICE, office], [LAB, lab]]));
  oldUnread.set(PAT, [...office, ...lab].filter((m, i, a) => !m.outgoing && !m.seen && a.findIndex((x) => x.id === m.id) === i).length);
  add(PAT, false, at(0, "08:10"), "Morning! Did the new phone pick everything up?", { seen: false, read_at: null });
  add(PAT, true, at(0, "08:12"), "Only today's messages so far. The rest loads as I scroll back.");
}

/** The time a chat may show back to (none: everything): what every
 *  reachable hub that still has older messages has loaded down to. */
function floorOf(peer: string): string | null {
  let f: string | null = null;
  for (const [hub, list] of hubOld.get(peer) || []) {
    const h = st.hubs.find((x) => x.url === hub);
    if (!list.length || !h || h.state === "disconnected") continue;
    const t = list[list.length - 1].created_at;
    if (!f || t > f) f = t;
  }
  // "" once everything showed: it stays so (a hub back doesn't hide any)
  const shown = shownFloor.get(peer);
  if (shown === "") return null;
  if (f === null) shownFloor.set(peer, "");
  else if (!shown || f < shown) shownFloor.set(peer, f);
  else f = shown;
  return f;
}

/** A made-up picture for an image attachment (the mock has no files). */
const pictures = new Map<string, Promise<ArrayBuffer>>();
function picture(name: string, outgoing: boolean): Promise<ArrayBuffer> {
  let p = pictures.get(name);
  if (!p) {
    p = new Promise<ArrayBuffer>((ok, bad) => {
      const tall = /screenshot/i.test(name);
      const c = document.createElement("canvas");
      c.width = tall ? 540 : 960; c.height = tall ? 1170 : 640;
      const g = c.getContext("2d")!;
      const grad = g.createLinearGradient(0, 0, c.width, c.height);
      grad.addColorStop(0, outgoing ? "#c86b4c" : "#3b6fb5"); grad.addColorStop(1, "#1d2230");
      g.fillStyle = grad; g.fillRect(0, 0, c.width, c.height);
      g.fillStyle = "rgba(255,255,255,.85)"; g.font = "bold 44px sans-serif"; g.fillText(name, 32, 80);
      g.strokeStyle = "rgba(255,255,255,.35)"; g.lineWidth = 6; g.strokeRect(24, 120, c.width - 48, c.height - 150);
      c.toBlob((b) => (b ? b.arrayBuffer().then(ok, bad) : bad("no picture")), "image/png");
    });
    pictures.set(name, p);
  }
  return p;
}
/** The acted-out Android update (appUpdate*). */
let updateAllowed = false;
let updateState = "";
const updateProgress = new Set<(done: number, total: number) => void>();

/** Pasted images by the path savePasted gave them. */
const pasted = new Map<string, Uint8Array>();

// ------------------------------------------------------------------ events
const listeners = new Set<(e: HcEvent) => void>();
const emit = (e: HcEvent) => listeners.forEach((f) => f(e));
const chatEv = (peer: string) => emit({ type: "chat", peer });
const hubEv = (url: string) => emit({ type: "hub", url });

// ?labback=S: the lab hub comes back after S seconds (lazy history: its
// older messages then fill in)
const labBack = Number(params.get("labback")) || 0;
const labUpAt = labBack ? now() + labBack * 1000 : Infinity;
// the lab hub keeps failing; its countdown runs and it retries on schedule
setInterval(() => {
  for (const h of st.hubs) {
    if (h.state === "disconnected" && h.retry_at_ms && h.retry_at_ms <= now()) reconnect(h);
  }
}, 1000);
function reconnect(h: HubStatus) {
  h.state = "connecting"; h.retry_at_ms = null; hubEv(h.url);
  setTimeout(() => {
    if (/10\.0\.0\.7|unreach|10\.0\.9\./.test(h.url) && !(h.url === LAB && now() >= labUpAt)) { h.state = "disconnected"; h.error = "connection refused (os error 10061)"; h.retry_at_ms = now() + 16000; }
    else { h.state = "connected"; h.error = null; }
    hubEv(h.url);
  }, 1100);
}

/** A download that creeps along (about 4 minutes), so the transfers UI shows. */
async function slowDownload(m: Message) {
  const a = m.attachments[0];
  chatEv(m.peer);
  let done = Math.round(a.bytes * 0.38);
  while (done < a.bytes) {
    if (a.state !== "downloading") return;
    emit({ type: "transfer", local_id: a.local_id, message_id: m.id, upload: false, done, total: a.bytes });
    await sleep(1000);
    done = Math.min(a.bytes, done + Math.round(a.bytes / 400));
  }
  a.state = "done"; a.local_path = "C:\\Users\\alex\\Downloads\\" + a.name; chatEv(m.peer);
}

// --------------------------------------------------------------- linking
const linkListeners = new Set<(e: LinkEvent) => void>();
const linkEmit = (e: LinkEvent) => linkListeners.forEach((f) => f(e));
let linkRun = 0;
const lookups = new Map<string, number>();
const KEY_QR = "hubchat-key:1:" + "Qm9vdHN0cmFwIGtleSBidW5kbGUgZm9yIGFsZXguM2JlMmM5IHdpdGggaHVicyBvZmZpY2UgYW5kIGxhYg".repeat(3);

/** An identity arrived on this (onboarding) device, with the office hub. */
function adopt(withHubs: boolean): string {
  st.me = { id: "alex", address: "alex.3be2c9", name: "Alex Rivera", about: "Platform team" };
  st.recovery_saved = true;
  if (withHubs && !st.hubs.some((h) => h.url === OFFICE)) {
    const h: HubStatus = { url: OFFICE, name: "office", state: "connecting", error: null, retry_at_ms: null, max_attachment_bytes: GB, features: ["v2"] };
    st.hubs.push(h); reconnect(h);
    dir = clone(SEED_DIR).map((c) => ({ ...c, hubs: [OFFICE] }));
    setTimeout(() => emit({ type: "directory" }), 300);
  }
  return st.me.address;
}
function parseLink(input: string): ParsedLink {
  let t = input.trim(); let hub: string | null = null; let role: LinkRole | null = null; let hubs: string[] = []; let hubName: string | null = null;
  const m = /^hubchat-link:([^@]+)@(.+)$/i.exec(t);
  if (/^hubchat-link:/i.test(t) && !m) throw "damaged link QR code";
  if (m) { t = m[1]; hub = m[2]; hubs = [m[2]]; role = "take"; }
  else if (/^hubchat:\/\//i.test(t)) {
    let u: URL;
    try { u = new URL(t); } catch { throw "damaged link QR code"; }
    const c = u.searchParams.get("code");
    if (!c) throw "the link has no code";
    t = c; hubs = u.searchParams.getAll("hub").filter((h) => h.trim()); hub = hubs[0] ?? null; hubName = u.searchParams.get("name");
    const r = u.searchParams.get("role");
    role = r === "give" || r === "take" ? r : null;
  }
  const raw = t.toUpperCase().replace(/[^A-Z0-9]/g, "");
  if (raw.length !== 16) throw "a link code has 16 letters and digits (XXXX-XXXX-XXXX-XXXX); got " + raw.length;
  return { code: raw.replace(/(.{4})(?=.)/g, "$1-"), hub, hubs, hub_name: hubName, role };
}

/** The hubs a link brings, as this device reaches them (user 20:38Z): the
 *  other device's localhost hub, answering here by its Tailscale name, and
 *  the lab hub, which doesn't answer from here (its Tailscale name does). */
const LINK_HUBS: HubRow[] = [
  { theirs: "http://localhost:7370", address: "http://home-pc:7370", candidates: ["http://home-pc:7370", "http://100.101.102.103:7370", "http://192.168.1.20:7370"], name: "office", error: null, reachable: true },
  { theirs: "http://10.0.0.7:7370", address: "http://10.0.0.7:7370", candidates: ["http://10.0.0.7:7370", "http://lab.tail5c2e.ts.net:7370"], name: null, error: "connection refused (os error 10061)", reachable: false },
];
/** What a link brought, held (nothing saved) until linkConfirm or linkDiscard. */
let pendingLink: { to: string } | null = null;
/** The other device approves a link after this long. */
const approveAfter = (ms: number) => (params.get("approve") ? Number(params.get("approve")) * 1000 : ms);

/** This device's hubs become exactly `hubs` (as typed: normalized, repeats dropped). */
function setHubs(hubs: string[]) {
  st.hubs = [];
  for (const raw of hubs) {
    const url = normHub(raw);
    if (!url || st.hubs.some((h) => h.url === url)) continue;
    const h: HubStatus = { url, name: hubLabel(url), state: "connecting", error: null, retry_at_ms: null, max_attachment_bytes: GB, features: ["v2"] };
    st.hubs.push(h); reconnect(h);
  }
}
/** A fresh device adopts the identity a link brought, with the hubs the user kept. */
function adoptLinked(hubs: string[]): string {
  st.me = { id: "alex", address: "alex.3be2c9", name: "Alex Rivera", about: "Platform team" };
  st.recovery_saved = true;
  setHubs(hubs);
  const first = st.hubs[0]?.url;
  dir = first ? clone(SEED_DIR).map((c) => ({ ...c, hubs: [first] })) : [];
  setTimeout(() => emit({ type: "directory" }), 300);
  return st.me.address;
}
/** Leave this identity, adopt Maya's (her profile comes with it) with the hubs the user kept. */
function switchTo(address: string, hubs: string[]): string {
  st.me = { id: address.split(".")[0], address, name: "Maya Lin", about: "Design · Android" };
  st.recovery_saved = true;
  setHubs(hubs);
  msgs.length = 0;
  for (const k of Object.keys(drafts)) delete drafts[k];
  const first = st.hubs[0]?.url;
  dir = first ? clone(SEED_DIR).filter((c) => c.address !== address).map((c) => ({ ...c, hubs: [first] })) : [];
  if (first) dir.push(contact("alex.3be2c9", "person", { user: "Alex Rivera" }, "Platform team", true, now(), [first]));
  setTimeout(() => emit({ type: "directory" }), 300);
  return address;
}

// -------------------------------------------------------------- pipeline
const find = (id: string) => msgs.find((m) => m.id === id);
const contactOf = (a: string) => dir.find((c) => c.address === a);

// the hub picker, as the core routes (engine.rs pick_hub / route): a pinned
// hub only while it is connected and lists the peer; Automatic = a hub where
// they are online, then the one the chat last went through, then by address
const pins: Record<string, string> = {};
// messages that found no hub and wait for one (a pin change sends them)
const parked = new Set<string>();
function route(peer: string): SendRoute {
  const c = contactOf(peer);
  const listed = (c?.hubs || []).filter((u) => st.hubs.some((h) => h.url === u)).sort();
  const up = (u: string) => st.hubs.some((h) => h.url === u && h.state === "connected");
  const usable = listed.filter(up);
  const last = [...msgs].reverse().find((m) => m.peer === peer && m.outgoing && m.hub)?.hub;
  // (a mock contact is online on all its hubs or none: online decides nothing)
  // no hub lists them: the first connected hub tries (and the hub says no)
  const automatic = !listed.length
    ? st.hubs.map((h) => h.url).filter(up).sort()[0] ?? null
    : (last && usable.includes(last) ? last : usable[0]) ?? null;
  const pinned = pins[peer] && st.hubs.some((h) => h.url === pins[peer]) ? pins[peer] : null;
  const next = pinned ? (usable.includes(pinned) ? pinned : null) : automatic;
  return { pinned, automatic, next, hubs: listed.map((url) => ({ url, online: !!c?.online })) };
}

async function upload(m: Message): Promise<boolean> {
  for (const a of m.attachments) {
    if (a.state === "uploaded") continue;
    a.state = "uploading"; chatEv(m.peer);
    const total = a.bytes; let done = 0;
    const step = Math.max(1, Math.round(total / 24));
    while (done < total) {
      await sleep(140);
      if (a.state === "cancelled") return false;
      done = Math.min(total, done + step);
      emit({ type: "transfer", local_id: a.local_id, message_id: m.id, upload: true, done, total });
    }
    a.state = "uploaded"; chatEv(m.peer);
  }
  return true;
}

async function pipeline(m: Message) {
  m.state = "queued"; m.error = null; chatEv(m.peer);
  await sleep(300);
  const c = contactOf(m.peer);
  const via = route(m.peer).next;
  const hub = st.hubs.find((h) => h.url === via);
  if (!c) {
    await sleep(600);
    m.state = "failed"; m.error = "no hub knows " + m.peer + " (address not found)"; chatEv(m.peer); return;
  }
  if (!hub) { parked.add(m.id); return; } // waits, like the core does, until a hub reaches them
  m.state = "sending"; chatEv(m.peer);
  if (!(await upload(m))) { m.state = "failed"; m.error = "upload cancelled"; chatEv(m.peer); return; }
  await sleep(500);
  m.state = "sent"; m.hub = hub.url; m.sent_at = iso(now()); m.received_at = iso(now()); chatEv(m.peer);
  if (!c.online) return;
  await sleep(1300); m.state = "fetched"; m.fetched_at = iso(now()); chatEv(m.peer);
  await sleep(500); m.state = "delivered"; m.delivered_at = iso(now()); chatEv(m.peer);
  await sleep(2500); m.state = "read"; m.read_at = iso(now()); chatEv(m.peer);
  if (c.kind === "org" || c.kind === "chat") {
    await sleep(2500);
    const r = add(m.peer, false, now(), c.kind === "org" ? "Got it. An agent is on it; I'll report back here." : "On it. I'll reply in this chat when I'm done.", { seen: false, reply_to: m.id });
    emit({ type: "incoming", peer: m.peer, id: r.id, preview: r.body });
    chatEv(m.peer);
  }
}

function chatsList(): ChatSummary[] {
  const by = new Map<string, Message[]>();
  for (const m of msgs) { const l = by.get(m.peer) || []; l.push(m); by.set(m.peer, l); }
  const out: ChatSummary[] = [];
  for (const [peer, l] of by) {
    l.sort((a, b) => Date.parse(a.created_at) - Date.parse(b.created_at));
    out.push({ peer, last: clone(l[l.length - 1]), unread: l.filter((m) => !m.outgoing && !m.seen).length + (oldUnread.get(peer) || 0) });
  }
  return out.sort((a, b) => Date.parse(b.last.created_at) - Date.parse(a.last.created_at));
}

const WORDS = "amber anchor apple arrow atlas badge basket beacon birch blossom bracket breeze bridge cabin canyon cedar circle cliff clover comet copper coral cotton crater".split(" ");
const SIZES: Record<string, number> = { "nightly-logs.zip": 182 * 1048576, "screenshot-rc3.png": 412 * 1024, "release-notes.md": 6 * 1024 };

function normHub(raw: string): string | null {
  let a = raw.trim().replace(/\/+$/, "");
  if (!a || /\s/.test(a)) return null;
  if (!/^[a-z][a-z0-9+.-]*:\/\//i.test(a)) a = "http://" + a;
  try {
    const u = new URL(a);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    if (u.protocol === "http:" && !u.port) u.port = "7370";
    return u.protocol + "//" + u.host;
  } catch { return null; }
}
/** The office hub (the PC's localhost hub in LINK_HUBS) under its other names. */
const ALIASES: Record<string, string> = { "home-pc": "office", "100.101.102.103": "office", "192.168.1.20": "office" };
const hubLabel = (url: string) => {
  const host = new URL(url).hostname, h = host.split(".");
  return ALIASES[host] ?? (h[0] === "hub" && h[1] ? h[1] : /^\d+$/.test(h[0]) ? "hub-" + h[h.length - 1] : h[0]);
};

// Scan setup code: the core's parsing (crates/hubchat-core/src/setup.rs), in short
function parseSetupLink(input: string): { link: SetupLink | null; error: { kind: "not_setup" } | { kind: "needs_newer"; v: string } | { kind: "invalid"; param: string } | null } {
  const t = input.trim();
  if (!/^hubchat:\/\/setup(?:[/?#]|$)/i.test(t)) return { link: null, error: { kind: "not_setup" } };
  const q = new URLSearchParams(t.split("?")[1] || "");
  const get = (k: string) => (q.get(k) || "").trim() || null;
  const bad = (param: string) => ({ link: null, error: { kind: "invalid" as const, param } });
  const v = get("v");
  if (!v || !/^\d+$/.test(v) || Number(v) < 1) return bad("v");
  if (Number(v) > 1) return { link: null, error: { kind: "needs_newer", v } };
  const hub = normHub(get("hub") || "");
  if (!hub) return bad("hub");
  const org = (get("org") || "").replace(/^@net:/, "").toLowerCase();
  if (!/^[a-z0-9_-][a-z0-9._-]*\.[a-z0-9._-]*[a-z0-9_-]$/.test(org)) return bad("org");
  for (const p of ["orgname", "pc", "hubname"]) if (!get(p)) return bad(p);
  const raw = (get("code") || "").replace(/[- ]/g, "").toUpperCase();
  if (!/^[A-Z0-9]{8}$/.test(raw)) return bad("code");
  const net = get("net");
  if (net !== "tailscale" && net !== "wifi") return bad("net");
  if (net === "tailscale" && !get("ts")) return bad("ts");
  return { link: { hub, org, orgname: get("orgname")!.slice(0, 64), pc: get("pc")!.slice(0, 64), ts: net === "tailscale" ? get("ts") : null, code: raw.slice(0, 4) + "-" + raw.slice(4), net, hubname: get("hubname")! }, error: null };
}
const setups = new Map<string, SetupState>();
const setupFail = params.get("setupfail");

export const mockApi: Api = {
  state: async () => clone(st),
  uiState: async () => {},

  checkId: async (id) => {
    let error: string | null = null;
    if (!id) error = "Choose an id.";
    else if (id.length < 2) error = "An id has at least 2 characters.";
    else if (id.length > 24) error = "An id has at most 24 characters.";
    else if (!/^[a-z0-9]/.test(id)) error = "An id starts with a letter or a number.";
    else if (!/^[a-z0-9-]+$/.test(id)) error = "Use a–z, 0–9 and “-” only.";
    return { ok: !error, error, max_len: 24 };
  },
  createIdentity: async (id, name) => {
    await sleep(500);
    st.me = { id, address: id + "." + uid().slice(0, 6), name: name.trim(), about: "" };
    st.recovery_saved = false;
    return st.me.address;
  },
  restoreWords: async (words) => {
    await sleep(400);
    const w = words.toLowerCase().split(/[^a-z]+/).filter(Boolean);
    if (w.length !== 24) throw "recovery words are exactly 24 words; got " + w.length;
    const bad = w.find((x) => x.length < 3);
    if (bad) throw "\"" + bad + "\" is not on the word list";
    st.me = { id: "alex", address: "alex.3be2c9", name: "Alex Rivera", about: "" };
    st.recovery_saved = true;
    return st.me.address;
  },
  recoveryWords: async () => [...WORDS],
  recoverySaved: async () => { st.recovery_saved = true; },
  setProfile: async (name, about) => { await sleep(250); if (st.me) { st.me.name = name.trim(); st.me.about = about.trim(); } },
  setReadReceipts: async (on) => { st.read_receipts = on; },
  setNotifications: async (n) => { st.notifications = { ...n }; },
  setStayConnected: async (on) => { await sleep(300); st.stay_connected = on; },
  setActive: async () => {},
  setDeviceName: async (name) => {
    await sleep(200);
    if (name.trim().length > 64) throw "a device name has at most 64 characters";
    st.device_name = name.trim() || (st.platform === "android" ? "Pixel 8" : "Home-PC");
    return st.device_name;
  },

  probeHub: async (input): Promise<Probe> => {
    const url = normHub(input);
    if (!url) return { result: "invalid", error: "not a hub address: " + JSON.stringify(input.trim()) };
    await sleep(900);
    // a bare host found on another of the hub's ports (user 23:46Z)
    if (/^star-hub\/?$/i.test(input.trim())) return { result: "connected", url: "http://star-hub:7378", name: "star-hub", max_attachment_bytes: GB, features: ["v2"], version: "2.0.0", discovered: true };
    if (/unreach|10\.0\.9\.|10\.0\.0\.7|127\.0\.0\.1|localhost/.test(url)) return { result: "unreachable", url, error: "connection refused (os error 10061)" };
    if (/example|google|github/.test(url)) return { result: "not_a_hub", url, error: "GET /healthz answered 404 Not Found" };
    return { result: "connected", url, name: hubLabel(url), max_attachment_bytes: GB, features: ["v2"], version: "1.4.0" };
  },
  probeLinkHubs: async (hubs, name): Promise<Probe> => {
    let first: Probe | null = null;
    for (const h of hubs) {
      const p = await mockApi.probeHub(h);
      if (p.result === "connected" && (!name || p.name === name)) return p;
      first ??= p;
    }
    return first ?? { result: "invalid", error: "the link names no hub" };
  },
  addHub: async (input) => {
    const url = normHub(input);
    if (!url) throw "not a hub address";
    if (st.hubs.some((h) => h.url === url)) throw "you already added this hub";
    const h: HubStatus = { url, name: hubLabel(url), state: "connecting", error: null, retry_at_ms: null, max_attachment_bytes: GB, features: ["v2"] };
    st.hubs.push(h);
    reconnect(h);
    if (!dir.length) {
      setTimeout(() => { dir = clone(SEED_DIR).map((c) => ({ ...c, hubs: [url] })); emit({ type: "directory" }); }, 1300);
    }
    return url;
  },
  removeHub: async (url) => {
    await sleep(300); st.hubs = st.hubs.filter((h) => h.url !== url);
    // like the core: a pin goes with its hub (adding it again doesn't bring it back)
    for (const p of Object.keys(pins)) if (pins[p] === url) delete pins[p];
    dir = dir.map((c) => ({ ...c, hubs: c.hubs.filter((x) => x !== url) })).filter((c) => c.hubs.length); hubEv(url); emit({ type: "directory" });
  },
  retryNow: async () => { for (const h of st.hubs) if (h.state === "disconnected") reconnect(h); },

  directory: async () => clone(dir),
  resolve: async (input): Promise<Resolved> => {
    const raw = input.trim().replace(/^@net:/, "").toLowerCase();
    const valid = !!raw && /^[a-z0-9._-]+$/.test(raw);
    const exact = dir.find((d) => d.address === raw) || null;
    const matches = !exact && valid ? dir.filter((d) => d.address.startsWith(raw + ".")) : [];
    return { exact: clone(exact), matches: clone(matches), is_me: raw === st.me?.address, valid, address: raw };
  },
  chats: async () => chatsList(),
  chat: async (peer, o = {}) => {
    // as the core pages: by time then id, newest `limit` before `before`, none older than `from`
    const key = (m: Message) => [m.created_at, m.id] as const;
    const lt = (a: Message, b: Message) => { const [x, y] = [key(a), key(b)]; return x[0] < y[0] || (x[0] === y[0] && x[1] < y[1]); };
    const floor = floorOf(peer);
    let all = msgs.filter((m) => m.peer === peer && (!floor || m.created_at > floor)).sort((a, b) => (lt(a, b) ? -1 : lt(b, a) ? 1 : 0));
    if (o.before) all = all.filter((m) => lt(m, o.before!));
    if (o.from) all = all.filter((m) => !lt(m, o.from!));
    await sleep(o.before ? 350 : 0);
    return clone(all.slice(-(o.limit ?? 50)));
  },
  loadOlder: async (peer): Promise<OlderPage> => {
    const out: OlderPage = { added: 0, more: [], unreachable: [] };
    const per = hubOld.get(peer);
    if (!per) return out;
    await sleep(600);
    for (const [hub, list] of per) {
      if (!list.length) continue;
      if (st.hubs.find((h) => h.url === hub)?.state !== "connected") { out.unreachable.push(hub); continue; }
      for (const m of list.splice(Math.max(0, list.length - 20))) {
        if (find(m.id)) continue;
        msgs.push(clone(m)); out.added++;
        if (!m.outgoing && !m.seen) oldUnread.set(peer, Math.max(0, (oldUnread.get(peer) || 0) - 1));
      }
      if (list.length) out.more.push(hub);
    }
    if (out.added) chatEv(peer);
    return out;
  },
  message: async (id) => { const m = find(id); return m ? clone(m) : null; },
  send: async (n: NewOutgoing) => {
    const m = add(n.peer, true, now(), n.body, {
      id: n.id, kind: n.kind ?? null, reply_to: n.reply_to ?? null, state: "queued", hub: null,
      sent_at: null, received_at: null, fetched_at: null, delivered_at: null, read_at: null,
      attachments: (n.attachments || []).map((a) => att(a.name, a.bytes, "pending", { source: a.source, hub_id: null })),
    });
    void pipeline(m);
  },
  retry: async (id) => {
    const m = find(id); if (!m) throw "no such message";
    m.attachments.forEach((a) => { if (a.state !== "uploaded") a.state = "pending"; });
    void pipeline(m);
  },
  cancelTransfer: async (localId) => {
    for (const m of msgs) for (const a of m.attachments) if (a.local_id === localId) { a.state = "cancelled"; chatEv(m.peer); }
  },
  download: async (messageId, localId) => {
    const m = find(messageId); const a = m?.attachments.find((x) => x.local_id === localId);
    if (!m || !a) throw "no such attachment";
    a.state = "downloading"; a.error = null; chatEv(m.peer);
    let done = 0; const step = Math.max(1, Math.round(a.bytes / 16));
    while (done < a.bytes) {
      await sleep(120);
      if (a.state === "cancelled") { chatEv(m.peer); throw "download cancelled"; }
      done = Math.min(a.bytes, done + step);
      emit({ type: "transfer", local_id: a.local_id, message_id: m.id, upload: false, done, total: a.bytes });
    }
    a.state = "done"; a.local_path = "C:\\Users\\alex\\Downloads\\" + a.name; chatEv(m.peer);
    return a.local_path;
  },
  sendRoute: async (peer) => route(peer),
  setSendHub: async (peer, hub) => {
    if (hub) pins[peer] = hub; else delete pins[peer];
    chatEv(peer);
    // what waited for a hub goes now, if it can (the core's sender wakes);
    // a message still on its way is not sent twice
    for (const m of msgs) if (m.peer === peer && parked.delete(m.id) && m.state === "queued") void pipeline(m);
  },
  markRead: async (peer) => {
    let n = 0;
    for (const m of msgs) if (m.peer === peer && !m.outgoing && !m.seen) { m.seen = true; m.read_at = new Date().toISOString(); n++; }
    // what the hubs counted but this device hasn't loaded: read too
    if (oldUnread.get(peer)) { oldUnread.delete(peer); for (const l of hubOld.get(peer)?.values() || []) for (const m of l) m.seen = true; n++; }
    if (n) chatEv(peer);
  },
  deleteMessage: async (id) => { const i = msgs.findIndex((m) => m.id === id); if (i >= 0) { const p = msgs[i].peer; msgs.splice(i, 1); chatEv(p); } },
  deleteChat: async (peer) => { for (let i = msgs.length - 1; i >= 0; i--) if (msgs[i].peer === peer) msgs.splice(i, 1); chatEv(peer); },
  draft: async (peer) => drafts[peer] ?? null,
  setDraft: async (peer, body) => { drafts[peer] = body; },
  saveRecovery: async (dest) => dest || "Downloads/hubchat-recovery-alex.txt",
  devices: async () => ({ this_device: "hc-mock-pc", devices: [
    { device_id: "hc-mock-pc", name: "HOME-PC", created_at: null, last_seen: null, online: true },
    { device_id: "hc-mock-phone", name: "Android phone", created_at: null, last_seen: "2026-10-08T17:58:00.000Z", online: false },
  ] }),
  fileInfo: async (source) => { const name = source.split(/[\\/]/).pop() || "file"; return { name, bytes: pasted.get(source)?.length ?? SIZES[name] ?? 12345 }; },
  attachmentPreview: async (messageId, localId) => {
    const m = find(messageId); const a = m?.attachments.find((x) => x.local_id === localId);
    if (!m || !a) throw "no such attachment";
    await sleep(m.outgoing ? 60 : 400);
    if (a.state === "expired" || /broken/i.test(a.name)) throw "gone";
    const own = a.source ? pasted.get(a.source) : undefined;
    return own ? (own.slice().buffer as ArrayBuffer) : picture(a.name, m.outgoing);
  },
  // the browser has no native clipboard: tests set window.__clip (text) or
  // window.__clipImage (PNG bytes)
  clipboardText: async () => (window as unknown as { __clip?: string }).__clip || null,
  clipboardImage: async () => {
    const img = (window as unknown as { __clipImage?: Uint8Array }).__clipImage;
    if (!img) throw "no picture on the clipboard";
    return img.slice().buffer as ArrayBuffer;
  },
  // Android's in-app update, acted out: ?update=1 offers 0.2.0; the first
  // Update asks for Android's permission, then it downloads and Android's
  // window "opens"; ?update=bad fails the signature check
  appUpdateCheck: async () => {
    await sleep(300);
    const u = new URLSearchParams(location.search).get("update");
    return u === "1" || u === "bad" ? { version: "0.2.0", notes: "Markdown and fixes.", url: "https://example.com/Hubchat_0.2.0_arm64.apk", signature: "mock" } : null;
  },
  appUpdateInstall: async () => {
    if (!updateAllowed) return "permission";
    for (let i = 1; i <= 10; i++) { await sleep(120); updateProgress.forEach((f) => f(i * 1.8e6, 18e6)); }
    if (new URLSearchParams(location.search).get("update") === "bad") throw "the download isn't signed by Hubchat's update key";
    updateState = "confirm";
    return "installing";
  },
  appUpdateAllow: async () => { updateAllowed = true; },
  appUpdateState: async () => updateState,
  onAppUpdateProgress: async (f) => { updateProgress.add(f); return () => { updateProgress.delete(f); }; },
  filePreview: async (source, name) => {
    await sleep(80);
    if (/broken/i.test(name)) throw "unreadable";
    const own = pasted.get(source);
    return own ? (own.slice().buffer as ArrayBuffer) : picture(name, true);
  },
  savePasted: async (name, data) => {
    // a fresh file each time, as the shell does
    const dir = "C:\\Users\\alex\\AppData\\Roaming\\Hubchat\\pasted\\";
    const [stem, ext] = [name.replace(/\.[^.]*$/, ""), name.split(".").pop()];
    let p = dir + name;
    for (let i = 2; pasted.has(p); i++) p = dir + stem + "-" + i + "." + ext;
    pasted.set(p, data);
    return p;
  },

  linkStart: async (hub, deviceName, joinCode) => {
    await sleep(700);
    const my = ++linkRun;
    const code = joinCode ? parseLink(joinCode).code : "K7QD-4MXP-9TRA-2HZE";
    setTimeout(() => { if (linkRun === my) linkEmit({ state: "waiting", expires_in_s: 600 }); }, 100);
    if (st.me) {
      // a signed-in device: the other device approves after 2 s
      const from = st.me.address;
      setTimeout(() => {
        if (linkRun !== my) return;
        if (code.startsWith("SAME")) linkEmit({ state: "same", address: from });
        else { pendingLink = { to: "maya.e71f2b" }; linkEmit({ state: "review", from, to: pendingLink.to, name: "Maya Lin", hubs: clone(LINK_HUBS) }); }
      }, approveAfter(2000));
      return { code, qr: "hubchat://link?code=" + code + "&hub=" + encodeURIComponent(normHub(hub) || hub) + "&role=take", hub: normHub(hub) || hub };
    }
    // the other device approves after a while (long enough to look at the code)
    setTimeout(() => {
      if (linkRun !== my) return;
      pendingLink = { to: "alex.3be2c9" };
      linkEmit({ state: "review", from: null, to: pendingLink.to, name: "Alex Rivera", hubs: clone(LINK_HUBS) });
    }, approveAfter(45000));
    void deviceName;
    return { code, qr: "hubchat://link?code=" + code + "&hub=" + encodeURIComponent(normHub(hub) || hub) + "&role=take", hub: normHub(hub) || hub };
  },
  linkCancel: async () => { linkRun++; },
  linkConfirm: async (hubs) => {
    const p = pendingLink;
    if (!p) throw "nothing to confirm: link again";
    await sleep(900);
    pendingLink = null;
    return st.me ? switchTo(p.to, hubs) : adoptLinked(hubs);
  },
  linkDiscard: async () => { pendingLink = null; },
  linkOffer: async (hub) => {
    await sleep(300);
    const url = hub ? normHub(hub) : (st.hubs.find((h) => h.state === "connected") || st.hubs[0])?.url;
    if (!url) throw "add a hub first: the new device links through one";
    const code = Array.from({ length: 4 }, () => Array.from({ length: 4 }, () => "ABCDEFGHJKMNPQRSTUVWXYZ23456789"[Math.floor(Math.random() * 31)]).join("")).join("-");
    return { code, qr: "hubchat://link?code=" + encodeURIComponent(code) + "&hub=" + encodeURIComponent(url) + "&role=give", hub: url, phone_access_off: params.get("dooroff") === "1" };
  },
  parseLink: async (input) => parseLink(input),
  linkLookup: async (input): Promise<LinkLookup> => {
    await sleep(500);
    const { code, hub } = parseLink(input);
    const n = (lookups.get(code) || 0) + 1; lookups.set(code, n);
    const unknown = hub && !st.hubs.some((h) => h.url === normHub(hub)) ? hub : null;
    const found = n >= 3 && !unknown;
    return { code, address: "link." + code.replace(/-/g, "").slice(0, 6).toLowerCase(), device_name: found ? "Pixel 7a" : null, hubs: found ? [OFFICE] : [], unknown_hub: unknown };
  },
  linkApprove: async (code) => { await sleep(1300); lookups.delete(code); return OFFICE; },
  keyQr: async () => { await sleep(200); return KEY_QR; },
  restoreQr: async (text) => {
    await sleep(600);
    if (!/^hubchat-key:/.test(text)) throw "this QR code doesn't hold a Hubchat key";
    return adopt(true);
  },
  keyFileExport: async (passphrase, dest) => {
    await sleep(500);
    if (passphrase.length < 8) throw "the passphrase needs at least 8 characters";
    void dest;
  },
  keyFileImport: async (source, passphrase) => {
    await sleep(800);
    if (passphrase === "wrong") throw "wrong passphrase, or " + source.split(/[\\/]/).pop() + " isn't a Hubchat key file";
    return adopt(true);
  },
  onLink: async (f) => { linkListeners.add(f); return () => { linkListeners.delete(f); }; },

  parseSetup: async (input) => {
    await sleep(300);
    if (setupFail === "update") return { link: null, error: { kind: "needs_newer", v: "2" } };
    if (setupFail === "damaged") return { link: null, error: { kind: "invalid", param: "code" } };
    return parseSetupLink(input);
  },
  setupCheck: async (hub) => {
    await sleep(900);
    if (setupFail === "unreach" || setupFail === "unreach-wifi") return { reachable: false, name: null, error: "no answer within 5 s" };
    return { reachable: true, name: hubLabel(hub), error: null };
  },
  appInstalled: async () => (st.platform === "android" ? setupFail !== "no-ts" : null),
  openApp: async (what) => { toast("[Opens " + (what === "get_tailscale" ? "Tailscale in Google Play" : what === "open_tailscale" ? "the Tailscale app" : "Android's Wi-Fi settings") + "]"); return true; },
  setupStart: async (input, name) => {
    const l = parseSetupLink(input).link;
    if (!l) throw "not a setup code";
    await sleep(1200);
    if (setupFail === "join") throw "invalid input: couldn't join " + l.pc + "'s hub";
    if (!st.hubs.some((h) => h.url === l.hub)) {
      st.hubs.push({ url: l.hub, name: l.hubname, state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: GB, features: ["v2"], version: "2.0.0" });
      hubEv(l.hub);
    }
    if (!dir.some((c) => c.address === l.org)) dir.push(contact(l.org, "org", { org: l.orgname }, "", true, now(), [l.hub]));
    emit({ type: "directory" });
    setups.set(l.org, { outcome: null, code: l.code, orgname: l.orgname, pc: l.pc, sent_at: iso(now()), at: null, reply_id: null });
    add(l.org, true, now(), "Hi " + l.orgname + ", this is " + name + ", linking Hubchat on my phone.\n\nSetup code: " + l.code, { state: "sent", read_at: null, delivered_at: null, fetched_at: null });
    chatEv(l.org);
    const reply = params.get("setupreply") || "linked";
    if (reply !== "none") {
      setTimeout(() => {
        const m = add(l.org, false, now(), reply === "linked"
          ? "Welcome, " + name + ". " + l.orgname + " now knows this address is you.\n\nSetup code: " + l.code + " linked"
          : "That setup code has expired or was already used.\n\nSetup code: " + l.code + " expired", { seen: false });
        setups.set(l.org, { ...setups.get(l.org)!, outcome: reply === "linked" ? "linked" : "expired", at: m.created_at, reply_id: m.id });
        emit({ type: "incoming", peer: l.org, id: m.id, preview: m.body });
        chatEv(l.org);
      }, 2500);
    }
    return l.org;
  },
  setupStatus: async (org) => clone(setups.get(org) ?? null),

  onPendingLink: async () => () => {},
  takePendingLink: async () => {
    const p = params.get("link");
    if (!p || linkTaken) return null;
    linkTaken = true;
    return p;
  },
  takePendingChat: async () => {
    const p = params.get("pending");
    if (!p || pendingTaken) return null;
    pendingTaken = true;
    return p;
  },
  openAttachment: async (messageId, localId, reveal) => {
    const a = find(messageId)?.attachments.find((x) => x.local_id === localId);
    if (!a?.local_path) throw "not downloaded yet";
    toast((reveal ? "Showing " : "Opening ") + a.name + (reveal ? " in its folder" : ""));
  },

  onEvent: async (f) => { listeners.add(f); return () => { listeners.delete(f); }; },
};
let pendingTaken = false;
let linkTaken = false;
