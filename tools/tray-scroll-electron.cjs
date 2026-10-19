const {app,BrowserWindow}=require('electron');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const out=process.env.HUBCHAT_TRAY_SCROLL_OUT;
app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.whenReady().then(async()=>{
  const w=new BrowserWindow({show:false,width:260,height:380,webPreferences:{backgroundThrottling:false}});
  w.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
  const results=[];
  const paint=()=>w.webContents.executeJavaScript('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(()=>setTimeout(r,120))))');
  // scrollIntoView may use the padding while still exposing the entire row.
  // Visibility ends at the border, not at the padded initial content box.
  const measure=()=>w.webContents.executeJavaScript(`(()=>{const p=document.querySelector('.recent-pane'),box=p.getBoundingClientRect();return {rows:p.querySelectorAll('button').length,viewport:p.clientHeight-10,scrollHeight:p.scrollHeight,scrollTop:p.scrollTop,width:p.clientWidth,box:box.toJSON(),overflow:getComputedStyle(p).overflowY,focused:document.activeElement.dataset.peer,visible:[...p.querySelectorAll('button')].filter(b=>{const r=b.getBoundingClientRect();return r.top>=box.top+1-.1&&r.bottom<=box.bottom-1+.1}).map(b=>b.dataset.peer)}})()`);
  const key=async key=>{w.webContents.sendInputEvent({type:'keyDown',keyCode:key});w.webContents.sendInputEvent({type:'keyUp',keyCode:key});await paint();};
  try{
    await w.loadFile(path.join(out,'index.html'));await paint();
    let m=await measure();results.push({state:'open',...m});
    assert.equal(m.rows,16);assert.equal(m.viewport,280);assert.equal(m.visible.length,10);assert.equal(m.visible[0],'peer0');assert.equal(m.scrollTop,0);assert.equal(m.overflow,'auto');
    assert.equal(await w.webContents.executeJavaScript('getComputedStyle(document.documentElement).colorScheme'),'dark');
    await w.webContents.capturePage().then(i=>fs.writeFileSync(out+'/ten-visible.png',i.toPNG()));
    const badges=await w.webContents.executeJavaScript(`Array.from(document.querySelectorAll('.recent-badge')).map(b=>{const r=b.getBoundingClientRect(),s=b.parentElement.getBoundingClientRect(),row=b.closest('button').getBoundingClientRect();return {text:b.textContent,left:r.left,right:r.right,slotLeft:s.left,slotRight:s.right,rowLeft:row.left,rowRight:row.right}})`);
    results.push({state:'badge-containment',badges});
    assert.ok(badges.some(b=>b.text==='99+'));
    for(const b of badges)assert.ok(b.left>=b.slotLeft-.1&&b.right<=b.slotRight+.1&&b.left>=b.rowLeft&&b.right<=b.rowRight,'unread badge must fit its reserved column and row');
    // Real wheel input, followed by an input barrier rather than a fabricated scroll.
    w.webContents.sendInputEvent({type:'mouseWheel',x:110,y:160,deltaY:-160,deltaX:0,canScroll:true});
    await paint();m=await measure();results.push({state:'wheel',...m});assert.ok(m.scrollTop>0);
    await key('End');m=await measure();results.push({state:'end',...m});assert.equal(m.focused,'peer15');assert.ok(m.visible.includes('peer15'));
    await w.webContents.capturePage().then(i=>fs.writeFileSync(out+'/older-chats.png',i.toPNG()));
    const before=m.scrollTop;
    await w.webContents.executeJavaScript('{const v=fixture();v.revision=2;popup.update(v)}');await paint();assert.equal((await measure()).scrollTop,before);
    await w.webContents.executeJavaScript('{const v=fixture();v.generation=2;popup.update(v)}');await paint();assert.equal((await measure()).scrollTop,0);
    await w.webContents.capturePage().then(i=>fs.writeFileSync(out+'/reopened-at-top.png',i.toPNG()));
    await w.webContents.executeJavaScript('{const v=fixture();v.generation=3;v.maxHeight=150;popup.update(v)}');await paint();await key('End');m=await measure();results.push({state:'short-work-area',...m});assert.ok(m.box.bottom<=150);assert.ok(m.visible.includes('peer15'));
    await key('Home');m=await measure();assert.equal(m.focused,'peer0');assert.ok(m.visible.includes('peer0'));
    await key('End');await key('Enter');
    const actions=await w.webContents.executeJavaScript('calls.filter(c=>c.command.endsWith("action"))');assert.equal(actions.length,1);assert.equal(actions[0].args.id,'peer15');
    fs.writeFileSync(out+'/result.json',JSON.stringify({results,actions,pass:true},null,2));console.log('TRAY_SCROLL_VISUAL_PASS');app.exit(0);
  }catch(e){fs.writeFileSync(out+'/result.json',JSON.stringify({results,error:e.stack,pass:false},null,2));console.error(e.stack);app.exit(2);}
});
