import test from 'node:test'
import assert from 'node:assert/strict'
import { dropPoint } from '../src/lib/droppoint.ts'

// User agents of the three desktop webviews (WebView2, WebKitGTK, WKWebView).
const WINDOWS = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0'
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15'
const MACOS = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)'

test('Windows: the drag position is in device pixels, so it is divided by devicePixelRatio', () => {
  assert.deepEqual(dropPoint({ x: 300, y: 150 }, 1.5, WINDOWS), { x: 200, y: 100 })
  assert.deepEqual(dropPoint({ x: 300, y: 150 }, 1, WINDOWS), { x: 300, y: 150 })
})

// wry's WebKitGTK handler passes GTK's widget coordinates, which are logical
// pixels; WebKitGTK's devicePixelRatio is the GDK scale (2 on a HiDPI or
// fractionally scaled screen). Halving them put the drop in the sidebar.
test('Linux: a drop at GDK scale 2 lands where the pointer is (the drag position is already in CSS pixels)', () => {
  assert.deepEqual(dropPoint({ x: 900, y: 400 }, 2, LINUX), { x: 900, y: 400 })
  assert.deepEqual(dropPoint({ x: 900, y: 400 }, 1, LINUX), { x: 900, y: 400 })
})

// wry's WKWebView handler passes draggingLocation, in AppKit points.
test('macOS: a drop on a Retina screen lands where the pointer is (the drag position is in points)', () => {
  assert.deepEqual(dropPoint({ x: 900, y: 400 }, 2, MACOS), { x: 900, y: 400 })
})
