// Mounted App, synthetic API only: inline video and audio (Hubchat 1.2.7; user 2026-10-30 14:24Z and 14:33Z, docket
// hubchat-inline-video-player-for-video-attachment), as Orgtree's media card plays them. A received video shows its
// player and first frame on load, paused, and only its own controls play it; audio opens on render the same way (user 2026-11-10). Scrolled
// away, hidden or gone with its chat, a player lets go and comes back paused. Past 256 MiB, unplayable or broken,
// the file row stays with Orgtree's note. The media come from tools/inline-media-electron.cjs's stand-in for the
// hcmedia scheme (a 206 streamed as the player reads it, as the hub answers); the driver does the clicks and keys
// (native input to its own window) between steps. ?platform=desktop|android
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Attachment, type Contact, type HubStatus, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

type Check = { name: string; ok: boolean; detail?: unknown };
type Media = { file: string; bytes: number; type: string };
declare global {
  interface Window {
    mediaManifest: Record<string, Media>;
    mediaFixture?: { checks: Check[]; calls: string[]; platform: string; step: (name: string) => Promise<unknown>; target: (what: string) => { x: number; y: number } | null };
    mediaFixtureError?: string;
  }
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const until = async (f: () => boolean, ms: number) => { const t = performance.now(); while (performance.now() - t < ms) { if (f()) return true; await sleep(25); } return f(); };

const A = "wss://hub-a.invalid";
const ME = "scratch.abcdef", PAT = "pat.1a2b3c", ALICE = "alice.111111";
const MIB = 1024 * 1024;
const NOT_SUPPORTED = "This format is not supported here. Download the file to open it in another app.";
const UNAVAILABLE = "Preview unavailable. Download the file to open it in another app.";
const TOO_BIG = "Playback is limited to 256 MiB. Download this file to open it.";

async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: Check[] = [], calls: string[] = [];
  const check = (name: string, ok: unknown, detail?: unknown) => { checks.push({ name, ok: !!ok, ...(ok ? {} : { detail }) }); };
  const manifest = window.mediaManifest;
  // a format this platform can't play, for the note: FLAC here (a stand-in; the check is the card's, not the codec's)
  const canPlay = HTMLMediaElement.prototype.canPlayType;
  HTMLMediaElement.prototype.canPlayType = function (type: string) { return type === "audio/flac" ? "" : canPlay.call(this, type); };

  const contact = (address: string, name: string): Contact =>
    ({ address, kind: "person", org_name: "", username: name, blurb: "", online: true, last_seen: null, hubs: [A], picture: null });
  const directory: Contact[] = [contact(ME, "Scratch"), contact(PAT, "Pat"), contact(ALICE, "Alice")];
  const hubs: HubStatus[] = [{ url: A, name: "Hub A", state: "connected", error: null, retry_at_ms: null, max_attachment_bytes: 1 << 30, features: ["v2"] }];
  const att = (local_id: string, name: string, bytes: number): Attachment =>
    ({ local_id, hub_id: "h-" + local_id, name, bytes, source: null, local_path: null, state: "remote", error: null, download: null });
  let n = 0;
  const message = (peer: string, body: string, attachments: Attachment[] = [], outgoing = false): Message => ({ id: peer + "-" + n, peer, outgoing, hub: A, body, kind: null, reply_to: null,
    created_at: new Date(1700000000000 + n++ * 60000).toISOString(), sent_at: null, received_at: null, state: outgoing ? "delivered" : "received", fetched_at: null, delivered_at: null, read_at: null,
    error: null, seen: true, attachments } as Message);
  const fillers = Array.from({ length: 30 }, (_, i) => message(PAT, "Earlier message " + (i + 1)));
  const patChat: Message[] = [...fillers,
    message(PAT, "Here's the clip", [att("vid", "clip.mp4", manifest.vid.bytes)]),
    message(PAT, "And the notes", [att("aud", "notes.mp3", manifest.aud.bytes)]),
    message(PAT, "Other formats", [att("webm", "clip.webm", manifest.webm.bytes), att("bad", "broken.mp4", manifest.bad.bytes), att("flac", "voice.flac", manifest.aud.bytes)]),
    message(PAT, "The whole lecture", [att("huge", "lecture.mp4", 300 * MIB)]),
    message(PAT, "Thanks!", [], true)];
  const aliceChat = [message(ALICE, "Hello")];
  const chat = (p: string) => (p === PAT ? patChat : p === ALICE ? aliceChat : []);
  const find = (id: string) => [...patChat, ...aliceChat].find((m) => m.id === id) ?? null;
  Object.assign(api, mockApi, {
    state: async () => ({ ...await mockApi.state(), me: { id: "scratch", address: ME, name: "Scratch", about: "" }, hubs, platform, recovery_saved: true, file_background_hint: null }),
    directory: async () => directory,
    chats: async () => [PAT, ALICE].map((p) => ({ peer: p, last: chat(p).at(-1), unread: 0 })),
    chat: async (p: string) => chat(p),
    message: async (id: string) => find(id),
    picture: async () => null,
    transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    openDm: async () => {}, markRead: async () => {}, draft: async () => "", setDraft: async () => {}, draftAnswers: async () => ({ replies: [], picks: {} }),
    setupStatus: async () => null, sendRoute: async () => ({ pinned: null, automatic: A, next: A, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
    download: async (_m: string, lid: string) => { calls.push("download:" + lid); },
    // the hcmedia URL, as the shell's would name it; the driver's stand-in answers it
    mediaUrl: (m: string, lid: string) => "hcmedia://media/" + encodeURIComponent(m) + "/" + encodeURIComponent(lid),
  });
  const root = createRoot(document.getElementById("root")!); root.render(<App />);
  await sleep(300);
  const row = (name: string) => [...document.querySelectorAll<HTMLElement>(".crow")].find((e) => e.querySelector(".crow-name")?.textContent === name);
  const header = () => (platform === "desktop" ? document.querySelector(".conv-head .name") : document.querySelector(".appbar .who .n .ell"))?.textContent ?? "";
  const open = async (name: string) => { row(name)?.click(); await until(() => header() === name, 3000); await sleep(250); };
  await open("Pat");
  check("the conversation with Pat is open", header() === "Pat", header());

  const bubble = (text: string) => [...document.querySelectorAll<HTMLElement>(".msg")].find((m) => m.querySelector(".mtext")?.textContent?.includes(text)) ?? null;
  const cardOf = (lid: string) => document.querySelector<HTMLElement>(`.att-media[data-att="${lid}"]`);
  const videoOf = (lid: string) => cardOf(lid)?.querySelector("video") ?? null;
  const audioOf = (lid: string) => cardOf(lid)?.querySelector("audio") ?? null;
  const rowOf = (lid: string) => document.querySelector<HTMLElement>(`.att-file[data-att="${lid}"]`);
  const note = (lid: string) => cardOf(lid)?.querySelector(".att-note")?.textContent ?? null;
  const playButton = (lid: string) => cardOf(lid)?.querySelector<HTMLButtonElement>("button.att-play") ?? null;
  const shown = (e: Element | null | undefined) => !!e && (e as HTMLElement).offsetWidth > 0 && (e as HTMLElement).offsetHeight > 0;
  const scroller = (e: HTMLElement | null) => { for (let p = e?.parentElement; p; p = p.parentElement) { const s = getComputedStyle(p); if (/(auto|scroll)/.test(s.overflowY) && p.scrollHeight > p.clientHeight) return p; } return null; };
  const centre = (e: HTMLElement | null) => { e?.scrollIntoView({ block: "center" }); };
  const state = (v: HTMLMediaElement | null) => v && { src: v.getAttribute("src"), paused: v.paused, t: +v.currentTime.toFixed(2), ready: v.readyState, preload: v.preload, controls: v.controls, w: (v as HTMLVideoElement).videoWidth ?? null };
  const firstFrame = async (v: HTMLVideoElement | null) => !!v && await until(() => v.readyState >= 2 && v.videoWidth > 0, 8000);
  const played = new Set<string>();
  document.addEventListener("playing", (e) => { const c = (e.target as HTMLElement).closest(".att-media") as HTMLElement | null; if (c?.dataset.att) played.add(c.dataset.att); }, true);
  const playing = async (v: HTMLMediaElement | null) => !!v && await until(() => !v.paused && v.currentTime > 0.3, 6000);
  let held: HTMLMediaElement | null = null;

  const steps: Record<string, () => Promise<unknown>> = {
    // desktop and Android: the video on load, then after a click on it (the driver's)
    "video-load": async () => {
      centre(cardOf("vid") ?? bubble("Here's the clip"));
      const v = videoOf("vid");
      const ready = await firstFrame(v);
      check("a received video shows its player on load: native controls, preload metadata", shown(v) && v!.controls && v!.preload === "metadata", state(v));
      check("its first frame shows, paused, at the start", ready && v!.paused && v!.currentTime === 0, state(v));
      await sleep(1500);
      check("it doesn't play on its own", !!v && v.paused && v.currentTime === 0 && !played.has("vid"), state(v));
      check("its file row stays below, with Download", !!rowOf("vid") && !!cardOf("vid")?.contains(rowOf("vid")) && /Download/.test(rowOf("vid")!.textContent || ""), rowOf("vid")?.outerHTML.slice(0, 300));
      return state(v);
    },
    "video-playing": async () => {
      const v = videoOf("vid");
      check("a click on it plays it", await playing(v), state(v));
      return state(v);
    },
    // desktop: fullscreen (the driver asks the element, as its own button does, with a user gesture)
    "video-fullscreen": async () => {
      const v = videoOf("vid");
      await until(() => document.fullscreenElement === v, 3000);
      await sleep(600);
      check("it fills the screen", !!v && document.fullscreenElement === v, { fullscreen: document.fullscreenElement?.tagName ?? null });
      check("and keeps playing there (the card's offscreen release leaves a fullscreen player alone)", !!v && !v.paused && !!v.getAttribute("src"), state(v));
      return state(v);
    },
    "video-escaped": async () => {
      const v = videoOf("vid");
      await until(() => !document.fullscreenElement, 3000);
      await sleep(300);
      check("Escape leaves fullscreen", !document.fullscreenElement, document.fullscreenElement?.tagName);
      check("and the chat stays open", header() === "Pat", header());
      return state(v);
    },
    // desktop: letting go when scrolled away, hidden, or the chat changes; never playing again on its own
    "stop-scroll": async () => {
      const v = videoOf("vid"), was = await playing(v);
      const list = scroller(cardOf("vid"));
      if (list) list.scrollTop = 0;
      await sleep(600);
      check("scrolled away, a playing video lets go of its file", was && !v!.getAttribute("src") && v!.paused, state(v));
      centre(cardOf("vid"));
      const back = await firstFrame(videoOf("vid"));
      await sleep(800);
      check("scrolled back, it shows its first frame again, paused: it never plays on its own", back && videoOf("vid") === v && v!.paused && v!.currentTime === 0, state(videoOf("vid")));
      return state(v);
    },
    "stop-hidden": async () => {
      const v = videoOf("vid"), was = await playing(v);
      const hide = (on: boolean) => { Object.defineProperty(document, "hidden", { configurable: true, get: () => on }); document.dispatchEvent(new Event("visibilitychange")); };
      hide(true); await sleep(400);
      check("hidden (the window minimised or in the tray), a playing video lets go", was && !v!.getAttribute("src") && v!.paused, state(v));
      hide(false); delete (document as { hidden?: boolean }).hidden;
      const back = await firstFrame(videoOf("vid"));
      await sleep(800);
      check("shown again, it is paused at its first frame", back && v!.paused && v!.currentTime === 0, state(v));
      return state(v);
    },
    "stop-chat": async () => {
      held = videoOf("vid");
      const was = await playing(held);
      await open("Alice");
      check("another chat opened, the playing video is gone and let go", was && header() === "Alice" && !videoOf("vid") && !!held && held.paused && !held.getAttribute("src"), { header: header(), held: state(held) });
      await open("Pat");
      centre(cardOf("vid"));
      const back = await firstFrame(videoOf("vid"));
      await sleep(800);
      check("back in the chat, the video is paused at its first frame", back && videoOf("vid")!.paused && videoOf("vid")!.currentTime === 0, state(videoOf("vid")));
      return state(videoOf("vid"));
    },
    // audio: Play audio, then its native control row, playing (the driver clicks the button)
    "audio-load": async () => {
      centre(cardOf("aud") ?? bubble("And the notes"));
      await sleep(600);
      const a = audioOf("aud");
      check("received audio shows its player on load like a video: visible native controls, preload metadata, no Play audio button", shown(a) && a!.controls && a!.preload === "metadata" && !playButton("aud"), { button: playButton("aud")?.outerHTML, audio: state(a) });
      await sleep(1500);
      check("the audio doesn't play on its own", !!a && a.paused && a.currentTime === 0 && !played.has("aud"), state(a));
      check("its file row stays below, with Download", !!rowOf("aud") && !!cardOf("aud")?.contains(rowOf("aud")) && /Download/.test(rowOf("aud")!.textContent || ""), rowOf("aud")?.outerHTML.slice(0, 300));
      return state(a);
    },
    "audio-playing": async () => {
      const a = audioOf("aud");
      check("a click on its play control plays it", await playing(a) && shown(a) && a!.controls && !playButton("aud"), state(a));
      return state(a);
    },
    "audio-chat": async () => {
      held = audioOf("aud");
      const was = await playing(held);
      await open("Alice");
      check("another chat opened, the audio stops", was && !!held && held.paused && !held.getAttribute("src"), state(held));
      await open("Pat");
      centre(cardOf("aud")); await sleep(600);
      check("back in the chat, its player is open again, paused: it never plays on its own", shown(audioOf("aud")) && audioOf("aud")!.paused && audioOf("aud")!.currentTime === 0 && !playButton("aud"), { button: !!playButton("aud"), audio: state(audioOf("aud")) });
      return null;
    },
    // limits: the ceiling, a format this platform can't play, a broken file, and Download
    "limits": async () => {
      centre(cardOf("huge") ?? bubble("The whole lecture"));
      await sleep(400);
      check("past 256 MiB a video has no player, Orgtree's note, and its file row", !!cardOf("huge") && !cardOf("huge")!.querySelector("video") && note("huge") === TOO_BIG && !!rowOf("huge"), cardOf("huge")?.outerHTML.slice(0, 400) ?? rowOf("huge")?.outerHTML.slice(0, 300));
      centre(cardOf("webm") ?? bubble("Other formats"));
      check("a WebM video shows its first frame too", await firstFrame(videoOf("webm")), state(videoOf("webm")));
      await until(() => !!note("bad"), 6000);
      check("a broken video says so, and keeps its file row", note("bad") === UNAVAILABLE && !shown(videoOf("bad")) && !!rowOf("bad"), { note: note("bad"), video: state(videoOf("bad")) });
      await until(() => !!note("flac"), 3000);
      check("a format this platform can't play says so, and keeps its file row", note("flac") === NOT_SUPPORTED && !!rowOf("flac") && !audioOf("flac")?.getAttribute("src"), { note: note("flac"), audio: state(audioOf("flac")) });
      rowOf("huge")?.click(); await sleep(200);
      check("a file row's Download still downloads", calls.includes("download:huge"), calls);
      return null;
    },
  };
  const target = (what: string) => {
    const e = what === "video" ? videoOf("vid") : what === "play-audio" ? audioOf("aud") : null;
    if (!e) return null;
    const r = e.getBoundingClientRect();
    // an audio player's play control is at the left end of its control row; a video toggles on a click at its centre
    return what === "play-audio" ? { x: Math.round(r.x + 24), y: Math.round(r.y + r.height / 2) } : { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) };
  };
  window.mediaFixture = { checks, calls, platform, target, step: async (name) => { const f = steps[name]; if (!f) throw Error("no step " + name); return f(); } };
}
run().catch((e) => { window.mediaFixtureError = String(e?.stack || e); });
