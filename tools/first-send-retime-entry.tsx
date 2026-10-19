import { createRoot } from "react-dom/client";
import { api, type Message } from "../src/api";
import { Conversation } from "../src/components/Conversation";
import { PlatformCtx } from "../src/components/ui";
import * as store from "../src/lib/store";

declare global {
  interface Window {
    firstSendFixture: { admitted: Message[]; persisted: Message[]; visible: Message[]; reread: Message[] };
    firstSendResult?: {platform: string; checks: {name: string; ok: boolean}[]};
    firstSendError?: string;
    firstSendProgress?: string;
  }
}
const settle = () => new Promise<void>(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 30))));
async function run() {
  window.firstSendProgress="starting";
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const fixture = window.firstSendFixture;
  const peer = "pat.abcdef";
  let accepted = false, mode: "visible" | "admitted" | "reread" = "visible", sends = 0;
  let listener: Parameters<typeof api.onEvent>[0] = () => {};
  Object.assign(api, {
    state: async () => ({me:{address:"scratch.123456"},hubs:[],platform}),
    chats: async () => [], directory: async () => [], transfers: async () => ({rows:[],next:null}),
    onEvent: async (f: typeof listener) => {listener=f;return () => {};},
    draft: async () => "", setDraft: async () => {}, composing: async () => {},
    setupStatus: async () => null, sendRoute: async () => ({pinned:null,automatic:null,next:null,hubs:[]}),
    loadOlder: async () => ({added:0,more:[],unreachable:[]}),
    chat: async () => accepted ? fixture[mode] : [],
    send: async () => {sends++;accepted=true;},
  });
  await store.startStore();
  window.firstSendProgress="store ready";
  const root=createRoot(document.getElementById("root")!);
  root.render(<PlatformCtx.Provider value={platform}><Conversation peer={peer} onBack={()=>{}} onInfo={()=>{}} onOpenAddr={()=>{}} /></PlatformCtx.Provider>);
  window.firstSendProgress="render queued";
  await settle();await settle();
  window.firstSendProgress="render settled";
  const checks: {name:string;ok:boolean}[]=[];
  const check=(name:string,ok:unknown)=>checks.push({name,ok:!!ok});
  const bubble=()=>document.getElementById("m-sync-first");
  check("empty conversation shows contact details", document.querySelector(".chat-start"));
  const ta=document.querySelector("textarea")!;
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,"value")!.set!.call(ta,"first outgoing");
  ta.dispatchEvent(new Event("input",{bubbles:true}));await settle();
  document.querySelector<HTMLButtonElement>(".sendbtn")!.click();await settle();await settle();
  window.firstSendProgress="send settled";
  check("exactly one accepted send", sends===1);
  check("composer clears after acceptance",ta.value==="");
  check("first synced bubble is visible without inbound",bubble()?.textContent?.includes("first outgoing"));
  check("new-chat details disappear after first send",!document.querySelector(".chat-start"));
  check("actual sent state stays truthful",bubble()?.querySelector(".tick")?.getAttribute("aria-label")?.includes("Sent"));
  mode="admitted";listener({type:"chat",peer});await settle();await settle();
  check("already loaded admission is visible before retiming",bubble()?.textContent?.includes("first outgoing"));
  mode="reread";listener({type:"chat",peer});await settle();await settle();
  window.firstSendProgress="reread settled";
  check("loaded UUID anchor survives retimed range reload",bubble()?.textContent?.includes("first outgoing"));
  check("new-chat details stay absent after retimed reload",!document.querySelector(".chat-start"));
  window.firstSendResult={platform,checks};
}
run().catch(e=>{window.firstSendError=String(e?.stack||e);});
