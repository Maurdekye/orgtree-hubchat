import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { installRecent } from "../src/trayrecent-view";
import { recentFixture } from "./tray-recent-fixture";

function setup(view = recentFixture(), picture?: () => Promise<unknown>) {
  const dom = new JSDOM("<!doctype html><body></body>", {pretendToBeVisual:true});
  const calls: {command:string;args:Record<string,unknown>}[]=[];
  const doc=dom.window.document;
  const menu=installRecent(doc.body,async(command,args)=>{calls.push({command,args}); if(command.endsWith("picture")) return picture?.();});
  menu.update(view);
  const key=(key:string)=>doc.dispatchEvent(new dom.window.KeyboardEvent("keydown",{key,bubbles:true,cancelable:true}));
  return {dom,doc,calls,menu,key,close:()=>{menu.dispose();dom.window.close();}};
}

test("six recent rows retain all chat kinds, names and avatars without an Open Hubchat row",()=>{
  const t=setup();try{
    assert.equal(t.doc.querySelectorAll(".recent-row").length,6);
    assert.equal(t.doc.querySelectorAll(".recent-avatar").length,6);
    assert.equal(t.doc.querySelectorAll(".k-org").length,2);
    assert.equal(t.doc.querySelectorAll(".k-chat").length,1);
    assert.ok(!t.doc.body.textContent!.includes("Open Hubchat"));
    assert.equal(t.doc.querySelector(".recent-name")!.textContent,"Maya");
  }finally{t.close();}
});
test("typing replaces the unread badge slot and zero unread has no badge",()=>{
  const t=setup();try{
    assert.equal(t.doc.querySelectorAll(".recent-badge").length,2);
    assert.equal(t.doc.querySelectorAll(".recent-spin").length,1);
    const typing=t.doc.querySelector('[data-peer="org.222222"]')!;
    assert.equal(typing.querySelectorAll(".recent-badge").length,0);
    assert.ok(typing.getAttribute("aria-label")!.includes("typing"));
    assert.deepEqual([...t.doc.querySelectorAll(".recent-badge")].map(e=>e.textContent),["2","99+"]);
    const next=recentFixture();next.revision=2;next.recent[1].typing=false;t.menu.update(next);
    assert.equal(t.doc.querySelectorAll(".recent-spin").length,0);
    assert.equal(t.doc.querySelector('[data-peer="org.222222"] .recent-badge')!.textContent,"3");
  }finally{t.close();}
});
test("keyboard selection opens the requested chat exactly once through the shared native action",()=>{
  const t=setup();try{
    t.key("ArrowDown");t.key("ArrowDown");t.key("Enter");t.key("Enter");
    const a=t.calls.filter(c=>c.command.endsWith("action"));assert.equal(a.length,1);
    assert.deepEqual(a[0].args,{generation:1,id:"org.222222",checked:null});
  }finally{t.close();}
});
test("Escape and an outside click dismiss an empty list without navigation",()=>{
  for(const outside of [false,true]){
    const v=recentFixture();v.recent=[];const t=setup(v);try{
      assert.equal(t.doc.querySelector(".recent-empty")!.textContent,"No recent chats.");
      if(outside)t.doc.body.dispatchEvent(new t.dom.window.Event("pointerdown",{bubbles:true}));else t.key("Escape");
      assert.equal(t.calls.filter(c=>c.command.endsWith("close")).length,1);
      assert.equal(t.calls.filter(c=>c.command.endsWith("action")).length,0);
    }finally{t.close();}
  }
});
test("unavailable state does not claim an empty chat history and names remain plain text",()=>{
  const v=recentFixture();v.recent=[];v.recentAvailable=false;const t=setup(v);try{
    assert.equal(t.doc.querySelector(".recent-empty")!.textContent,"Recent chats unavailable.");
    const next=recentFixture();next.revision=2;next.recent[0].name='<img src=x onerror="bad()">';t.menu.update(next);
    assert.equal(t.doc.querySelectorAll("img,script").length,0);
    assert.equal(t.doc.querySelector(".recent-name")!.textContent,next.recent[0].name);
  }finally{t.close();}
});
test("late old-open snapshots cannot restore the old chat list",()=>{
  const t=setup();try{
    const next=recentFixture();next.generation=2;next.recent=[next.recent[4]];t.menu.update(next);t.menu.update(recentFixture());
    assert.equal(t.doc.querySelectorAll("button").length,1);
    t.key("End");t.key("Enter");assert.equal(t.calls.at(-1)!.args.generation,2);
  }finally{t.close();}
});
test("late picture responses are discarded after a popup or metadata change",async()=>{
  let resolve!:(value:unknown)=>void;
  const v=recentFixture();v.recent[0].pictureKey="original";
  const t=setup(v,()=>new Promise(r=>{resolve=r;}));try{
    const next=recentFixture();next.generation=2;t.menu.update(next);
    resolve("iVBORw0KGgo=");await Promise.resolve();await Promise.resolve();
    assert.equal(t.doc.querySelectorAll("img").length,0);
    assert.equal(t.calls.filter(c=>c.command.endsWith("picture")).length,1);
  }finally{t.close();}
});
