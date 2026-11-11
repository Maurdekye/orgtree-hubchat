// What the start screen says when Hubchat Web's core does not start
// (start.tsx). No imports: tools/web-start-error.test.ts runs it in Node.

/** How the core starts what it says of a database written by a newer build
 *  (hubchat-core, `store::NEWER_STORE`; the two must stay the same words). */
const NEWER_STORE = "this database was written by a newer Hubchat";

export interface StartFailure {
  title?: string;
  text: string;
  /** shown as an error */
  bad: boolean;
  /** the one button: it reloads the page */
  action: "Reload";
}

/** `error`: what the core's start threw. `origin`: where this page is served from. `version`: this build's. */
export function startFailure(error: unknown, origin: string, version: string): StartFailure {
  const said = typeof error === "string" ? error : error instanceof Error ? error.message : String(error);
  if (said.includes(NEWER_STORE)) {
    // The server went back to an older build (spec, Version skew). The data is left as it is:
    // the newer build opens it again once the server has it.
    return {
      title: "Update the Hubchat Web server",
      text: `This browser last used a newer Hubchat Web than the one ${origin} serves now${version ? ` (version ${version})` : ""}. `
        + "Ask whoever runs that server to update it, then reload. Your identity and messages are still here, untouched.",
      bad: false,
      action: "Reload",
    };
  }
  return { text: "Hubchat couldn't start its core: " + said, bad: true, action: "Reload" };
}
