// Mounted layouts, synthetic API only. No native opener, files or network.
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type Attachment, type Message } from "../src/api";
import { mockApi } from "../src/lib/mock";

declare global { interface Window { contactFilesResult?: {platform:string;checks:{name:string;ok:boolean}[]}; contactFilesError?:string; contactFilesProgress?:string; } }
const settle = () => new Promise<void>(r => setTimeout(r, 70));
async function run() {
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const peer="casey.abcdef",checks:{name:string;ok:boolean}[]=[],calls:string[]=[];
  const check=(name:string,ok:unknown)=>checks.push({name,ok:!!ok});
  const att=(id:string,name:string,extra:Partial<Attachment>={}):Attachment=>({local_id:id,name,bytes:1200,hub_id:id,source:null,local_path:null,state:"remote",error:null,...extra});
  const messages:Message[]=Array.from({length:65},(_,i)=>({id:"fixture-"+i,peer,outgoing:false,hub:"https://fixture.invalid",body:"Message "+i,kind:null,reply_to:null,created_at:new Date(1700000000000+i*60000).toISOString(),sent_at:null,received_at:null,state:"received",seen:true,read_at:null,fetched_at:null,delivered_at:null,error:null,attachments:[]}));
  messages[0].attachments=[att("photo","calendar.png")];
  messages[1].attachments=[att("saved","report.pdf",{state:"done",local_path:"C:/synthetic/report.pdf"})];
  messages[2].attachments=[att("remote","clip.mp4")];
  messages[3].attachments=[att("busy","archive.zip",{state:"downloading",download:{state:"running",confirmed:300,retry_at:0,error:null}})];
  messages[4].attachments=[att("failed","retry.txt",{state:"failed",error:"Synthetic failure"})];
  messages[5].attachments=[att("expired","old.txt",{state:"expired"})];
  messages[6].outgoing=true; messages[6].state="sent"; messages[6].attachments=[att("source","sent.txt",{state:"uploaded",source:"content://synthetic/sent"})];
  let listener:Parameters<typeof api.onEvent>[0]=()=>{},olderCalls=0;
  Object.assign(api,mockApi,{
    state:async()=>({...await mockApi.state(),me:{id:"scratch",address:"scratch.abcdef",name:"Scratch",about:""},hubs:[],platform,recovery_saved:true,file_background_hint:null}),
    directory:async()=>[{address:peer,name:"Casey",kind:"person",org_name:"",blurb:"",hubs:[]}],
    chats:async()=>[{peer,last:messages[64],unread:0}],
    chat:async(_peer:string,o?:{before?:Message;from?:Message;limit?:number})=>{
      if(o?.before){olderCalls++;return messages.filter(m=>m.created_at<o.before!.created_at).slice(-(o.limit??50));}
      if(o?.from)return messages.filter(m=>m.created_at>=o.from!.created_at);
      return messages.slice(-(o?.limit??50));
    },
    message:async(id:string)=>messages.find(m=>m.id===id)??null,
    transfers:async()=>({rows:[],next:null}),onEvent:async(f:typeof listener)=>{listener=f;return()=>{};},
    openDm:async()=>{},markRead:async()=>{},draft:async()=>"",setDraft:async()=>{},draftAnswers:async()=>({replies:[],picks:{}}),
    setupStatus:async()=>null,sendRoute:async()=>({pinned:null,automatic:null,next:null,hubs:[]}),
    loadOlder:async()=>({added:0,more:[],unreachable:[]}),
    openAttachment:async(m:string,a:string,reveal:boolean)=>{calls.push(`open:${m}:${a}:${reveal}`);},
    download:async(m:string,a:string)=>{calls.push(`download:${m}:${a}`);const x=messages.flatMap(m=>m.attachments).find(x=>x.local_id===a)!;x.download={state:"running",confirmed:0,retry_at:0,error:null};listener({type:"chat",peer});},
    attachmentPreview:async()=>Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg=="),c=>c.charCodeAt(0)).buffer,
  });
  const root=createRoot(document.getElementById("root")!);root.render(<App/>);await settle();await settle();
  const click=async(e:Element|null)=>{(e as HTMLElement|null)?.click();await settle();await settle();};
  await click(document.querySelector(".crow"));
  const contact=async()=>{await click(document.querySelector(platform==="desktop"?'[aria-label="Contact info"]':'.appbar .who'));};
  await contact();
  const row=(id:string)=>document.querySelector(`[data-contact-file="${id}"]`)??[...document.querySelectorAll(platform==="desktop"?'.filerow':'.li')].find(e=>e.textContent?.includes(messages.flatMap(m=>m.attachments).find(a=>a.local_id===id)!.name))??null;
  const menu=async(id:string)=>{const r=row(id);const b=r?.querySelector('[aria-label^="Actions for"]');if(b)await click(b);else {r?.dispatchEvent(new MouseEvent("contextmenu",{bubbles:true,cancelable:true,clientX:210,clientY:320}));await settle();}};
  const item=(label:string)=>[...document.querySelectorAll('[role="menuitem"]')].find(e=>e.textContent?.trim()===label)??null;
  const closeMenu=async()=>{if(document.querySelector('[role="menu"]'))document.body.dispatchEvent(new KeyboardEvent("keydown",{key:"Escape",bubbles:true,cancelable:true}));await settle();};
  check("file list retains all attachment names",["photo","saved","remote","busy","failed","expired","source"].every(id=>!!row(id)));
  check("rows expose keyboard activation",row("saved")?.querySelector('button')||row("saved")?.getAttribute("tabindex")==="0");
  check("in-progress and failed rows show transfer state",row("busy")?.textContent?.includes("Downloading")&&row("failed")?.textContent?.includes("failed"));
  await click(row("photo")?.querySelector('.contact-file-open')??row("photo"));
  check("image click opens existing viewer",document.querySelector('.imgview[aria-label="calendar.png"]'));
  await click(document.querySelector('.imgview [aria-label="Close"]'));
  // Android closes its viewer through history; wait for the pop to settle.
  await settle();
  await click(row("saved")?.querySelector('.contact-file-open')??row("saved"));
  check("saved file click uses bound native attachment opener",calls.includes("open:fixture-1:saved:false"));
  await menu("saved");
  check("saved menu exposes platform save action",platform==="desktop"?!!item("Show in folder"):!!item("Open"));
  if(platform==="desktop")await click(item("Show in folder"));else await closeMenu();
  check("folder reveal is desktop only",platform==="desktop"?calls.includes("open:fixture-1:saved:true"):!calls.some(c=>c.endsWith(":true")));
  await menu("remote");check("remote file offers Download",item("Download"));await click(item("Download"));
  check("download action queues existing transfer without opening",calls.includes("download:fixture-2:remote")&&!calls.some(c=>c.startsWith("open:fixture-2")));
  await menu("failed");check("failed file offers retry",item("Retry download"));await closeMenu();
  await menu("expired");check("expired file has no download or open action",!!item("Show in chat")&&!item("Download")&&!item("Open")&&!item("View"));await closeMenu();
  await click(row("source")?.querySelector('.contact-file-open')??row("source"));
  check("outgoing content URI uses native bound opener",calls.includes("open:fixture-6:source:false"));
  // Exercise a remote click and completion separately from the save-only action.
  messages[2].attachments[0].download=null;listener({type:"chat",peer});await settle();await settle();
  await click(row("remote")?.querySelector('.contact-file-open')??row("remote"));
  messages[2].attachments[0]={...messages[2].attachments[0],state:"done",local_path:"content://synthetic/clip",download:{state:"done",confirmed:1200,retry_at:0,error:null}};listener({type:"chat",peer});await settle();await settle();
  check("remote click opens only after download completes",calls.includes("open:fixture-2:remote:false"));
  // A file in the panel lies outside the newest50 Conversation page.
  await menu("photo");await click(item("Show in chat"));await settle();await settle();
  check("show in chat returns to conversation",!document.querySelector(platform==="desktop"?'.info':'.hero')&&!!document.querySelector('.timeline'));
  check("show in chat loads older pages and highlights exact message",olderCalls>0&&document.getElementById("m-fixture-0")?.classList.contains("flash"));
  await contact();
  // Leave the panel with another pending auto-open: it must not launch later.
  messages[4].attachments[0]={...messages[4].attachments[0],state:"remote",error:null};listener({type:"chat",peer});await settle();await settle();
  await click(row("failed")?.querySelector('.contact-file-open')??row("failed"));
  await click(document.querySelector(platform==="desktop"?'.info-head [aria-label="Close"]':'.appbar [aria-label="Back"]'));
  messages[4].attachments[0]={...messages[4].attachments[0],local_path:"content://synthetic/retry",download:{state:"done",confirmed:1200,retry_at:0,error:null}};listener({type:"chat",peer});await settle();await settle();
  check("closing panel cancels deferred file open",!calls.includes("open:fixture-4:failed:false"));
  await contact();await menu("saved");
  window.contactFilesResult={platform,checks};
}
run().catch(e=>{window.contactFilesError=String(e?.stack||e);});
