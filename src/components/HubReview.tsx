// The hubs an arriving identity brings, reviewed before anything is saved
// (user 20:38Z): one row per hub, prefilled with the address this device
// reached (the other device may know its hub as localhost, which here means
// this device), each checked live. Edit an address, untick a hub, add one;
// Confirm sends the ticked addresses as shown. Used by onboarding (a fresh
// device) and by joining a link on a signed-in device (the switch screen).
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type HubRow, type Probe } from "../api";
import { Icon } from "../lib/icons";
import { errText } from "../lib/native";
import { NoteCard, usePlatform } from "./ui";

type Check =
  | { k: "empty" }
  | { k: "checking" }
  /** `found`: a bare host answered on a port it didn't name (user 23:46Z). */
  | { k: "ok"; name: string; found?: string }
  | { k: "bad"; error: string }
  /** Not an address at all: it can't be saved. */
  | { k: "invalid"; error: string };

interface Row {
  key: number;
  /** The hub as the other device knows it (null: a hub added here). */
  theirs: string | null;
  value: string;
  candidates: string[];
  on: boolean;
  /** The user ticked or unticked it (or added it): `on` is theirs. Until
   *  then it follows the check (see `wanted`). */
  touched: boolean;
  check: Check;
  added: boolean;
}

/** How long typing pauses before an edited address is checked. */
const DEBOUNCE = 600;

/** An address as people read and type it: no http:// (the core adds it back,
 *  as it does to anything typed), no trailing slash; https:// stays. */
const short = (a: string) => a.trim().replace(/^http:\/\//i, "").replace(/\/+$/, "");
const same = (a: string, b: string) => short(a).toLowerCase() === short(b).toLowerCase();
/** "on port 7378", or "over https" for a tunnel, for "Found X …". */
export function foundWhere(url: string): string {
  try {
    const u = new URL(url);
    return u.protocol === "https:" ? "over https" : "on port " + (u.port || "80");
  } catch { return "at " + url; }
}

/** An address that names the device it is used on (localhost, 127.x, ::1, 0.0.0.0). */
function loopback(a: string): boolean {
  const hostPort = a.trim().toLowerCase().replace(/^[a-z][a-z0-9+.-]*:\/\//, "").split("/")[0];
  const host = hostPort.startsWith("[") ? hostPort.slice(1, hostPort.indexOf("]")) : hostPort.replace(/:\d*$/, "");
  return host === "localhost" || host.endsWith(".localhost") || /^127\./.test(host) || host === "::1" || host === "0.0.0.0";
}

/** Ticked until the user says otherwise: a hub that answers, and one that
 *  doesn't answer now (hubs can be added while unreachable), except a
 *  loopback address that doesn't answer here: it names the other device. */
const wanted = (value: string, c: Check) => c.k === "ok" || (c.k === "bad" && !loopback(value));

function fromProbe(p: Probe): Check {
  if (p.result === "connected") return { k: "ok", name: p.name, found: p.discovered ? p.url : undefined };
  if (p.result === "invalid") return { k: "invalid", error: p.error };
  return { k: "bad", error: p.result === "not_a_hub" ? "not a hub: " + p.error : p.error };
}

function fromHub(h: HubRow, key: number): Row {
  const check: Check = !h.address.trim() ? { k: "empty" }
    : h.reachable ? { k: "ok", name: h.name || short(h.address) }
    : { k: "bad", error: h.error || "no answer" };
  return { key, theirs: h.theirs, value: short(h.address), candidates: h.candidates, on: wanted(h.address, check), touched: false, check, added: false };
}

/** The review's state: rows as they arrived (ticked as `wanted` says), the
 *  edits, and what Confirm would send. */
export function useHubReview(hubs: HubRow[]) {
  const [rows, setRows] = useState<Row[]>(() => hubs.map(fromHub));
  const next = useRef(hubs.length);
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());
  /** The latest check per row: an answer for an older one is dropped. */
  const seqs = useRef(new Map<number, number>());
  useEffect(() => {
    const t = timers.current, s = seqs.current;
    return () => { t.forEach(clearTimeout); t.clear(); s.clear(); };
  }, []);

  const patch = (key: number, p: Partial<Row>) => setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...p } : r)));
  /** A check's outcome; an untouched row's tick follows it (still checking: unchanged). */
  const settle = (key: number, c: Check) => setRows((rs) => rs.map((r) => (r.key !== key ? r
    : { ...r, check: c, on: r.touched || c.k === "checking" ? r.on : wanted(r.value, c) })));
  const stop = (key: number) => {
    const t = timers.current.get(key);
    if (t !== undefined) clearTimeout(t);
    timers.current.delete(key);
  };
  /** Check `value` for row `key` after `delay` ms (typing restarts the wait). */
  const check = (key: number, value: string, delay: number) => {
    stop(key);
    const my = (seqs.current.get(key) ?? 0) + 1;
    seqs.current.set(key, my);
    const input = value.trim();
    if (!input) { settle(key, { k: "empty" }); return; }
    settle(key, { k: "checking" });
    timers.current.set(key, setTimeout(() => {
      timers.current.delete(key);
      const answer = (c: Check) => { if (seqs.current.get(key) === my) settle(key, c); };
      api.probeHub(input).then((p) => answer(fromProbe(p)), (e) => answer({ k: "bad", error: errText(e) }));
    }, delay));
  };

  const ticked = rows.filter((r) => r.on);
  const urls: string[] = [];
  for (const r of ticked) {
    // a bare host that answered on another port is saved with it
    const v = (r.check.k === "ok" && r.check.found ? short(r.check.found) : "") || r.value.trim();
    if (v && !urls.some((u) => same(u, v))) urls.push(v);
  }
  return {
    rows,
    /** The ticked addresses as shown, trimmed, in order (empty ones and repeats skipped). */
    urls,
    /** A ticked row is empty or no address at all: Confirm waits for it. */
    blocked: ticked.some((r) => !r.value.trim() || r.check.k === "invalid"),
    /** Nothing ticked: this device gets no hub. */
    none: ticked.length === 0,
    /** Typing in a row checks it once typing pauses. */
    edit: (key: number, value: string) => { patch(key, { value }); check(key, value, DEBOUNCE); },
    /** One of the addresses tried: fill it in and check it now. */
    pick: (key: number, value: string) => { patch(key, { value }); check(key, value, 0); },
    recheck: (key: number, value: string) => check(key, value, 0),
    tick: (key: number, on: boolean) => patch(key, { on, touched: true }),
    /** A hub added here: empty and ticked (the user's own choice). */
    add: () => {
      const key = next.current++;
      setRows((rs) => [...rs, { key, theirs: null, value: "", candidates: [], on: true, touched: true, check: { k: "empty" }, added: true }]);
    },
    remove: (key: number) => { stop(key); seqs.current.delete(key); setRows((rs) => rs.filter((r) => r.key !== key)); },
  };
}
export type HubReviewState = ReturnType<typeof useHubReview>;

