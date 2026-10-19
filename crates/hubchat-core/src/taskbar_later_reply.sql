-- 20: user 2026-10-19: any later outgoing message in the same chat
-- deals with earlier incoming questions for attention only. Explicit
-- answers/skips still settle regardless of chronology; card state is unchanged.
CREATE INDEX taskbar_later_outgoing ON messages(peer,created_at) WHERE outgoing=1;
CREATE INDEX taskbar_questions_peer ON taskbar_questions(peer);

DROP TRIGGER taskbar_question_insert;
DROP TRIGGER taskbar_answer_insert;
DROP TRIGGER taskbar_message_delete;
DROP TRIGGER taskbar_reply_insert;
DROP TRIGGER taskbar_reply_delete;

UPDATE taskbar_questions SET pending=NOT (
 EXISTS(SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
  AND a.created_at>(SELECT created_at FROM messages WHERE id=taskbar_questions.id))
 OR EXISTS(SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
  AND a.id IN (SELECT id FROM messages WHERE reply_to=taskbar_questions.id
              UNION SELECT message_id FROM reply_links WHERE target=taskbar_questions.id)));

CREATE TRIGGER taskbar_question_insert AFTER INSERT ON messages
 WHEN NEW.outgoing=0 AND NEW.kind='question' BEGIN
 INSERT INTO taskbar_questions(id,peer,pending)
 SELECT NEW.id,NEW.peer,NOT (
  EXISTS(SELECT 1 FROM messages a WHERE a.peer=NEW.peer AND a.outgoing=1 AND a.created_at>NEW.created_at)
  OR EXISTS(SELECT 1 FROM messages a WHERE a.peer=NEW.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=NEW.id
               UNION SELECT message_id FROM reply_links WHERE target=NEW.id)));
END;
CREATE TRIGGER taskbar_answer_insert AFTER INSERT ON messages
 WHEN NEW.outgoing=1 BEGIN
 UPDATE taskbar_questions SET pending=0 WHERE pending=1 AND peer=NEW.peer
  AND (id=NEW.reply_to OR EXISTS(SELECT 1 FROM messages q WHERE q.id=taskbar_questions.id AND q.created_at<NEW.created_at));
END;
CREATE TRIGGER taskbar_message_delete BEFORE DELETE ON messages BEGIN
 DELETE FROM taskbar_questions WHERE id=OLD.id;
 UPDATE taskbar_questions SET pending=NOT (
  EXISTS(SELECT 1 FROM messages a WHERE a.id<>OLD.id AND a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.created_at>(SELECT created_at FROM messages WHERE id=taskbar_questions.id))
  OR EXISTS(SELECT 1 FROM messages a WHERE a.id<>OLD.id AND a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=taskbar_questions.id
               UNION SELECT message_id FROM reply_links WHERE target=taskbar_questions.id)))
 WHERE OLD.outgoing=1 AND peer=OLD.peer;
END;
CREATE TRIGGER taskbar_reply_insert AFTER INSERT ON reply_links BEGIN
 UPDATE taskbar_questions SET pending=0 WHERE id=NEW.target
  AND EXISTS(SELECT 1 FROM messages a WHERE a.id=NEW.message_id AND a.outgoing=1 AND a.peer=taskbar_questions.peer);
END;
CREATE TRIGGER taskbar_reply_delete AFTER DELETE ON reply_links BEGIN
 UPDATE taskbar_questions SET pending=NOT (
  EXISTS(SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.created_at>(SELECT created_at FROM messages WHERE id=taskbar_questions.id))
  OR EXISTS(SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=taskbar_questions.id
               UNION SELECT message_id FROM reply_links WHERE target=taskbar_questions.id)))
 WHERE id=OLD.target;
END;
-- Sync can move created_at earlier and receipt normalization can retime a
-- question or its outgoing messages. Keep the projection in that transaction.
CREATE TRIGGER taskbar_message_retime AFTER UPDATE OF created_at ON messages
 WHEN NEW.created_at<>OLD.created_at BEGIN
 UPDATE taskbar_questions SET pending=NOT (
  EXISTS(SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.created_at>(SELECT created_at FROM messages WHERE id=taskbar_questions.id))
  OR EXISTS(SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=taskbar_questions.id
               UNION SELECT message_id FROM reply_links WHERE target=taskbar_questions.id)))
 WHERE id=NEW.id OR (NEW.outgoing=1 AND peer=NEW.peer);
END;
