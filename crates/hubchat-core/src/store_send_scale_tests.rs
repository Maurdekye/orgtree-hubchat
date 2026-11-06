//! Send latency against history size (docket mail-hub-queueing-outgoing-messages-takes-longer).
//! MEASUREMENT: prints the time of each step of "press send" at growing history.
use super::*;
use std::time::Instant;

fn fill(s: &Store, n: usize, peers: usize) {
    s.with(|c| {
        let tx = c.transaction()?;
        for i in 0..n {
            let peer = format!("p{}.abcdef", i % peers);
            let out = (i % 2) as i64;
            tx.execute(
                "INSERT INTO messages(id, peer, outgoing, body, sent_at, received_at, created_at, state, seen)
                 VALUES(?,?,?,?,?,?,?,?,1)",
                params![format!("m{i}"), peer, out, "hello there", "2026-11-01T00:00:00Z", "2026-11-01T00:00:00Z",
                        format!("2026-11-01T00:{:02}:{:02}.{:03}Z", (i / 60000) % 60, (i / 1000) % 60, i % 1000),
                        if out == 1 { "read" } else { "received" }],
            )?;
        }
        tx.commit()
    })
    .unwrap();
}

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000.0 }

#[test]
#[ignore = "measurement"]
fn measure_send_steps() {
    for n in [50usize, 500, 2_500, 5_000, 20_000] {
        let s = Store::open_in_memory().unwrap();
        fill(&s, n, 1);
        let msg = NewOutgoing { id: "new1".into(), peer: "p0.abcdef".into(), body: "x".into(), kind: None, reply_to: None, attachments: vec![] };
        let t = Instant::now(); s.set_draft("p0.abcdef", "x").unwrap(); let d1 = ms(t);
        let t = Instant::now(); s.queue_outgoing(&msg, "2026-11-06T12:00:00Z").unwrap(); let q = ms(t);
        let t = Instant::now(); s.set_draft("p0.abcdef", "").unwrap(); let d2 = ms(t);
        let t = Instant::now(); let _ = s.chats().unwrap(); let ch = ms(t);
        let t = Instant::now(); let _ = s.chat("p0.abcdef", None, None, 50).unwrap(); let c50 = ms(t);
        let t = Instant::now(); let _ = s.unread("p0.abcdef").unwrap(); let un = ms(t);
        let first = s.chat("p0.abcdef", None, None, (n as u32).min(5000)).unwrap();
        let o = first[0].clone();
        let t = Instant::now(); let r = s.chat("p0.abcdef", None, Some((o.created_at.as_str(), o.id.as_str())), 5000).unwrap(); let rl = ms(t);
        println!("RELOAD n={n} loaded={} reload_from_oldest={rl:.2}ms", r.len());
        println!("SCALE n={n} draft={d1:.2}ms queue={q:.2}ms draft_clear={d2:.2}ms | chats={ch:.2}ms chat50={c50:.2}ms unread={un:.2}ms");
    }
}

/// Reading a chat's newest 200 rows costs one attachments statement however long the history is
/// (docket mail-hub-queueing-outgoing-messages-takes-longer): not one per row.
#[test]
fn reading_the_newest_window_is_flat_in_history() {
    let mut counts = Vec::new();
    for n in [50usize, 2_000, 20_000] {
        let s = Store::open_in_memory().unwrap();
        fill(&s, n, 1);
        let before = s.attach_queries.load(std::sync::atomic::Ordering::Relaxed);
        let rows = s.chat("p0.abcdef", None, None, 200).unwrap();
        counts.push((n, rows.len(), s.attach_queries.load(std::sync::atomic::Ordering::Relaxed) - before));
    }
    for (n, rows, q) in &counts {
        assert!(*q <= 1, "n={n}: {rows} rows read took {q} attachment statements: {counts:?}");
    }
}
