// Hubchat Web's build (`npm run build:web`, tools/web-build.mjs): the UI the
// apps use, started from web/index.html over the core as WebAssembly. It is
// a config of its own so that nothing of the web build can reach the apps'
// bundle (vite.config.ts), and the apps' entry never imports src/web.
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-ignore node's types are there or not, by the install
import process from "node:process";
// @ts-ignore node's types are there or not, by the install
import { fileURLToPath } from "node:url";

const here = (p: string): string => fileURLToPath(new URL(p, import.meta.url)).replace(/\\/g, "/");
// wasm-bindgen's output for crates/hubchat-web: hubchat_web.js and hubchat_web_bg.wasm
const core: string = (process.env.HUBCHAT_WEB_CORE || here("./web/core")).replace(/\\/g, "/");
const out: string = process.env.HUBCHAT_WEB_OUT || here("./dist-web");

export default defineConfig(() => ({
  root: here("./web"),
  // the web app's own files (manifest, icons); the apps' public/ stays theirs
  publicDir: here("./web/public"),
  plugins: [react()],
  resolve: {
    alias: [
      { find: /^@hubchat-core\/wasm/, replacement: core + "/hubchat_web_bg.wasm" },
      { find: /^@hubchat-core$/, replacement: core + "/hubchat_web.js" },
    ],
  },
  // the core's worker is a module: it imports the bindings and the wasm file
  worker: { format: "es" as const },
  // KaTeX's fonts stay files: the CSP allows fonts from 'self' only, never data: URLs
  build: { outDir: out, emptyOutDir: true, target: "es2022", assetsInlineLimit: (file: string) => (/\.(woff2?|ttf)$/.test(file) ? false : undefined) },
  clearScreen: false,
  server: { port: 1430, strictPort: true, fs: { allow: [here(".")] } },
}));
