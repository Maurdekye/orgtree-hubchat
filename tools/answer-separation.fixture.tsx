// Real Composer, preparation store and saved-answer lifecycle; native calls
// are replaced by the contained proof after its DOM is installed.
import { Composer, type ComposerApi } from '../src/components/Composer';
import { PlatformCtx, type Platform } from '../src/components/ui';
import { removeReply, sentDraftAnswers, setReplies, unpick, useDraftAnswers } from '../src/lib/answers';
import type { Message } from '../src/api';
import type { CardKnown } from '../src/lib/questions';
import type { Ref } from 'react';
export { api } from '../src/api';
export * as store from '../src/lib/store';
export * as answers from '../src/lib/answers';
export { act, createElement, createRef } from 'react';
export { createRoot } from 'react-dom/client';

export function Fixture({ peer, cards, platform, composerRef, cardState, limit = 1000000 }: {
  peer: string; cards: Message[]; platform: Platform; composerRef: Ref<ComposerApi>;
  cardState: (id: string) => CardKnown; limit?: number;
}) {
  const d = useDraftAnswers(peer);
  const byId = new Map(cards.map(m => [m.id, m]));
  const replies = d.replies.flatMap(id => byId.has(id) ? [byId.get(id)!] : []);
  const picked = Object.entries(d.picks).flatMap(([id, labels]) => byId.has(id) && cardState(id) === 'open' ? [{ card: byId.get(id)!, labels }] : []);
  return <PlatformCtx.Provider value={platform}><Composer peer={peer} c={undefined}
    hubs={[{ url: 'https://synthetic.invalid', name: 'Synthetic', state: 'connected', max_attachment_bytes: limit } as any]}
    replies={replies} picked={picked} cardState={cardState} pos={new Map(cards.map((m, i) => [m.id, i]))}
    onRemoveReply={id => removeReply(peer, id)} onUnpick={id => unpick(peer, [id])}
    onCancelReply={() => setReplies(peer, [])} onSent={m => sentDraftAnswers(peer, m)} ref={composerRef} />
  </PlatformCtx.Provider>;
}
