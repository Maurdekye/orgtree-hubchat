-- 19: metadata-only attention projection. Reads depend on active reasons,
-- not the number of already answered cards in retained history.
CREATE INDEX taskbar_unread ON messages(id) WHERE outgoing=0 AND seen=0;
CREATE INDEX taskbar_old_unread ON history_marks(peer) WHERE old_unread>0;
CREATE TABLE taskbar_questions (
 id TEXT PRIMARY KEY,
 peer TEXT NOT NULL,
 pending INTEGER NOT NULL
);
CREATE INDEX taskbar_questions_pending ON taskbar_questions(id) WHERE pending=1;

-- Same settlement as card_answers: any opposite-direction reply in this
-- chat, including a queued answer, empty skip or secondary multi-reply.
INSERT INTO taskbar_questions(id,peer,pending)
 SELECT q.id,q.peer,NOT EXISTS(
  SELECT 1 FROM messages a WHERE a.peer=q.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=q.id
               UNION SELECT message_id FROM reply_links WHERE target=q.id))
 FROM messages q WHERE q.outgoing=0 AND q.kind='question';

CREATE TRIGGER taskbar_question_insert AFTER INSERT ON messages
 WHEN NEW.outgoing=0 AND NEW.kind='question' BEGIN
 INSERT INTO taskbar_questions(id,peer,pending)
 SELECT NEW.id,NEW.peer,NOT EXISTS(
  SELECT 1 FROM messages a WHERE a.peer=NEW.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=NEW.id
               UNION SELECT message_id FROM reply_links WHERE target=NEW.id));
END;
CREATE TRIGGER taskbar_answer_insert AFTER INSERT ON messages
 WHEN NEW.outgoing=1 BEGIN
 UPDATE taskbar_questions SET pending=0 WHERE peer=NEW.peer AND id=NEW.reply_to;
END;
CREATE TRIGGER taskbar_message_delete BEFORE DELETE ON messages BEGIN
 DELETE FROM taskbar_questions WHERE id=OLD.id;
 UPDATE taskbar_questions SET pending=NOT EXISTS(
  SELECT 1 FROM messages a WHERE a.id<>OLD.id AND a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=taskbar_questions.id
               UNION SELECT message_id FROM reply_links WHERE target=taskbar_questions.id))
 WHERE OLD.outgoing=1 AND peer=OLD.peer
  AND id IN (SELECT OLD.reply_to UNION SELECT target FROM reply_links WHERE message_id=OLD.id);
END;
CREATE TRIGGER taskbar_reply_insert AFTER INSERT ON reply_links BEGIN
 UPDATE taskbar_questions SET pending=0 WHERE id=NEW.target
  AND EXISTS(SELECT 1 FROM messages a WHERE a.id=NEW.message_id AND a.outgoing=1 AND a.peer=taskbar_questions.peer);
END;
CREATE TRIGGER taskbar_reply_delete AFTER DELETE ON reply_links BEGIN
 UPDATE taskbar_questions SET pending=NOT EXISTS(
  SELECT 1 FROM messages a WHERE a.peer=taskbar_questions.peer AND a.outgoing=1
   AND a.id IN (SELECT id FROM messages WHERE reply_to=taskbar_questions.id
               UNION SELECT message_id FROM reply_links WHERE target=taskbar_questions.id))
 WHERE id=OLD.target;
END;
