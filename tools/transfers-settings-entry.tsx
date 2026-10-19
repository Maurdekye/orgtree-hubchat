// Real layouts with synthetic state/API only. No hub, filesystem or native writes.
import { createRoot } from "react-dom/client";
import App from "../src/App";
import { api, type TransferView } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { refreshTransfers } from "../src/lib/store";

declare global { interface Window { transferSettings?: {platform:string;checks:{name:string;ok:boolean}[]}; transferSettingsError?:string; captureChats?:boolean; continueTransfers?:()=>void; } }
const pause=()=>new Promise<void>(r=>setTimeout(r,80));
async function run(){
 const platform=new URLSearchParams(location.search).get('platform')==='android'?'android':'desktop';
 const peer='casey.abcdef',checks:{name:string;ok:boolean}[]=[],calls:string[]=[];
 const check=(name:string,ok:unknown)=>checks.push({name,ok:!!ok});
 const t:TransferView={local_id:'active',message_id:'message',created_at:'2026-10-19T00:00:00Z',peer,name:'archive.zip',upload:false,state:'running',done:40,total:100,error:null};
 let running=true,historyReads=0;
 Object.assign(api,mockApi,{
  state:async()=>({...await mockApi.state(),me:{id:'scratch',address:'scratch.abcdef',name:'Scratch',about:''},hubs:[],platform,recovery_saved:true,file_background_hint:null}),
  directory:async()=>[{address:peer,name:'Casey',kind:'person',org_name:'',blurb:'',hubs:[]}],
  chats:async()=>[{peer,last:null,unread:0}],chat:async()=>[],
  transfers:async()=>({rows:running?[t]:[],next:null}),
  transferHistory:async()=>{historyReads++;return {rows:[{...t,local_id:'saved',message_id:'saved-message',state:'done',name:'report.pdf',available:true,can_redownload:true}],next:null};},
  onEvent:async()=>()=>{},openDm:async()=>{},markRead:async()=>{},draft:async()=>'',setDraft:async()=>{},draftAnswers:async()=>({replies:[],picks:{}}),
  setupStatus:async()=>null,sendRoute:async()=>({pinned:null,automatic:null,next:null,hubs:[]}),
  cancelTransfer:async(id:string)=>{calls.push('cancel:'+id);},
  openAttachment:async(m:string,a:string)=>{calls.push('open:'+m+':'+a);},
 });
 createRoot(document.getElementById('root')!).render(<App/>);await pause();await pause();
 const click=async(e:Element|null)=>{(e as HTMLElement|null)?.click();await pause();await pause();};
 const textButton=(label:string)=>[...document.querySelectorAll('button,[role="button"]')].find(e=>e.textContent?.trim()===label||e.querySelector('.t1')?.textContent===label)??null;
 const settings=()=>click(document.querySelector('[aria-label="Settings"]'));
 const entry=()=>platform==='desktop'?[...document.querySelectorAll('.set-nav button')].find(e=>e.textContent?.trim()==='Transfers')??null:textButton('Transfers');
 const panel=()=>document.querySelector('.transfers-panel');
 check('chat header has no transfers bar',!document.querySelector('.xstrip,.tb-xfer'));
 check('chat filters remain available',[...document.querySelectorAll('.fchip')].some(e=>e.firstChild?.textContent==='All'));
 check('history stays lazy on chat list',historyReads===0);
 if(platform==='desktop'){
  await click(document.querySelector('[aria-label="Shrink the chat list"]'));
  check('desktop rail has no transfers bar',!document.querySelector('.tb-xfer'));
  await click(document.querySelector('[aria-label="Show the chat list"]'));
 }else check('desktop rail has no transfers bar',!document.querySelector('.tb-xfer'));
 // The capture host snapshots the initial chat list before continuing.
 window.captureChats=true;await new Promise<void>(r=>{window.continueTransfers=r;});
 await settings();check('settings opens',!!document.querySelector(platform==='desktop'?'.settings':'.profile-card'));
 check('settings offers Transfers',!!entry());await click(entry());
 check('transfers section opens',!!panel());
 check('active transfer and progress retained',!!panel()?.querySelector('.xrow:not(.xhistory-row)')&&panel()?.textContent?.includes('archive.zip')&&panel()?.querySelector<HTMLElement>('.tb-bar b')?.style.width==='40%');
 check('finished history retained',!!panel()?.textContent?.includes('report.pdf'));
 await click(panel()?.querySelector('.xhistory-row button')??null);
 check('history View uses existing opener',calls.includes('open:saved-message:saved'));
 await click(panel()?.querySelector('.xrow:not(.xhistory-row) [aria-label="Cancel"],.xrow:not(.xhistory-row) button')??null);
 check('active Cancel uses existing API',calls.includes('cancel:active'));
 await click(panel()?.querySelector('.xrow:not(.xhistory-row)')??null);
 check('active row opens its conversation',!!document.querySelector('.timeline')&&!panel());
 if(platform==='android')await click(document.querySelector('.appbar [aria-label="Back"]'));
 await settings();await click(entry());
 running=false;await refreshTransfers();await pause();await pause();
 check('idle Transfers stays reachable with history',!!panel()&&!!panel()?.textContent?.includes('report.pdf')&&!panel()?.querySelector('.xrow:not(.xhistory-row)'));
 // Leave representative active + finished content for the requested capture.
 running=true;await refreshTransfers();await pause();await pause();
 window.transferSettings={platform,checks};
}
run().catch(e=>{window.transferSettingsError=String(e?.stack||e);});
