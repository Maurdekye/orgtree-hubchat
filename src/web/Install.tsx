// What an iPhone or iPad shows in a browser tab instead of the app (spec
// R3.6): how to put Hubchat on the Home Screen. A tab's storage there can be
// deleted after a week without a visit, and a Home Screen app has its own
// and keeps it; so no identity is made or restored in the tab, and the core
// is not even loaded.
import { Logo } from "../lib/icons";

/** The Share button, as the browser draws it. */
const ShareGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12M8 7l4-4 4 4M8 11H6.5A1.5 1.5 0 0 0 5 12.5v7A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5v-7a1.5 1.5 0 0 0-1.5-1.5H16" /></svg>
);
/** Add to Home Screen, as the share sheet draws it: a plus in a square. */
const AddGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4" /><path d="M12 8.5v7M8.5 12h7" /></svg>
);

export function Install() {
  return (
    <div className="boot gate install">
      <Logo size={56} />
      <b>Add Hubchat to your Home Screen</b>
      <span>Tap Share, then Add to Home Screen, then open Hubchat from there.</span>
      <ol className="taps" aria-label="How to add Hubchat to your Home Screen">
        <li><i><ShareGlyph /></i><span>Tap <b>Share</b></span></li>
        <li><i><AddGlyph /></i><span>Tap <b>Add to Home Screen</b></span></li>
        <li><i><Logo size={24} /></i><span>Open <b>Hubchat</b> from your Home Screen</span></li>
      </ol>
    </div>
  );
}
