import fs from 'node:fs';
import {test} from 'node:test';
import assert from 'node:assert/strict';
const read=p=>fs.readFileSync(new URL('../'+p,import.meta.url),'utf8');
const base='src-tauri/gen/android/app/src/main/java/dev/orgtree/hubchat/';
test('worker completes the tested notification policy before removing its durable handoff',()=>{
 const source=read(base+'NotificationReply.kt');
 assert.ok(source.indexOf('finishReplyNotification(outcome,')<source.indexOf('finishReplyHandoff(outcome)'));
 assert.ok(source.includes('dismiss = { NotificationReplies.dismiss(applicationContext, target) }'));
 assert.ok(source.includes('ConnectionService.checkNotificationReply(target.owner, id, target.peer, 45)'));
 assert.ok(!source.includes('ConnectionService.checkNow('),'reply completion is independent of global hub/outbox readiness');
 const save=source.indexOf('NotificationReplies.save(context, id, payload)');
 const queued=source.indexOf('NotificationReplies.status(context, target, "Reply queued")',save);
 const enqueue=source.indexOf('NotificationReplies.enqueue(context, id)',save);
 assert.ok(save<queued&&queued<enqueue,'receiver status cannot race after worker failure');
});
test('dismissal and message posting/clearing share the token-protecting monitor',()=>{
 const reply=read(base+'NotificationReply.kt'),service=read(base+'ConnectionService.kt');
 const dismiss=reply.slice(reply.indexOf('@Synchronized fun dismiss('),reply.indexOf('  fun file('));
 assert.ok(dismiss.includes('it.id == 2 && matchesReplyNotification('));
 assert.ok(dismiss.includes('ConnectionService.clearMessage(target.peer)'));
 for(const method of ['fun notifyMessage(', 'fun clearMessage(']){
  const at=service.indexOf(method);assert.ok(at>=0);
  assert.ok(service.slice(at,at+150).includes('synchronized(NotificationReplies)'));
 }
 assert.ok(!dismiss.includes('cancelAll('),'other chats stay untouched');
});
