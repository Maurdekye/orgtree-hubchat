// Hand a file the app made to the browser (spec R1.4, 6.3): a page cannot
// write to a folder as the apps do.
import { isApplePhoneOrPad } from "./device";

/** Save a small text file: the share sheet on an iPhone or iPad (Save to Files, Notes, AirDrop; a
 *  download does not work in a Home Screen app there), a download in every other browser. False: the
 *  user closed the share sheet without choosing. Call it in the click itself: a browser shares and
 *  downloads only for one. */
export async function saveText(name: string, text: string): Promise<boolean> {
  const file = new File([text], name, { type: "text/plain" });
  if (isApplePhoneOrPad() && typeof navigator.share === "function" && navigator.canShare?.({ files: [file] })) {
    try {
      await navigator.share({ files: [file] });
      return true;
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return false;
      // the share sheet would not open (no click behind this call any more): a download is still worth a try
    }
  }
  const url = URL.createObjectURL(file);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.style.display = "none";
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  return true;
}
