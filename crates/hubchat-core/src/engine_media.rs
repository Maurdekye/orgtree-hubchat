//! Inline video and audio (Hubchat 1.2.7, media.rs): a player's byte ranges of
//! an attachment, from this device's copy or the hub, nothing saved.
use super::*;
use crate::media::{media_type, span, MediaAnswer, Ranged, Span, MEDIA_CHUNK_BYTES, MEDIA_MAX_BYTES};
use std::io::{Read, Seek, SeekFrom};

impl Engine {
    /// A player's `range` of an attachment that plays inline (`media_type`, at
    /// most MEDIA_MAX_BYTES), a bounded piece at a time. The sources are an image
    /// preview's, in its order, without its saved copy: the original this device
    /// sent, then a finished download, then the hub. Nothing is saved and the
    /// attachment's download is left alone; a hub that no longer has the file
    /// marks it expired, as for a preview.
    pub async fn media_range(&self, message_id: &str, local_id: &str, range: Option<&str>) -> Result<MediaAnswer> {
        let m = self
            .store
            .message(message_id)?
            .ok_or_else(|| Error::Invalid("no such message".into()))?;
        let a = m
            .attachments
            .iter()
            .find(|a| a.local_id == local_id)
            .ok_or_else(|| Error::Invalid("no such attachment".into()))?;
        let content_type = media_type(&a.name).ok_or_else(|| Error::Invalid("not played inline".into()))?;
        if a.bytes > MEDIA_MAX_BYTES {
            return Err(Error::Invalid("too big to play inline".into()));
        }
        let sent = if m.outgoing { a.source.clone() } else { None };
        for source in sent.into_iter().chain(a.local_path.clone()) {
            let host = self.host.clone();
            let range = range.map(str::to_owned);
            let read = tokio::task::spawn_blocking(move || read_local(&*host, &source, range.as_deref(), content_type)).await;
            if let Ok(Some(answer)) = read {
                return Ok(answer);
            }
        }
        if a.state == "expired" {
            return Err(Error::Hub { status: 410, detail: "no longer on the hub".into() });
        }
        let (Some(hub_id), Some(hub)) = (a.hub_id.clone(), a.hub.clone().or_else(|| m.hub.clone())) else {
            return Err(Error::Invalid("not on a hub".into()));
        };
        let (client, stop) = self
            .hubs
            .lock()
            .unwrap()
            .get(&hub)
            .map(|h| (h.client.clone(), h.stop.clone()))
            .ok_or_else(|| Error::Invalid(format!("hub {hub} was removed")))?;
        let Some(want) = span(range, a.bytes, MEDIA_CHUNK_BYTES) else {
            return Ok(MediaAnswer::Unsatisfiable { total: Some(a.bytes) });
        };
        let mut suspended = self.suspended.subscribe();
        if *suspended.borrow_and_update() || self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let got = tokio::select! {
            biased;
            _ = suspended.changed() => Err(Error::Cancelled),
            result = stop.run(client.get_range(&self.me, &hub_id, want, a.bytes)) => result,
        };
        match got {
            Ok(Ranged::Bytes { span, total, body }) => Ok(MediaAnswer::Part { content_type, span, total, body }),
            Ok(Ranged::Unsatisfiable { total }) => Ok(MediaAnswer::Unsatisfiable { total }),
            Err(e) => {
                if let Error::Hub { status: 410, .. } = &e {
                    self.store
                        .set_attachment(local_id, "expired", None, None, Some(&e.to_string()))?;
                    self.host.event(Event::Chat { peer: m.peer.clone() });
                }
                Err(e)
            }
        }
    }
}