/** The rows, "Add a hub", and a note when no hub is ticked. `head`: a title over the rows. */
export function HubReview({ rev, disabled, head }: { rev: HubReviewState; disabled?: boolean; head?: ReactNode }) {
  const platform = usePlatform();
  const device = platform === "android" ? "phone" : "PC";
  return (
    <div className="hubrev">
      {head ? <div className="hr-h">{head}</div> : null}
      {rev.rows.map((r) => <HubLine key={r.key} r={r} rev={rev} disabled={disabled} />)}
      <button className="btn ghost hr-add" onClick={rev.add} disabled={disabled}><Icon name="add" />Add a hub</button>
      {rev.none ? <NoteCard icon="warning" warn>No hub: this {device} won't connect until you add one in Settings › Hubs.</NoteCard> : null}
    </div>
  );
}

function HubLine({ r, rev, disabled }: { r: Row; rev: HubReviewState; disabled?: boolean }) {
  const platform = usePlatform();
  const c = r.check;
  // the other addresses tried, offered when the one shown doesn't answer
  const tries = c.k === "bad" || c.k === "invalid" ? r.candidates.filter((x) => x.trim() && !same(x, r.value)) : [];
  let status: ReactNode;
  if (c.k === "checking") status = <span className="hr-s busy"><span className="spin" />Checking…</span>;
  else if (c.k === "ok") status = <span className="hr-s ok"><Icon name="check_circle" />{c.found ? <span>Found <b>{c.name}</b> {foundWhere(c.found)}</span> : <b>{c.name}</b>}</span>;
  else if (c.k === "empty") status = <span className="hr-s">Type the address this {platform === "android" ? "phone" : "PC"} should use</span>;
  else {
    status = (
      <span className="hr-s bad"><Icon name="error" /><span>{c.error}{c.k === "bad"
        ? <> <button className="link" onClick={() => rev.recheck(r.key, r.value)} disabled={disabled}>Check again</button></> : null}</span></span>
    );
  }
  return (
    <div className={"hr-row" + (r.on ? "" : " off")}>
      <input type="checkbox" className="hr-tick" checked={r.on} disabled={disabled} aria-label={"Bring " + (short(r.value) || "this hub")} onChange={(e) => rev.tick(r.key, e.target.checked)} />
      <div className="hr-main">
        <label className={"input" + (c.k === "invalid" ? " bad" : "")}>
          <input type="text" value={r.value} disabled={disabled} autoFocus={r.added} aria-label="Hub address" placeholder="e.g. hub.office.lan"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false} inputMode="url"
            onChange={(e) => rev.edit(r.key, e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); rev.recheck(r.key, r.value); } }} />
        </label>
        <div className="hr-st">
          {status}
          {r.theirs && !same(r.theirs, r.value)
            ? <span className="hr-theirs">{platform === "android" ? "Your PC" : "Your other device"} uses <span className="mono">{short(r.theirs)}</span></span> : null}
        </div>
        {tries.length ? (
          <div className="hr-try"><span>Try:</span>{tries.map((x) => <button key={x} className="chip" onClick={() => rev.pick(r.key, short(x))} disabled={disabled}>{short(x)}</button>)}</div>
        ) : null}
      </div>
      {r.added ? <button className="icon-btn hr-x" onClick={() => rev.remove(r.key)} disabled={disabled} aria-label="Remove this hub" title="Remove"><Icon name="close" /></button> : null}
    </div>
  );
}
