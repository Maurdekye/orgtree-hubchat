// Files the user picks in Hubchat Web. The apps name a picked file by its
// path; a page has the file itself, so it stands in the commands' arguments
// as a token ("web-file:3:hubchat-key.hubchat-key") that webApi.ts swaps for
// what the core needs.

const PREFIX = "web-file:";
const picked = new Map<string, File>();
let last = 0;

/** Let the user choose one file. Null: they closed the chooser. Call it in a click. */
export function pickFile(accept: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.style.display = "none";
    let settled = false;
    const settle = (token: string | null) => {
      if (settled) return;
      settled = true;
      input.remove();
      resolve(token);
    };
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) { settle(null); return; }
      // the name ends the token, so the UI's baseName() shows it
      const token = PREFIX + (++last) + ":" + file.name.replace(/[\\/:]/g, "_");
      picked.set(token, file);
      settle(token);
    };
    // a chooser closed without a file
    input.addEventListener("cancel", () => settle(null));
    document.body.appendChild(input);
    input.click();
  });
}

/** Let the user choose files to send. Empty: they closed the chooser. Call it in a click. */
export function pickFilesToSend(): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.style.display = "none";
    let settled = false;
    const settle = (files: File[]) => {
      if (settled) return;
      settled = true;
      input.remove();
      resolve(files);
    };
    input.onchange = () => settle(Array.from(input.files ?? []));
    input.addEventListener("cancel", () => settle([]));
    document.body.appendChild(input);
    input.click();
  });
}

/** The text of a file picked with `pickFile`, at most `limit` bytes of it. Rejects with a sentence. */
export async function pickedText(token: string, limit: number): Promise<string> {
  const file = picked.get(token);
  if (!file) throw "Choose the file again.";
  if (file.size > limit) throw "That file is too large to be a Hubchat key file.";
  return await file.text();
}
