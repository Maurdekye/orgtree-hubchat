// Inline mentions of other Hubchat clients (user 2026-10-29, docket
// hubchat-inline-mentions-of-other-hubchat-clients): a mention of someone one
// of my hubs lists carries their DM-list avatar (picture, else letters; an
// agent session its terminal icon; me my own avatar), anyone else a small grey
// dot, a tooltip saying they aren't on my hubs, muted text and no chat to open.
//
// MentionMark is the mark alone and reads the store itself, so it follows hub,
// directory and picture changes wherever it sits. useMentionMarks places one
// in each mention link of a message: that HTML is md()'s, sanitized and set
// with dangerouslySetInnerHTML, so the marks live outside it (portals into
// hosts added after each render) and the sanitizer's allow-list stays as it
// is. The letters are drawn by CSS (data-initials): a copied selection never
// picks them up.
import { useLayoutEffect, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { PictureMeta } from "../api";
import { avatarFallback } from "../lib/avatarFallback";
import { Icon } from "../lib/icons";
import { mentionReach } from "../lib/mentions";
import { kindOf, type Kind } from "../lib/peers";
import { usePicture } from "../lib/pictures";
import { useSnap } from "../lib/store";

/** The DM list's avatar at text size: the same picture, kind colours and shape. */
function MarkAvatar({ kind, name, address, picture }: { kind: Kind | "me"; name: string; address: string; picture: PictureMeta | null | undefined }) {
  const fallback = avatarFallback(kind, name);
  const src = usePicture(kind === "chat" ? undefined : address, picture);
  const [failed, setFailed] = useState<string | null>(null);
  const shown = src && src !== failed ? src : null;
  return (
    <span className={"mmark av " + fallback.cls} aria-hidden="true" data-initials={shown || kind === "chat" ? undefined : fallback.text}>
      {shown ? <img src={shown} alt="" onError={() => setFailed(shown)} /> : kind === "chat" ? <Icon name="terminal" /> : null}
    </span>
  );
}

/** A mention's mark: the avatar when one of my hubs lists the address, a grey
 *  dot when none does, nothing before any directory arrived. */
export function MentionMark({ address }: { address: string }) {
  const snap = useSnap();
  const r = mentionReach(address, snap);
  if (r.state === "unknown") return null;
  if (r.state === "off") return <span className="mmark mdot" aria-hidden="true" />;
  const me = snap.state?.me;
  return r.me && me
    ? <MarkAvatar kind="me" name={me.name || me.id} address={address} picture={me.picture} />
    : <MarkAvatar kind={kindOf(r.contact)} name={r.name} address={address} picture={r.contact?.picture} />;
}

/** The link around a mark: its tooltip, and "off" (muted, opens nothing) when
 *  none of my hubs lists the address. */
function MentionLink({ anchor, address }: { anchor: HTMLElement; address: string }) {
  const r = mentionReach(address, useSnap());
  useLayoutEffect(() => {
    anchor.title = r.title;
    anchor.classList.toggle("off", r.state === "off");
    if (r.state === "off") anchor.setAttribute("aria-disabled", "true"); else anchor.removeAttribute("aria-disabled");
  }, [anchor, r.title, r.state]);
  return <MentionMark address={address} />;
}

interface Host { host: HTMLElement; anchor: HTMLElement; address: string }

/** Marks for every mention link inside `root`. Render the result inside the
 *  same element whose handlers should see clicks on a mark: React passes a
 *  portal's events up its own tree. React replaces the HTML when it changes;
 *  the new links then get new hosts. */
export function useMentionMarks(root: RefObject<HTMLElement | null>): ReactNode[] {
  const [hosts, setHosts] = useState<Host[]>([]);
  useLayoutEffect(() => {
    const links = root.current ? [...root.current.querySelectorAll<HTMLElement>("a.addr.mention")] : [];
    const next = links.map((anchor) => {
      let host = anchor.firstElementChild as HTMLElement | null;
      if (!host?.classList.contains("mmark-host")) {
        host = document.createElement("span");
        host.className = "mmark-host";
        host.setAttribute("aria-hidden", "true");
        anchor.prepend(host);
      }
      return { host, anchor, address: anchor.dataset.slug || "" };
    });
    setHosts((prev) => (prev.length === next.length && prev.every((p, i) => p.host === next[i].host && p.address === next[i].address) ? prev : next));
  });
  return hosts.map((h, i) => createPortal(<MentionLink anchor={h.anchor} address={h.address} />, h.host, i + ":" + h.address));
}
