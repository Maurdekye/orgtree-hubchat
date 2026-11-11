// The identity's key in this browser (spec R3.1). IndexedDB database
// `hubchat-keys`, store `identity`, one record: { v: 1, id, iv, ciphertext,
// key }. `key` is an AES-GCM 256 CryptoKey made with extractable: false, and
// the secret is encrypted with it, so the browser's storage files never hold
// the secret in the clear (roughly what the desktop gets from the system key
// store). It is no defence against script running in this origin: nothing in
// a web page is.
//
// Runs in the core's worker; IndexedDB and WebCrypto are both there. The
// server that serves the page never sees any of this.

const DB = "hubchat-keys";
const STORE = "identity";
const RECORD = "me";

export interface KeptIdentity { id: string; secret: string }

interface KeyRecord { v: 1; id: string; iv: Uint8Array<ArrayBuffer>; ciphertext: ArrayBuffer; key: CryptoKey }

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => { r.result.createObjectStore(STORE); };
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error ?? new Error("the browser's key store didn't open"));
    r.onblocked = () => reject(new Error("the browser's key store is held by another tab"));
  });
}

async function run<T>(mode: IDBTransactionMode, work: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const req = work(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(req.result);
      tx.onabort = tx.onerror = () => reject(tx.error ?? req.error ?? new Error("the browser's key store refused"));
    });
  } finally {
    db.close();
  }
}

/** The key this browser keeps, or null when it keeps none. Throws when there is one it cannot read. */
export async function loadIdentity(): Promise<KeptIdentity | null> {
  const rec = await run<KeyRecord | undefined>("readonly", (s) => s.get(RECORD));
  if (!rec) return null;
  if (rec.v !== 1 || typeof rec.id !== "string" || !rec.key) throw new Error("this browser keeps a key Hubchat can't read");
  const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: rec.iv }, rec.key, rec.ciphertext);
  return { id: rec.id, secret: new TextDecoder().decode(plain) };
}

/** Keep the key (replacing any other). Resolves only once the browser has committed it. */
export async function saveIdentity(id: string, secret: string): Promise<void> {
  const key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, new TextEncoder().encode(secret));
  const rec: KeyRecord = { v: 1, id, iv, ciphertext, key };
  await run("readwrite", (s) => s.put(rec, RECORD));
}

/** Forget the key (Start over). */
export async function clearIdentity(): Promise<void> {
  await run("readwrite", (s) => s.delete(RECORD));
}
