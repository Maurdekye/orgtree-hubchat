//! Upgrade old queued messages before any further network upload. Old partial
//! sessions cannot prove they contain the same bytes as today's local source.
use super::*;

impl Engine {
    pub(super) async fn snapshot_legacy(&self, message: Message, hub: &str) -> Result<Message> {
        let old: Vec<_> = message
            .attachments
            .iter()
            .filter(|a| {
                a.staged_path.is_none()
                    && !(a.state == "uploaded"
                        && a.hub.as_deref() == Some(hub)
                        && a.hub_id.is_some())
            })
            .cloned()
            .collect();
        if old.is_empty() {
            return Ok(message);
        }
        let candidate = NewOutgoing {
            id: message.id.clone(),
            peer: message.peer.clone(),
            body: String::new(),
            kind: None,
            reply_to: None,
            attachments: old
                .iter()
                .map(|a| crate::store::NewAttachment {
                    name: a.name.clone(),
                    bytes: a.bytes,
                    source: a.source.clone().unwrap_or_default(),
                })
                .collect(),
        };
        let host = self.host.clone();
        let prepared =
            tokio::task::spawn_blocking(move || crate::staging::prepare(host.as_ref(), candidate))
                .await
                .map_err(|_| Error::Invalid("File preparation stopped; retry the message.".into()))
                .and_then(|result| result.map_err(crate::staging::explain_error));
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                self.store
                    .fail_legacy_snapshot(&message.id, &old, &e.to_string())?;
                self.host.event(Event::Chat {
                    peer: message.peer.clone(),
                });
                return Err(e);
            }
        };
        // Cancellation/deletion while copying wins. Failed admission drops
        // only this preparation's private directory; original paths stay intact.
        if !self.store.install_legacy_snapshot(
            &message.id,
            &old,
            &prepared.paths,
            &prepared.digests,
        )? {
            return Err(Error::Cancelled);
        }
        prepared.commit();
        self.host.event(Event::Chat { peer: message.peer });
        self.store.message(&message.id)?.ok_or(Error::Cancelled)
    }
}
