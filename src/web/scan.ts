// Scan a QR code with the camera (the desktop app's link QR; user
// 2026-11-11). Where the browser reads QR codes itself (BarcodeDetector:
// Chrome on Android), it is asked first. A browser on an iPhone has no such
// reader, so every frame also goes to the core's worker, which reads it
// (crates/hubchat-web scan.rs). The camera runs only while this screen is
// up; no frame is kept.
//
// A code on a monitor is small in the picture (the first real try, user
// 2026-11-11 11:53Z, failed on frames scaled down to 800 pixels): the camera
// is asked for 1080p and the frames go to the reader at that size.

/** Reads one frame: its size and its grey pixels, one byte each. Null: no code in it. */
export type ReadFrame = (width: number, height: number, grey: Uint8Array) => Promise<string | null>;

/** Frames larger than this on their longer side are scaled down to it. A 1080p camera frame is not. */
const LONG_SIDE = 1920;

/** The browser's own QR reader, where it has one. */
interface NativeReader { detect(source: HTMLVideoElement): Promise<{ rawValue?: string }[]> }
async function nativeReader(): Promise<NativeReader | null> {
  const Detector = (window as unknown as { BarcodeDetector?: { new (o: { formats: string[] }): NativeReader; getSupportedFormats?: () => Promise<string[]> } }).BarcodeDetector;
  if (!Detector) return null;
  try {
    const formats = (await Detector.getSupportedFormats?.()) ?? [];
    return formats.includes("qr_code") ? new Detector({ formats: ["qr_code"] }) : null;
  } catch { return null; }
}
/** A pause between frames, so the scan does not take the whole core. */
const PAUSE_MS = 120;

/** Why the camera did not open, as a sentence. */
function cameraError(e: unknown): string {
  const name = e instanceof DOMException ? e.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") return "Hubchat needs the camera to scan a code. Allow the camera for this page in your browser's settings, then try again.";
  if (name === "NotFoundError" || name === "OverconstrainedError") return "Hubchat found no camera on this device. Type the code instead.";
  if (name === "NotReadableError" || name === "AbortError") return "The camera is in use by another app. Close that app, then try again.";
  return "Hubchat couldn't open the camera. Type the code instead.";
}

/** Red, green, blue, alpha to one grey byte a pixel. */
export function toGrey(rgba: Uint8ClampedArray): Uint8Array {
  const grey = new Uint8Array(rgba.length >> 2);
  for (let i = 0, p = 0; i < grey.length; i++, p += 4) grey[i] = (rgba[p] * 77 + rgba[p + 1] * 150 + rgba[p + 2] * 29) >> 8;
  return grey;
}

/** Open the camera over the app and wait for a QR code. Null: the user closed it. Rejects with a
 *  sentence when the camera cannot be used. */
export async function scanQr(read: ReadFrame): Promise<string | null> {
  if (!navigator.mediaDevices?.getUserMedia) throw "This browser gives Hubchat no camera. Type the code instead.";
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: false, video: { facingMode: { ideal: "environment" }, width: { ideal: 1920 }, height: { ideal: 1080 } } });
  } catch (e) {
    throw cameraError(e);
  }

  // keep refocusing as the phone moves, where the camera can be told to (it is the default on most)
  const track = stream.getVideoTracks()[0];
  const modes = (track?.getCapabilities?.() as { focusMode?: string[] } | undefined)?.focusMode;
  if (track && modes?.includes("continuous")) void track.applyConstraints({ advanced: [{ focusMode: "continuous" } as MediaTrackConstraintSet] }).catch(() => {});
  let native = await nativeReader();

  const box = document.createElement("div");
  box.className = "web-scan";
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", "Scan a QR code");
  const video = document.createElement("video");
  // an iPhone plays a video full screen unless it is told to play in the page
  video.playsInline = true;
  video.muted = true;
  video.autoplay = true;
  video.srcObject = stream;
  const hint = document.createElement("div");
  hint.className = "web-scan-hint";
  hint.textContent = "Point the camera at the QR code. Move closer if it isn't read.";
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "btn web-scan-cancel";
  cancel.textContent = "Cancel";
  box.append(video, hint, cancel);
  document.body.appendChild(box);
  cancel.focus();

  const canvas = document.createElement("canvas");
  const pen = canvas.getContext("2d", { willReadFrequently: true });
  let done = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  return new Promise<string | null>((resolve, reject) => {
    const finish = (text: string | null, error?: string) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("visibilitychange", onHide);
      for (const t of stream.getTracks()) t.stop();
      video.srcObject = null;
      box.remove();
      if (error) reject(error); else resolve(text);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); finish(null); } };
    // a page in the background loses the camera: close, so the user opens it again knowingly
    const onHide = () => { if (document.visibilityState !== "visible") finish(null); };
    const step = async () => {
      if (done) return;
      const w = video.videoWidth, h = video.videoHeight;
      if (native && w > 0 && video.readyState >= 2) {
        try {
          const text = (await native.detect(video)).find((c) => c.rawValue)?.rawValue;
          if (text) { finish(text); return; }
        } catch { native = null; /* the core reads from here on */ }
        if (done) return;
      }
      if (pen && w > 0 && h > 0 && video.readyState >= 2) {
        const k = Math.min(1, LONG_SIDE / Math.max(w, h));
        canvas.width = Math.max(1, Math.round(w * k));
        canvas.height = Math.max(1, Math.round(h * k));
        pen.drawImage(video, 0, 0, canvas.width, canvas.height);
        try {
          const text = await read(canvas.width, canvas.height, toGrey(pen.getImageData(0, 0, canvas.width, canvas.height).data));
          if (text) { finish(text); return; }
        } catch (e) {
          finish(null, typeof e === "string" ? e : "Hubchat couldn't read the camera's picture.");
          return;
        }
      }
      timer = setTimeout(() => void step(), PAUSE_MS);
    };
    cancel.onclick = () => finish(null);
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("visibilitychange", onHide);
    stream.getVideoTracks()[0]?.addEventListener("ended", () => finish(null, "The camera stopped. Try again."));
    void video.play().catch(() => { /* autoplay starts it */ });
    void step();
  });
}
