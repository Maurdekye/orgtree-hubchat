//! Inline video and audio in the chat (Hubchat 1.2.7; user 2026-10-30 14:24Z
//! "an inline video player for hubchat", 14:33Z "Yes, add audio"): which
//! attachments play inline, and the byte ranges a player asks for, answered a
//! bounded piece at a time. The shell's hcmedia scheme (src-tauri/src/media.rs)
//! passes each request to `Engine::media_range` (engine_media.rs), which reads
//! this device's copy or asks the hub for that range: nothing is saved, and the
//! attachment's download state is untouched.
use std::time::Duration;

use futures_util::StreamExt;

use crate::hub::{check, HubClient};
use crate::{Error, Identity, Result};

/// The most an attachment may weigh to play inline: Orgtree's media card
/// (renderer canvas/media.tsx MEDIA_MAX_BYTES). In step with src/lib/media.ts.
pub const MEDIA_MAX_BYTES: u64 = 256 * 1024 * 1024;
/// The most one answer carries; the player asks again for what follows, as it
/// does from Tauri's own asset protocol.
pub const MEDIA_CHUNK_BYTES: u64 = 1024 * 1024;

/// The type a playable attachment is served as, by its extension: the formats
/// of Orgtree's media card. None: it keeps its file card.
pub fn media_type(name: &str) -> Option<&'static str> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        _ => return None,
    })
}

/// The attachment a player's hcmedia URL names: its path `/<message id>/<local
/// id>`, which the page's convertFileSrc writes as one escaped segment.
pub fn media_ids(path: &str) -> Option<(String, String)> {
    let raw = unescape(path.strip_prefix('/')?)?;
    let (message, local) = raw.split_once('/')?;
    if message.is_empty() || local.is_empty() || local.contains('/') {
        return None;
    }
    Some((message.to_owned(), local.to_owned()))
}

fn unescape(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut rest = s.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            out.push(b);
            rest = tail;
        }
    }
    String::from_utf8(out).ok()
}

/// The status a player gets when its range can't be answered: 404 for what
/// doesn't play inline (or isn't there), the hub's own refusal, 503 while
/// Hubchat is paused or stopping, 502 when the hub can't be reached.
pub fn refusal_status(e: &Error) -> u16 {
    match e {
        Error::Invalid(_) => 404,
        Error::Hub { status: s @ (403 | 404 | 410), .. } => *s,
        Error::Cancelled | Error::FilePaused => 503,
        _ => 502,
    }
}

/// Bytes `start..=end` of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

