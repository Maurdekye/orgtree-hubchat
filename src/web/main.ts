// Hubchat Web's entry (web/index.html). The mark is set before any other
// module loads: src/api.ts reads it once (`isWeb`), and what the UI does
// differently in a browser hangs on that.
(window as unknown as { __HUBCHAT_WEB__: true }).__HUBCHAT_WEB__ = true;

void import("./start");
