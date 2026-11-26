// Where a native drag-and-drop point lands on the page. Tauri hands the
// webview's drag position over labelled "physical", but what it holds depends
// on the system (wry 0.57): on Windows (WebView2, ScreenToClient) it is in
// device pixels; on Linux (WebKitGTK, GTK widget coordinates) and macOS
// (WKWebView, AppKit points) it is already in logical pixels, which are the
// page's CSS pixels. Dividing those by devicePixelRatio (2 at GDK scale 2 or
// on a Retina screen) put the drop up and to the left of the pointer, outside
// the chat, so it was silently ignored.
export type Point = { x: number; y: number };

/** The CSS-pixel point for a native drag position `p` in a page with
 *  `devicePixelRatio` `dpr`, on the system named by the user agent `ua`. */
export function dropPoint(p: Point, dpr: number, ua: string): Point {
  if (!/Windows/.test(ua)) return { x: p.x, y: p.y };
  const k = dpr || 1;
  return { x: p.x / k, y: p.y / k };
}