impl Span {
    /// How many bytes it covers (never none).
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// The bytes a player's `Range` header asks of a `total`-byte file, at most
/// `chunk` of them. None when nothing there can be served (a start at or past
/// the end, an empty or inverted range, an empty file): a 416. Players always
/// send a range (`bytes=0-` first), so a missing or unreadable header reads from
/// the start; of a list of ranges, the first is served.
pub fn span(range: Option<&str>, total: u64, chunk: u64) -> Option<Span> {
    if total == 0 || chunk == 0 {
        return None;
    }
    let last = total - 1;
    let (start, end) = match range.and_then(first_range) {
        Some(Ask::From(start, end)) => (start, end.map_or(last, |e| e.min(last))),
        Some(Ask::Suffix(0)) => return None,
        Some(Ask::Suffix(n)) => (total.saturating_sub(n), last),
        None => (0, last),
    };
    if start > last || end < start {
        return None;
    }
    Some(Span { start, end: end.min(start.saturating_add(chunk - 1)) })
}

enum Ask {
    /// `bytes=a-` or `bytes=a-b`
    From(u64, Option<u64>),
    /// `bytes=-n`: the last n bytes
    Suffix(u64),
}

fn first_range(header: &str) -> Option<Ask> {
    let h = header.trim();
    let unit = h.get(..6)?;
    if !unit.eq_ignore_ascii_case("bytes=") {
        return None;
    }
    let (a, b) = h[6..].split(',').next()?.trim().split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() {
        return Some(Ask::Suffix(b.parse().ok()?));
    }
    let start = a.parse().ok()?;
    if b.is_empty() {
        return Some(Ask::From(start, None));
    }
    Some(Ask::From(start, Some(b.parse().ok()?)))
}

/// A `Content-Range` answer: `bytes s-e/t` (the bytes it carries and the
/// file's size; `t` may be `*`, unknown) or `bytes */t` (a 416's size).
fn content_range(header: &str) -> Option<(Option<Span>, Option<u64>)> {
    let h = header.trim();
    let unit = h.get(..6)?;
    if !unit.eq_ignore_ascii_case("bytes ") {
        return None;
    }
    let (range, total) = h[6..].trim().split_once('/')?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    let span = match range.trim() {
        "*" => None,
        r => {
            let (s, e) = r.split_once('-')?;
            let (start, end) = (s.trim().parse().ok()?, e.trim().parse().ok()?);
            if end < start {
                return None;
            }
            Some(Span { start, end })
        }
    };
    Some((span, total))
}

/// One answer to a player (`Engine::media_range`).
#[derive(Debug, PartialEq, Eq)]
pub enum MediaAnswer {
    /// The bytes of `span`, of a `total`-byte file of this type: a 206.
    Part { content_type: &'static str, span: Span, total: u64, body: Vec<u8> },
    /// Nothing at that range, of a file of `total` bytes if known: a 416.
    Unsatisfiable { total: Option<u64> },
}

/// What the hub gave for a range of an attachment.
#[derive(Debug, PartialEq, Eq)]
pub enum Ranged {
    /// The bytes of `span`, of a `total`-byte file.
    Bytes { span: Span, total: u64, body: Vec<u8> },
    /// Nothing at that range (the hub's 416), of a file of `total` bytes if it said.
    Unsatisfiable { total: Option<u64> },
}

impl HubClient {
    /// Bytes `want` of an attachment, kept in memory, never saved: a piece of a
    /// file playing inline. `total` is the size its message gave, for a hub that
    /// doesn't say. A hub that ignores the range (a 200) has its body read up to
    /// the end of `want` only.
    pub async fn get_range(&self, me: &Identity, attachment_id: &str, want: Span, total: u64) -> Result<Ranged> {
        self.session_work(async {
            let id: String = url::form_urlencoded::byte_serialize(attachment_id.as_bytes()).collect();
            let resp = self
                .http
                .get(self.address().join(&format!("/api/attachments/{id}")))
                .header("X-Org-Auth", me.auth_header())
                .header("Range", format!("bytes={}-{}", want.start, want.end))
                .timeout(Duration::from_secs(60))
                .send()
                .await?;
            let said = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(content_range);
            match resp.status().as_u16() {
                416 => Ok(Ranged::Unsatisfiable { total: said.and_then(|(_, t)| t) }),
                206 => {
                    let (Some(span), size) = said.ok_or_else(|| bad("a partial answer without its range"))? else {
                        return Err(bad("a partial answer without its range"));
                    };
                    if span.start != want.start {
                        return Err(bad("a partial answer from another place in the file"));
                    }
                    let keep = span.end.min(want.end) - span.start + 1;
                    let body = read_at_most(resp, 0, keep).await?;
                    if body.is_empty() {
                        return Err(bad("an empty partial answer"));
                    }
                    let end = span.start + body.len() as u64 - 1;
                    Ok(Ranged::Bytes { span: Span { start: span.start, end }, total: size.unwrap_or(total).max(end + 1), body })
                }
                _ => {
                    let resp = check(resp).await?;
                    // the whole file: skip to the start of the range, keep its bytes
                    let size = resp.content_length().unwrap_or(total);
                    if want.start >= size {
                        return Ok(Ranged::Unsatisfiable { total: Some(size) });
                    }
                    let body = read_at_most(resp, want.start, want.size()).await?;
                    if body.is_empty() {
                        return Err(bad("the file ended before the range"));
                    }
                    let end = want.start + body.len() as u64 - 1;
                    Ok(Ranged::Bytes { span: Span { start: want.start, end }, total: size.max(end + 1), body })
                }
            }
        })
        .await
    }
}

fn bad(what: &str) -> Error {
    Error::Unreachable(format!("the hub sent {what}"))
}

/// A body's bytes from `skip` on, at most `keep` of them; the rest is never read.
async fn read_at_most(resp: reqwest::Response, mut skip: u64, keep: u64) -> Result<Vec<u8>> {
    let mut body = resp.bytes_stream();
    let mut out = Vec::with_capacity(keep.min(MEDIA_CHUNK_BYTES) as usize);
    while (out.len() as u64) < keep {
        let Some(piece) = body.next().await else { break };
        let piece = piece.map_err(|e| Error::Unreachable(e.to_string()))?;
        let mut piece = &piece[..];
        if skip > 0 {
            let n = skip.min(piece.len() as u64) as usize;
            piece = &piece[n..];
            skip -= n as u64;
        }
        let room = (keep - out.len() as u64).min(piece.len() as u64) as usize;
        out.extend_from_slice(&piece[..room]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::HubAddress;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn the_formats_of_orgtrees_media_card_play_and_nothing_else() {
        let played = [
            ("clip.mp4", "video/mp4"),
            ("Clip.WEBM", "video/webm"),
            ("a.b.mp3", "audio/mpeg"),
            ("x.wav", "audio/wav"),
            ("x.ogg", "audio/ogg"),
            ("x.m4a", "audio/mp4"),
            ("x.aac", "audio/aac"),
            ("x.flac", "audio/flac"),
        ];
        for (name, ty) in played {
            assert_eq!(media_type(name), Some(ty), "{name}");
        }
        for name in ["mp4", "clip.mov", "clip.mkv", "song.opus", "x.png", "clip.mp4.zip", "noext", ""] {
            assert_eq!(media_type(name), None, "{name}");
        }
    }

    #[test]
    fn a_range_is_served_a_bounded_piece_at_a_time() {
        let total = 5 * MIB;
        let s = |start, end| Some(Span { start, end });
        // the player's first ask, and a missing or unreadable header: from the start
        assert_eq!(span(Some("bytes=0-"), total, MIB), s(0, MIB - 1));
        assert_eq!(span(None, total, MIB), s(0, MIB - 1));
        assert_eq!(span(Some("items=0-9"), total, MIB), s(0, MIB - 1));
        assert_eq!(span(Some("bytes=x-"), total, MIB), s(0, MIB - 1));
        // a seek, a bounded ask inside one piece, an end past the file
        assert_eq!(span(Some("bytes=3145728-"), total, MIB), s(3 * MIB, 4 * MIB - 1));
        assert_eq!(span(Some("bytes=10-19"), total, MIB), s(10, 19));
        assert_eq!(span(Some("BYTES=10-19"), total, MIB), s(10, 19));
        assert_eq!(span(Some("bytes=0-99999999999"), 100, MIB), s(0, 99));
        assert_eq!(span(Some("bytes=4718592-"), total, MIB), s(4718592, total - 1));
        // the end of the file (an index at the end, as an MP4 without faststart keeps it)
        assert_eq!(span(Some("bytes=-500"), total, MIB), s(total - 500, total - 1));
        assert_eq!(span(Some("bytes=-500"), 100, MIB), s(0, 99));
        assert_eq!(span(Some("bytes=-4194304"), total, MIB), s(MIB, 2 * MIB - 1));
        // of several ranges, the first
        assert_eq!(span(Some("bytes=5-9, 20-29"), total, MIB), s(5, 9));
        // nothing to serve: 416
        assert_eq!(span(Some("bytes=5242880-"), total, MIB), None);
        assert_eq!(span(Some("bytes=9-5"), total, MIB), None);
        assert_eq!(span(Some("bytes=-0"), total, MIB), None);
        assert_eq!(span(Some("bytes=0-"), 0, MIB), None);
    }

    #[test]
    fn a_media_url_names_one_attachment_of_one_message() {
        let ids = |m: &str, l: &str| Some((m.to_owned(), l.to_owned()));
        assert_eq!(media_ids("/msg-1%2Flocal_2"), ids("msg-1", "local_2"));
        assert_eq!(media_ids("/msg-1%2flocal_2"), ids("msg-1", "local_2"));
        assert_eq!(media_ids("/msg/local"), ids("msg", "local"));
        assert_eq!(media_ids("/m%C3%A9%2Fl"), ids("mé", "l"));
        for bad in ["", "/", "msg%2Flocal", "/msg", "/%2Flocal", "/msg%2F", "/a%2Fb%2Fc", "/msg%2", "/msg%zz%2Fl", "/m%FF%2Fl"] {
            assert_eq!(media_ids(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_refused_range_gets_the_status_of_its_reason() {
        let hub = |status| Error::Hub { status, detail: String::new() };
        assert_eq!(refusal_status(&Error::Invalid("not played inline".into())), 404);
        assert_eq!(refusal_status(&hub(410)), 410);
        assert_eq!(refusal_status(&hub(403)), 403);
        assert_eq!(refusal_status(&hub(404)), 404);
        assert_eq!(refusal_status(&hub(500)), 502);
        assert_eq!(refusal_status(&Error::Unreachable("refused".into())), 502);
        assert_eq!(refusal_status(&Error::Cancelled), 503);
        assert_eq!(refusal_status(&Error::FilePaused), 503);
    }

    #[test]
    fn content_ranges_are_read_as_hubs_write_them() {
        assert_eq!(content_range("bytes 0-99/1000"), Some((Some(Span { start: 0, end: 99 }), Some(1000))));
        assert_eq!(content_range("bytes 5-9/*"), Some((Some(Span { start: 5, end: 9 }), None)));
        assert_eq!(content_range("bytes */1000"), Some((None, Some(1000))));
        assert_eq!(content_range("bytes 9-5/1000"), None);
        assert_eq!(content_range("items 0-1/2"), None);
        assert_eq!(content_range("bytes 0-1"), None);
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

    /// A hub that answers one request with `answer`, after checking it.
    async fn hub_answering(answer: Vec<u8>, expect: &'static [&'static str]) -> (HubClient, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let head = request(&mut socket).await;
            assert!(head.starts_with("get /api/attachments/file%2b1 "), "{head}");
            assert!(head.contains("x-org-auth: "), "{head}");
            for line in expect {
                assert!(head.contains(line), "{line} in {head}");
            }
            let _ = socket.write_all(&answer).await;
        });
        (HubClient::new(HubAddress::parse(&url).unwrap()), server)
    }

    #[tokio::test]
    async fn the_hub_is_asked_for_the_range_and_its_piece_is_kept_in_memory() {
        let me = Identity::generate("media-proof").unwrap();
        let (client, server) = hub_answering(
            b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 1048576-1048580/9437184\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".to_vec(),
            &["range: bytes=1048576-2097151"],
        )
        .await;
        let want = Span { start: MIB, end: 2 * MIB - 1 };
        let got = client.get_range(&me, "file+1", want, 1).await.unwrap();
        assert_eq!(got, Ranged::Bytes { span: Span { start: MIB, end: MIB + 4 }, total: 9437184, body: b"hello".to_vec() });
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_hub_that_sends_more_than_asked_is_cut_at_the_range() {
        let me = Identity::generate("media-proof").unwrap();
        let (client, server) = hub_answering(
            b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-9/10\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789".to_vec(),
            &["range: bytes=0-3"],
        )
        .await;
        let got = client.get_range(&me, "file+1", Span { start: 0, end: 3 }, 10).await.unwrap();
        assert_eq!(got, Ranged::Bytes { span: Span { start: 0, end: 3 }, total: 10, body: b"0123".to_vec() });
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_hub_that_ignores_the_range_is_read_only_to_its_end() {
        let me = Identity::generate("media-proof").unwrap();
        let (client, server) = hub_answering(
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789".to_vec(),
            &["range: bytes=4-6"],
        )
        .await;
        let got = client.get_range(&me, "file+1", Span { start: 4, end: 6 }, 10).await.unwrap();
        assert_eq!(got, Ranged::Bytes { span: Span { start: 4, end: 6 }, total: 10, body: b"456".to_vec() });
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_range_past_the_hubs_file_is_unsatisfiable_with_its_size() {
        let me = Identity::generate("media-proof").unwrap();
        let (client, server) = hub_answering(
            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */10\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            &["range: bytes=20-29"],
        )
        .await;
        let got = client.get_range(&me, "file+1", Span { start: 20, end: 29 }, 40).await.unwrap();
        assert_eq!(got, Ranged::Unsatisfiable { total: Some(10) });
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_hub_refusal_is_its_status() {
        let me = Identity::generate("media-proof").unwrap();
        let (client, server) = hub_answering(
            b"HTTP/1.1 410 Gone\r\nContent-Type: application/json\r\nContent-Length: 26\r\nConnection: close\r\n\r\n{\"detail\":\"blob expired\"}\n".to_vec(),
            &["range: bytes=0-9"],
        )
        .await;
        let got = client.get_range(&me, "file+1", Span { start: 0, end: 9 }, 10).await;
        assert!(matches!(got, Err(Error::Hub { status: 410, .. })), "{got:?}");
        server.await.unwrap();
    }
}
