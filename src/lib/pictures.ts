import { useEffect, useState } from "react";
import { api, type PictureMeta } from "../api";
import { getSnap, useSnap } from "./store";

// Only native-verified PNG data enters this cache. Scope every entry and late
// completion to its identity; never retain bytes after an identity switch.
const cache = new Map<string, string>();
const pending = new Map<string, Promise<string | null>>();
let cacheOwner: string | null = null;
function scope(owner: string | null) {
  if (cacheOwner !== owner) { cacheOwner = owner; cache.clear(); pending.clear(); }
}
export function usePicture(peer: string | undefined, meta: PictureMeta | null | undefined): string | null {
  const snap = useSnap();
  const owner = snap.state?.me?.address ?? null;
  const revision = meta?.revision;
  const hash = meta?.sha256;
  const identity = meta?.picture_owner ?? null;
  const hubs = snap.state?.hubs.map(h => h.url + ":" + h.state).join("|") ?? "";
  const key = owner && peer && revision && hash ? JSON.stringify([owner, peer, identity, revision, hash]) : null;
  const [shown, setShown] = useState<{ key: string; src: string } | null>(null);
  scope(owner);
  useEffect(() => {
    let live = true;
    if (!key || !peer) return;
    if (cache.has(key)) { setShown({ key, src: cache.get(key)! }); return; }
    let promise = pending.get(key);
    if (!promise) {
      promise = api.picture(peer).then(p => {
        if (!p?.data || (p.picture_owner ?? null) !== identity || p.revision !== revision || p.sha256 !== hash || getSnap().state?.me?.address !== owner) return null;
        const src = "data:image/png;base64," + p.data;
        scope(owner);
        cache.set(key, src);
        while (cache.size > 80) cache.delete(cache.keys().next().value!);
        return src;
      }, () => null).finally(() => { if (pending.get(key) === promise) pending.delete(key); });
      pending.set(key, promise);
    }
    promise.then(src => { if (live && src && getSnap().state?.me?.address === owner) setShown({ key, src }); });
    return () => { live = false; };
  }, [key, peer, identity, revision, hash, owner, hubs]);
  return key && shown?.key === key ? shown.src : null;
}