/// `range` of this device's copy of a playable file, at most a piece of it, or
/// a 416 past its end. None when it can't be read here (gone, moved, a stream
/// that can't seek): the next source is tried.
fn read_local(host: &dyn Host, source: &str, range: Option<&str>, content_type: &'static str) -> Option<MediaAnswer> {
    let mut f = host.open_source(source).ok()?;
    let total = f.metadata().ok()?.len();
    if total == 0 || total > MEDIA_MAX_BYTES {
        return None;
    }
    let Some(want) = span(range, total, MEDIA_CHUNK_BYTES) else {
        return Some(MediaAnswer::Unsatisfiable { total: Some(total) });
    };
    f.seek(SeekFrom::Start(want.start)).ok()?;
    let mut body = Vec::with_capacity(want.size() as usize);
    f.take(want.size()).read_to_end(&mut body).ok()?;
    if body.is_empty() {
        return None;
    }
    let end = want.start + body.len() as u64 - 1;
    Some(MediaAnswer::Part { content_type, span: Span { start: want.start, end }, total, body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NewAttachment, NewOutgoing};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const MIB: u64 = 1024 * 1024;
    /// Nothing listens here: a hub that is asked fails the test.
    const NOWHERE: &str = "http://127.0.0.1:9";

    struct LocalHost(PathBuf);
    impl Host for LocalHost {
        fn event(&self, _: Event) {}
        fn open_source(&self, p: &str) -> std::io::Result<std::fs::File> {
            std::fs::File::open(p)
        }
        fn download_dir(&self) -> PathBuf {
            self.0.clone()
        }
    }

    fn fixture(store: Arc<Store>, downloads: &std::path::Path, url: &str) -> Arc<Engine> {
        let e = Engine::new(store, Identity::generate("media-proof").unwrap(), Profile::default(), Arc::new(LocalHost(downloads.into())));
        e.hubs.lock().unwrap().insert(
            url.into(),
            HubRuntime {
                client: HubClient::new(HubAddress::parse(url).unwrap()),
                status: HubStatus {
                    url: url.into(),
                    name: "scratch".into(),
                    state: HubState::Connected,
                    error: None,
                    retry_at_ms: None,
                    max_attachment_bytes: 1073741824,
                    features: vec!["uploads".into()],
                    version: None,
                    answered_ms: None,
                    waiting_since_ms: None,
                    clock_offset_ms: None,
                },
                retry_now: Arc::new(Notify::new()),
                kick: Arc::new(Notify::new()),
                stop: CancelFlag::default(),
            },
        );
        e
    }

    /// A received message `id` with one attachment on the hub at `url`.
    fn incoming(store: &Store, url: &str, id: &str, name: &str, bytes: u64) -> String {
        store.add_hub(url, &now()).unwrap();
        let env = serde_json::from_value(serde_json::json!({
            "id": id, "from": "remote.111111", "to": "local.222222", "body": "",
            "received_at": "2026-10-30T00:00:00Z",
            "attachments": [{ "id": "remote-file", "name": name, "bytes": bytes }]
        }))
        .unwrap();
        store.insert_incoming(url, &env, &now()).unwrap();
        store.message(id).unwrap().unwrap().attachments[0].local_id.clone()
    }

    /// A message this device sent, its file still at `source`.
    fn outgoing(store: &Store, source: &str, name: &str, bytes: u64) -> String {
        let attachments = vec![NewAttachment { name: name.into(), bytes, source: source.into() }];
        let msg = NewOutgoing { id: "message".into(), peer: "other.123456".into(), body: String::new(), kind: None, reply_to: None, attachments };
        store.queue_outgoing(&msg, &now()).unwrap();
        store.message("message").unwrap().unwrap().attachments[0].local_id.clone()
    }

    async fn request(s: &mut tokio::net::TcpStream) -> String {
        let mut out = Vec::new();
        while !out.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            s.read_exact(&mut b).await.unwrap();
            out.push(b[0]);
            assert!(out.len() < 16000);
        }
        String::from_utf8(out).unwrap().to_lowercase()
    }

    #[tokio::test]
    async fn a_received_video_plays_from_the_hub_a_piece_at_a_time_and_saves_nothing() {
        let downloads = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let lid = incoming(&store, &url, "message", "clip.mp4", 3 * MIB);
        let e = fixture(store.clone(), downloads.path(), &url);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let head = request(&mut socket).await;
            assert!(head.starts_with("get /api/attachments/remote-file "), "{head}");
            assert!(head.contains("range: bytes=0-1048575"), "{head}");
            let answer = format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{}\r\nContent-Length: {MIB}\r\nConnection: close\r\n\r\n", MIB - 1, 3 * MIB);
            socket.write_all(answer.as_bytes()).await.unwrap();
            socket.write_all(&vec![7u8; MIB as usize]).await.unwrap();
        });
        // the player's first ask: the start of the file, one piece
        match e.media_range("message", &lid, Some("bytes=0-")).await.unwrap() {
            MediaAnswer::Part { content_type, span, total, body } => assert_eq!(
                (content_type, span, total, body.len() as u64),
                ("video/mp4", Span { start: 0, end: MIB - 1 }, 3 * MIB, MIB)
            ),
            other => panic!("{other:?}"),
        }
        server.await.unwrap();
        // nothing saved, and the attachment is as it was: not downloaded
        let a = store.message("message").unwrap().unwrap().attachments.remove(0);
        assert_eq!((a.state.as_str(), a.local_path.as_deref()), ("remote", None));
        assert!(store.download_job(&lid).unwrap().is_none());
        assert_eq!(std::fs::read_dir(downloads.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn the_original_this_device_sent_plays_from_disk_without_the_hub() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let file = dir.path().join("sent.webm");
        std::fs::write(&file, b"0123456789").unwrap();
        let lid = outgoing(&store, &file.to_string_lossy(), "sent.webm", 10);
        let e = fixture(store, dir.path(), NOWHERE);
        let got = e.media_range("message", &lid, Some("bytes=2-5")).await.unwrap();
        assert_eq!(got, MediaAnswer::Part { content_type: "video/webm", span: Span { start: 2, end: 5 }, total: 10, body: b"2345".to_vec() });
        // past its end: nothing there, and its size
        assert_eq!(e.media_range("message", &lid, Some("bytes=10-")).await.unwrap(), MediaAnswer::Unsatisfiable { total: Some(10) });
    }

    #[tokio::test]
    async fn a_finished_download_plays_from_where_it_was_saved() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let lid = incoming(&store, NOWHERE, "message", "Song.MP3", 6);
        let saved = dir.path().join("Song.MP3");
        std::fs::write(&saved, b"abcdef").unwrap();
        store.set_attachment(&lid, "done", None, Some(&saved.to_string_lossy()), None).unwrap();
        let e = fixture(store, dir.path(), NOWHERE);
        let got = e.media_range("message", &lid, None).await.unwrap();
        assert_eq!(got, MediaAnswer::Part { content_type: "audio/mpeg", span: Span { start: 0, end: 5 }, total: 6, body: b"abcdef".to_vec() });
    }

    #[tokio::test]
    async fn only_playable_files_within_the_ceiling_are_served() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let text = incoming(&store, NOWHERE, "text", "notes.txt", 10);
        let long = incoming(&store, NOWHERE, "long", "lecture.mp4", MEDIA_MAX_BYTES + 1);
        let e = fixture(store, dir.path(), NOWHERE);
        for (id, lid) in [("text", text.as_str()), ("long", long.as_str()), ("long", "no-such-attachment"), ("no-such-message", "x")] {
            let got = e.media_range(id, lid, None).await;
            assert!(matches!(got, Err(Error::Invalid(_))), "{id}: {got:?}");
        }
    }

    #[tokio::test]
    async fn a_file_gone_from_the_hub_is_marked_expired_as_for_a_preview() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let lid = incoming(&store, &url, "message", "clip.webm", 100);
        let e = fixture(store.clone(), dir.path(), &url);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            let body = r#"{"detail":"blob expired"}"#;
            let answer = format!("HTTP/1.1 410 Gone\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            socket.write_all(answer.as_bytes()).await.unwrap();
        });
        let got = e.media_range("message", &lid, Some("bytes=0-")).await;
        assert!(matches!(got, Err(Error::Hub { status: 410, .. })), "{got:?}");
        server.await.unwrap();
        assert_eq!(store.message("message").unwrap().unwrap().attachments[0].state, "expired");
        // and isn't asked again
        let again = e.media_range("message", &lid, Some("bytes=0-")).await;
        assert!(matches!(again, Err(Error::Hub { status: 410, .. })), "{again:?}");
    }
}
