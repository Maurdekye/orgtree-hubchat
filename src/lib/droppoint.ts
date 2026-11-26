// Where a native drag-and-drop point lands on the page. Tauri hands the
// webview's drag position over as a "physical" position, and the page works
// in CSS pixels.
export type Point = { x: number; y: number };

/** The CSS-pixel point for a native drag position `p` in a page with
 *  `devicePixelRatio` `dpr`, on the system named by the user agent `ua`. */
export function dropPoint(p: Point, dpr: number, ua: string): Point {
  void ua;
  const k = dpr || 1;
  return { x: p.x / k, y: p.y / k };
}
