// Audit existing desktop/Android collapse defaults through the real store.
import {createRoot} from 'react-dom/client'
import {useState} from 'react'
import {api, type TransferView} from '../src/api'
import {refreshTransfers} from '../src/lib/store'
import {TransfersChip,TransfersStrip} from '../src/components/Transfers'
import {PlatformCtx} from '../src/components/ui'
const W=window as unknown as {proof:{refresh:()=>Promise<void>;reopen:()=>void;confirmed:number;opened:string[]}}
W.proof={refresh:refreshTransfers,reopen:()=>{},confirmed:40,opened:[]}
api.transfers=async()=>({rows:[{local_id:'fixture-transfer',message_id:'fixture-message',created_at:'2026-10-14T00:00:00Z',
  peer:'fixture.peer',name:'retained.txt',upload:false,state:'running',done:W.proof.confirmed,total:100,error:null} as TransferView],next:null})
function Host(){const [generation,next]=useState(0);W.proof.reopen=()=>next(v=>v+1)
  const platform=document.body.dataset.platform==='android'?'android':'desktop'
  return <PlatformCtx.Provider value={platform}><div className={platform==='android'?'mobile':'desktop'} key={generation}>
    {platform==='android'?<TransfersStrip onOpen={p=>W.proof.opened.push(p)}/>:<TransfersChip onOpen={p=>W.proof.opened.push(p)}/>}
  </div></PlatformCtx.Provider>
}
void refreshTransfers().then(()=>createRoot(document.getElementById('root')!).render(<Host/>))
