// The saved text wrapper is local input, never an identity or an upload.
// Continue still owns BIP-39/checksum/id validation through restoreWords.
export const RECOVERY_FILE_LIMIT = 64 * 1024;
export const RECOVERY_FILE_ERROR = "Choose a Hubchat recovery words file, or a text file containing all 24 words in order.";

export function recoveryFileWords(text: string): string {
  if (text.length > RECOVERY_FILE_LIMIT) throw Error(RECOVERY_FILE_ERROR);
  const normalized = text.replace(/^\ufeff/, "").replace(/\r\n/g, "\n").trim();
  let words: string[];
  if (normalized.startsWith("Hubchat recovery words\n")) {
    const saved = /^Hubchat recovery words\n\nAddress: @net:[^\n]+\nSaved:[^\n]+\n\n([\s\S]*?)\n\nThese 24 words ARE your Hubchat identity:[\s\S]*$/.exec(normalized);
    if (!saved) throw Error(RECOVERY_FILE_ERROR);
    const lines = saved[1].split("\n");
    words = lines.map((line, i) => {
      const numbered = /^\s*(\d{1,2})\. ([a-z]+)\s*$/i.exec(line);
      if (!numbered || Number(numbered[1]) !== i + 1) throw Error(RECOVERY_FILE_ERROR);
      return numbered[2].toLowerCase();
    });
  } else {
    words = normalized.toLowerCase().split(/\s+/);
  }
  if (words.length !== 24 || words.some(w => !/^[a-z]+$/.test(w))) throw Error(RECOVERY_FILE_ERROR);
  return words.join(" ");
}
