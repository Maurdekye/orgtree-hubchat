// Match Orgtree's full view reload, retaining the current unsent/view state
// synchronously first. This is per-window session storage, scoped by identity.
import {useEffect,useLayoutEffect,useRef,useState} from "react";
import {toast} from "./toast";
import {errText} from "./native";
import {hasPreparingMessages} from "./store";

const KEY="hubchat.refresh-state";
const readers=new Map<string,()=>unknown>();
export const refreshKey=(kind:string,identity:string,peer?:string)=>JSON.stringify([kind,identity,peer]);
export function useRestoredRefreshState<T>(key:string):T|undefined {
  // Rendering (including StrictMode's repeated initializer) must not consume
  // the saved value. Remove it only after this mount commits.
  const [value]=useState<T|undefined>(()=>{
    try {return JSON.parse(sessionStorage.getItem(KEY)||"{}")[key];}catch{return undefined;}
  });
  useEffect(()=>{
  try {
    const values=JSON.parse(sessionStorage.getItem(KEY)||"{}");
    delete values[key];
    if(Object.keys(values).length)sessionStorage.setItem(KEY,JSON.stringify(values));else sessionStorage.removeItem(KEY);
  }catch{/* A failed read leaves ordinary core-backed restoration available. */}
  },[key]);
  return value;
}
export function useRefreshState(key:string,read:()=>unknown):void {
  const current=useRef(read);current.current=read;
  useLayoutEffect(()=>{const reader=()=>current.current();readers.set(key,reader);
    return ()=>{if(readers.get(key)===reader)readers.delete(key);};},[key]);
}
let refreshing=false;
export function refreshView():void {
  if(refreshing||document.querySelector('[aria-modal="true"],.scrim,.sheet'))return;
  try {
    if(hasPreparingMessages())throw Error("Wait for messages to finish preparing before refreshing.");
    const values=Object.fromEntries([...readers].map(([key,read])=>[key,read()]));
    sessionStorage.setItem(KEY,JSON.stringify(values));
    refreshing=true;
    window.location.reload();
  }catch(e){refreshing=false;toast("Could not refresh: "+errText(e));}
  // A real reload destroys this document. The reset also keeps a refused or
  // intercepted reload from leaving the button permanently inert.
  setTimeout(()=>{refreshing=false;},0);
}
