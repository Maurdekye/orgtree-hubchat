// Inline video and audio (Hubchat 1.2.7; user 2026-10-30 14:24Z and 14:33Z), as Orgtree's media card plays them
// (renderer canvas/media.tsx after its 4.5.3 fix, kept in step for symmetry): a video opens on render, its player and
// first frame showing while it is on screen (preload="metadata"), and only its own controls play it, fullscreen
// included; audio waits for Play audio, nothing fetched before it. Scrolled away, hidden, or gone with its chat, a
// player lets go of its file and comes back paused: it never plays on its own. The file row below keeps Download,
// Open and progress. The bytes come through hcmedia (lib/media.ts), never a download.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type Attachment, type Message } from "../api";
import { MEDIA_MAX_BYTES, NOT_SUPPORTED, TOO_BIG, UNAVAILABLE, mediaFormat } from "../lib/media";

export function AttMedia({ a, m, fallback }: { a: Attachment; m: Message; fallback: ReactNode }) {
  const format = mediaFormat(a.name);
  const kind = format?.[0] ?? "video";
  const src = format ? api.mediaUrl(m.id, a.local_id) : null;
  const tooBig = a.bytes > MEDIA_MAX_BYTES;
  const [active, setActive] = useState(false), [error, setError] = useState("");
  const [onScreen, setOnScreen] = useState(typeof IntersectionObserver === "undefined");
  const element = useRef<HTMLMediaElement | null>(null), card = useRef<HTMLDivElement>(null);
  const version = useRef(0), live = useRef(true);
  const release = (media = element.current) => {
    version.current++;
    if (media?.hasAttribute("src")) { media.pause(); media.removeAttribute("src"); media.load(); }
  };
  useEffect(() => {
    live.current = true;
    const mounted = element.current;
    let inView = typeof IntersectionObserver === "undefined", hidden = document.hidden;
    const settle = () => {
      // a player that has taken the whole screen is on screen, whatever its card reports; leaving fullscreen
      // settles again, so a card out of view then lets go
      if (mounted && document.fullscreenElement === mounted) return;
      const on = inView && !hidden;
      setOnScreen(on);
      if (!on) { release(); setActive(false); }
    };
    const observer = typeof IntersectionObserver === "undefined" ? null : new IntersectionObserver((entries) => {
      const latest = entries[entries.length - 1];
      if (latest) { inView = latest.isIntersecting; settle(); }
    });
    if (card.current) observer?.observe(card.current);
    const visibility = () => { hidden = document.hidden; settle(); };
    document.addEventListener("visibilitychange", visibility);
    document.addEventListener("fullscreenchange", settle);
    return () => {
      live.current = false; release(mounted); observer?.disconnect();
      document.removeEventListener("visibilitychange", visibility); document.removeEventListener("fullscreenchange", settle);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // a video asks for its metadata and first frame while its card is on screen, and lets go when it is not
  useEffect(() => {
    const media = element.current;
    if (kind !== "video" || !onScreen || !media || !format || !src || tooBig) return;
    if (!media.canPlayType(format[1])) { setError(NOT_SUPPORTED); return; }
    setError("");
    media.src = src;
    return () => release(media);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onScreen]);
  const fail = () => { release(); setActive(false); setError(UNAVAILABLE); };
  const play = () => {
    const media = element.current;
    if (!media || !format || !src || tooBig) return;
    if (!media.canPlayType(format[1])) { setError(NOT_SUPPORTED); return; }
    const token = ++version.current;
    setError(""); setActive(true);
    media.src = src;
    void media.play().catch(() => { if (live.current && version.current === token) fail(); });
  };
  const player = !tooBig && !!src && !!format;
  return (
    <div ref={card} className={"att-media " + kind} data-att={a.local_id}>
      {player ? (kind === "video"
        ? <video ref={(n) => { element.current = n; }} controls preload="metadata" playsInline hidden={!!error} aria-label={a.name} onError={fail} />
        : <audio ref={(n) => { element.current = n; }} controls preload="none" hidden={!active} aria-label={a.name} onError={fail} />) : null}
      {player && kind === "audio" && !active ? <button className="att-play" onClick={play}>Play audio</button> : null}
      {fallback}
      {error || tooBig ? <span className="att-note" role="status">{tooBig ? TOO_BIG : error}</span> : null}
    </div>
  );
}
