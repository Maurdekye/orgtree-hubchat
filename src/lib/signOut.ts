// Settings › Devices › Sign out of this device: the words of its confirmation (the lost-key screen's
// Start over in Settings' dialog), and the order Hubchat Web leaves in.

export interface SignOutText { action: string; title: string; body: string; unsaved: string; ok: string; busy: string; cancel: string }

/** `device`: the device word ("PC", "phone", "browser", "device"). */
export function signOutText(device: string): SignOutText {
  return {
    action: "Sign out of this device",
    title: "Sign out of this device?",
    body: `This removes this ${device}'s chats, hubs and settings. Your address keeps working on your other devices, and you can bring it back here later by linking.`,
    // shown only while the recovery words aren't marked saved: on a person's only device, signing out
    // without them loses the identity for good
    unsaved: "Your recovery words aren't marked as saved. If no other device has your identity, save them first in Settings › Recovery words, or it can't come back.",
    ok: "Sign out",
    busy: "Signing out…",
    cancel: "Keep them",
  };
}

/** After signing out: the hubs that couldn't be told (unreachable, refused, or out of time), by name.
 *  null when every hub was told. */
export function unreachedText(hubs: string[], device: string): string | null {
  if (!hubs.length) return null;
  const names = hubs.length === 1 ? hubs[0] : hubs.slice(0, -1).join(", ") + " and " + hubs[hubs.length - 1];
  return `Signed out. Hubchat couldn't reach ${names}, so ${hubs.length === 1 ? "it" : "they"} may still list this ${device}.`;
}

/** The screen after a sign-out that stopped partway (the key or the chats are still here). */
export function stoppedText(device: string): { title: string; body: string; retry: string; busy: string } {
  return {
    title: "Hubchat couldn't finish signing out",
    body: `Signing out of this ${device} stopped partway: some of its key, chats or settings are still here. Try again to remove them.`,
    retry: "Try again",
    busy: "Signing out…",
  };
}

/** Tell the hubs to forget this browser's push subscription while the key can still ask, then `then`.
 *  A hub that can't be told forgets nothing, but the subscription itself is gone, so it never stops
 *  the sign-out (or Start over). */
export async function afterForgettingPush<T>(pushOff: () => Promise<unknown>, then: () => Promise<T>): Promise<T> {
  try { await pushOff(); } catch { /* see above */ }
  return then();
}
