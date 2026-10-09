// Scan setup code (user 2026-10-09 10:38Z, mockup v2; texts from
// hubchat-ux-flow.md §3b): after a scan or a hubchat://setup link, check
// the phone side in order and show only the first check that fails: a
// newer Hubchat needed, the Tailscale app (Android, net=tailscale), the PC
// reachable. Then the name (no identity yet) or "Add {pc}" (one exists),
// join the PC's hub, message the org with the code and open its chat.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type SetupLink } from "../api";
import { Icon } from "../lib/icons";
import { errText } from "../lib/native";
import { endSetup, openAfterSetup, scanSetup, updateHubchat, openOther, wantLinkInstead, type SetupReq } from "../lib/setup";
import { getSnap, refreshAll, refreshState, setOnboarding } from "../lib/store";
import { Addr, Avatar, Modal, ModalHead, usePlatform } from "./ui";
import { idFromName } from "./Onboarding";
import "../styles/setup.css";

type Fail = "update" | "damaged" | "no-ts" | "unreach" | "unreach-wifi" | "join";
type St = { k: "check" } | { k: "ok" } | { k: "fail"; f: Fail; why?: string } | { k: "name" } | { k: "add" } | { k: "joining" };

/** Tailscale's Android package (the manifest names it in <queries>). */
const TAILSCALE = "com.tailscale.ipn";

