// RED STUB
export const LOCKED_TEXT = "";
type Lockable = { locked?: "open" | "closed" | string | null } | undefined;
export const lockedPresence = (_c: Lockable): { state: "locked"; text: string; short: string; ticks: false } | null => null;
export const canMessage = (_c: Lockable) => true;
export const lockBadge = (_c: Lockable) => false;
export const lockBarText = (_name: string) => "";
export const ownLockdownText = (_hubs: string[]): string | null => null;