export function SetupFlow({ req }: { req: SetupReq }) {
  const platform = usePlatform();
  const android = platform === "android";
  const [link, setLink] = useState<SetupLink | null>(null);
  const [st, setSt] = useState<St>({ k: "check" });
  const [attempt, setAttempt] = useState(0);
  const [name, setName] = useState("");
  const close = () => endSetup();

  // the checks, in order; each shows only if it fails
  useEffect(() => {
    let live = true;
    setSt({ k: "check" });
    void (async () => {
      const fail = (f: Fail, why?: string) => { if (live) setSt({ k: "fail", f, why }); };
      const p = await api.parseSetup(req.input);
      if (!live) return;
      if (!p.link) return fail(p.error?.kind === "needs_newer" ? "update" : "damaged");
      const l = p.link;
      setLink(l);
      if (android && l.net === "tailscale") {
        const has = await api.appInstalled(TAILSCALE).catch(() => null);
        if (has === false) return fail("no-ts");
      }
      const r = await api.setupCheck(l.hub, l.hubname).catch((e) => ({ reachable: false, error: errText(e) }));
      if (!r.reachable) return fail(l.net === "wifi" ? "unreach-wifi" : "unreach", r.error ?? undefined);
      if (!live) return;
      setSt({ k: "ok" });
      setTimeout(() => { if (live) setSt({ k: getSnap().state?.me ? "add" : "name" }); }, 700);
    })().catch((e) => { if (live) setSt({ k: "fail", f: "damaged", why: errText(e) }); });
    return () => { live = false; };
  }, [req.input, attempt, android]);

  // Android: the screen holds one history entry of its own, so the system
  // back button leaves it (as JoinScreen does)
  const alive = useRef(false);
  useEffect(() => {
    if (!android) return;
    alive.current = true;
    if (history.state?.setup !== req.n) {
      const d = (history.state && typeof history.state.hc === "number" ? history.state.hc : 1) as number;
      history.pushState({ hc: d, setup: req.n }, "");
    }
    const pop = () => { if (history.state?.setup !== req.n) endSetup(); };
    window.addEventListener("popstate", pop);
    return () => {
      alive.current = false;
      window.removeEventListener("popstate", pop);
      // closed by a button: drop the entry (not on StrictMode's re-mount)
      setTimeout(() => { if (!alive.current && history.state?.setup === req.n) history.back(); }, 0);
    };
  }, [android, req.n]);

  // Escape closes on desktop, as the modal's ×
  useEffect(() => {
    if (android) return;
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); endSetup(); } };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [android]);

  const busy = useRef(false);
  /** Make the identity if there is none, join the hub, send the code, open the chat. */
  const join = async () => {
    if (!link || busy.current) return;
    busy.current = true;
    setSt({ k: "joining" });
    try {
      let me = getSnap().state?.me;
      if (!me) {
        const nm = name.trim();
        const chk = await api.checkId(idFromName(nm, 24));
        const id = chk.ok ? idFromName(nm, chk.max_len) : "me";
        await api.createIdentity(id, nm);
        await refreshState();
        me = getSnap().state?.me;
      }
      const peer = await api.setupStart(req.input, me?.name || name.trim() || me?.id || "me");
      endSetup();
      setOnboarding(false);
      await refreshAll();
      openAfterSetup(peer);
    } catch (e) {
      setSt({ k: "fail", f: "join", why: errText(e) });
    } finally {
      busy.current = false;
    }
  };

  const org = link?.orgname ?? "";
  const pc = link?.pc ?? "";
  const blk = android ? " block" : "";
  const retry = () => setAttempt((a) => a + 1);
  const acts = (b: ReactNode) => <div className="acts">{b}</div>;
  const sm = (label: ReactNode, onClick: () => void, primary?: boolean) => (
    <button className={"btn" + (primary ? " primary" : "")} onClick={onClick}>{label}</button>
  );

  let title: ReactNode = link ? <>Chat with {org}</> : "Scan setup code";
  let body: ReactNode;
  let foot: ReactNode = null;
  if (st.k === "name") {
    title = <>Chat with {org}.</>;
    const ok = !!name.trim();
    body = (
      <>
        <div className="field">
          <label htmlFor="setup-name">Your name</label>
          <label className="input"><input id="setup-name" value={name} autoFocus maxLength={48} autoComplete="off" placeholder="e.g. Alex Rivera"
            onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && ok) void join(); }} /></label>
        </div>
        <p className="setup-instead"><button className="link" onClick={() => { endSetup(); wantLinkInstead(); }}>Already use Hubchat on another device? Link this phone instead.</button></p>
      </>
    );
    foot = <button className={"btn primary" + blk} disabled={!ok} onClick={() => void join()}>Continue</button>;
  } else if (st.k === "add" && link) {
    title = <>Add {pc} and chat with {org}?</>;
    body = (
      <div className="found">
        <Avatar kind="org" name={org} size={48} />
        <span className="t"><b>{org}</b><span className="mono obaddr"><Addr a={link.org} /></span><span>through {pc}</span></span>
      </div>
    );
    foot = <>{android ? null : <button className="btn ghost" onClick={close}>Not now</button>}<button className={"btn primary" + blk} onClick={() => void join()}>Add</button>{android ? <button className="btn ghost block" onClick={close}>Not now</button> : null}</>;
  } else {
    let card: ReactNode;
    if (st.k === "check") card = <div className="probe-card busy"><span className="spin" /><div>Checking the setup code…</div></div>;
    else if (st.k === "joining") card = <div className="probe-card busy"><span className="spin" /><div>Joining {pc}'s hub…</div></div>;
    else if (st.k === "ok" && link) card = <div className="probe-card ok"><Icon name="check_circle" /><div><b>Found {pc}</b>{org} · <span className="mono"><Addr a={link.org} /></span></div></div>;
    else if (st.k === "fail") {
      const ts = link?.ts ?? "";
      // what went wrong underneath stays out of the text (plain words only): on hover at most
      const bad = (head: ReactNode, rest: ReactNode, buttons: ReactNode) => (
        <div className="probe-card bad" data-fail={st.f} title={st.why}><Icon name="error" /><div><b>{head}</b>{rest}{buttons ? acts(buttons) : null}</div></div>
      );
      if (st.f === "update") card = bad("This code needs a newer Hubchat.", null, sm("Update", () => void updateHubchat(platform).catch(() => {}), true));
      else if (st.f === "damaged") card = bad("This setup code is damaged.", " Show a new one on your PC and scan again.", android ? sm("Scan again", () => { endSetup(); void scanSetup(); }, true) : null);
      else if (st.f === "no-ts") card = (
        <div className="probe-card bad" data-fail={st.f} title={st.why}><Icon name="error" /><div><b className="inl">Install Tailscale on this phone</b> and sign in as <b className="inl">{ts}</b>, the same account as your PC. Then come back here.
          {acts(<>{sm("Get Tailscale", () => void openOther("get_tailscale"), true)}{sm("Try again", retry)}</>)}</div></div>
      );
      else if (st.f === "unreach") card = (
        <div className="probe-card bad" data-fail={st.f} title={st.why}><Icon name="error" /><div><b>Can't reach {pc}.</b>Is Tailscale on and signed in on this phone? Use <b className="inl">{ts}</b>, the same account as your PC.
          {acts(<>{android ? sm("Open Tailscale", () => void openOther("open_tailscale"), true) : null}{sm("Try again", retry, !android)}</>)}</div></div>
      );
      else if (st.f === "unreach-wifi") card = bad(<>Can't reach {pc}.</>, "Is this phone on the same Wi-Fi as your PC?",
        <>{android ? sm("Open Wi-Fi settings", () => void openOther("wifi_settings"), true) : null}{sm("Try again", retry, !android)}</>);
      else card = bad(<>Couldn't join {pc}'s hub.</>, " Try again.", sm("Try again", () => void join(), true));
    }
    body = <>{link ? <p className="lead">Connecting to {pc}'s hub.</p> : null}{card}</>;
  }

  if (android) {
    return (
      <div className="screen setup-over"><div className="ob">
        <button className="icon-btn obback" onClick={close} aria-label="Back"><Icon name="back" /></button>
        <div className="ob-body" style={{ paddingTop: 4 }}><h2>{title}</h2>{body}</div>
        {foot ? <div className="ob-foot">{foot}</div> : null}
      </div></div>
    );
  }
  return (
    <Modal onClose={() => {}} className="setup">
      <ModalHead title={title} onClose={close} />
      <div className="modal-b">{body}</div>
      {foot ? <div className="modal-f">{foot}</div> : null}
    </Modal>
  );
}
