//! Question cards and multi-reply end to end (docs/question-cards.md)
//! against a running scratch mail hub with the `meta` feature: an agent's
//! cards, written as Orgtree writes them, arrive as cards on both of alex's
//! devices; one empty reply answers two of them by picks and skips the
//! third; the agent's side reads the plain-text version and the same meta;
//! alex's other device gets the same replies and picks, so it settles the
//! cards the same way. Set HUBCHAT_V2_HUB to the hub's address; skipped
//! when unset.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host, HubState};
use hubchat_core::hub::{Outgoing, Profile};
use hubchat_core::hub_v2::Before;
use hubchat_core::questions::{Answer, QuestionMeta, ReplyMeta};
use hubchat_core::store::{NewOutgoing, Store};
use hubchat_core::{HubAddress, HubClient, Identity};
use serde_json::json;

struct TestHost {
    events: Mutex<Vec<Event>>,
    downloads: PathBuf,
}

impl Host for TestHost {
    fn event(&self, ev: Event) {
        self.events.lock().unwrap().push(ev);
    }
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        std::fs::File::open(source)
    }
    fn download_dir(&self) -> PathBuf {
        self.downloads.clone()
    }
}

fn device(me: &Identity, device_id: &str, dir: &std::path::Path) -> Arc<Engine> {
    let host = Arc::new(TestHost {
        events: Mutex::new(Vec::new()),
        downloads: dir.join(device_id),
    });
    let store = Arc::new(Store::open_in_memory().unwrap());
    let profile = Profile {
        kind: "person".into(),
        org_name: me.id().into(),
        username: me.id().into(),
        blurb: String::new(),
    };
    let e = Engine::new(store, me.clone(), profile, host);
    e.set_device(device_id, device_id);
    e
}

async fn until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !f() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

fn uid(p: &str) -> String {
    format!("{p}{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

/// A card as Orgtree sends it: the options as a numbered list in the body,
/// and the same in meta with the list as the plain-text suffix.
fn card(id: &str, to: &str, question: &str, options: &[&str], multiple: bool) -> Outgoing {
    let (body, meta) = if options.is_empty() {
        (question.to_string(), None)
    } else {
        let mut body = format!("{question}\n\n{}", if multiple { "Pick one or more:" } else { "Pick one:" });
        for (i, o) in options.iter().enumerate() {
            body.push_str(&format!("\n{}. {o}", i + 1));
        }
        let meta = json!({ "question": { "options": options, "multiple": multiple }, "fallback": [question.len(), body.len()] });
        (body, Some(meta))
    };
    Outgoing {
        id: id.into(),
        to: to.into(),
        body,
        kind: Some("question".into()),
        thread_id: None,
        reply_to: None,
        sent_at: None,
        attachments: vec![],
        body_part: None,
        meta,
    }
}

fn reply(id: &str, to: &str, body: &str, reply_to: Option<&str>) -> NewOutgoing {
    NewOutgoing {
        id: id.into(),
        peer: to.into(),
        body: body.into(),
        kind: None,
        reply_to: reply_to.map(Into::into),
        attachments: vec![],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cards_answers_and_skips_cross_a_meta_hub() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub with the meta feature");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let alex = Identity::generate("alex").unwrap();
    let agent = Identity::generate("agent").unwrap();
    let a = device(&alex, "alex-pc", dir.path());
    let b = device(&alex, "alex-phone", dir.path());
    let g = device(&agent, "agent-side", dir.path());
    for e in [&a, &b, &g] {
        e.start().unwrap();
        e.add_hub(&hub).unwrap();
    }
    for e in [&a, &b, &g] {
        until("first sync done", 30, || {
            e.hub_statuses().iter().any(|s| s.state == HubState::Connected && s.answered_ms.is_some())
        })
        .await;
        assert!(e.hub_statuses()[0].features.iter().any(|f| f == "meta"), "the hub lists meta");
    }

    // The agent's three cards reach both of alex's devices as cards.
    let client = HubClient::new(HubAddress::parse(&hub).unwrap());
    let (q1, q2, q3) = (uid("q1-"), uid("q2-"), uid("q3-"));
    for out in [
        card(&q1, &alex.address(), "Which channel should rc4 go to?", &["Beta", "Stable", "Hold it back"], false),
        card(&q2, &alex.address(), "Which platforms should I test rc4 on?", &["Windows", "Android", "macOS"], true),
        card(&q3, &alex.address(), "Anything else for the rc4 release notes?", &[], false),
    ] {
        client.send(&agent, &out).await.unwrap();
    }
    for (e, who) in [(&a, "alex-pc"), (&b, "alex-phone")] {
        until(&format!("{who} gets the cards"), 20, || {
            [&q1, &q2, &q3].iter().all(|q| e.store().message(q).unwrap().is_some())
        })
        .await;
        let m1 = e.store().message(&q1).unwrap().unwrap();
        assert_eq!(m1.body, "Which channel should rc4 go to?", "{who}: the plain list is left out");
        assert_eq!(
            m1.qa.question,
            Some(QuestionMeta { options: vec!["Beta".into(), "Stable".into(), "Hold it back".into()], multiple: false }),
            "{who}"
        );
        assert!(e.store().message(&q2).unwrap().unwrap().qa.question.unwrap().multiple, "{who}");
        let m3 = e.store().message(&q3).unwrap().unwrap();
        assert_eq!((m3.kind.as_deref(), m3.qa.question), (Some("question"), None), "{who}: free-response");
    }
    println!("  ok  the agent's cards arrive as cards on both devices, without the plain list");

    // On the pc: one empty message answers q1 and q2 by picks and skips q3.
    let r1 = uid("r1-");
    let picks = vec![
        Answer { question: q1.clone(), picked: vec!["Beta".into()] },
        Answer { question: q2.clone(), picked: vec!["Windows".into(), "Android".into()] },
    ];
    let all = vec![q1.clone(), q2.clone(), q3.clone()];
    a.send_with(reply(&r1, &agent.address(), "", None), ReplyMeta { replies: all.clone(), answers: picks.clone() })
        .await
        .unwrap();
    until("the agent's side gets r1", 20, || g.store().message(&r1).unwrap().is_some()).await;

    // What crossed the hub: the plain text every client reads, reply_to the
    // first card, and the meta.
    let text = "> Which channel should rc4 go to?\nAnswer: Beta\n> Which platforms should I test rc4 on?\nAnswer: Windows, Android\n> Anything else for the rc4 release notes?\nSkipped\n";
    let page = client.history(&agent, &alex.address(), &Before::Cursor(String::new()), 50).await.unwrap();
    let raw = page.messages.iter().find(|m| m.env.id == r1).expect("r1 in the agent's history");
    assert_eq!(raw.env.body, text, "what a client without meta reads");
    assert_eq!(raw.env.reply_to.as_deref(), Some(q1.as_str()));
    let meta = raw.env.meta.clone().expect("meta on a meta hub");
    assert_eq!(meta["replies"], json!(all));
    assert_eq!(meta["answers"], json!(picks));
    assert_eq!(meta["fallback"], json!([0, text.len()]));
    let got = g.store().message(&r1).unwrap().unwrap();
    assert_eq!((got.body.as_str(), &got.qa.replies, &got.qa.answers), ("", &all, &picks), "a Hubchat there shows no text and the picks");
    println!("  ok  one empty reply: plain text with Answer/Skipped lines for older clients, the same replies and picks in meta");

    // The phone gets the pc's message with the same replies, picks and hub
    // receipt time, so both settle the cards the same way.
    until("alex-phone gets r1", 20, || b.store().message(&r1).unwrap().is_some()).await;
    let (pc, phone) = (a.store().message(&r1).unwrap().unwrap(), b.store().message(&r1).unwrap().unwrap());
    assert!(phone.outgoing);
    assert_eq!((phone.body.as_str(), phone.reply_to.as_deref()), ("", Some(q1.as_str())));
    assert_eq!((&phone.qa.replies, &phone.qa.answers), (&all, &picks));
    assert!(pc.received_at.is_some());
    assert_eq!(pc.received_at, phone.received_at, "both devices order by the same receipt");
    println!("  ok  alex's other device gets the same replies, picks and receipt time");

    // A single plain reply with text keeps the old form: one quote, no meta.
    let r2 = uid("r2-");
    a.send_with(reply(&r2, &agent.address(), "Thanks", Some(&q3)), ReplyMeta::default()).await.unwrap();
    until("the agent's side gets r2", 20, || g.store().message(&r2).unwrap().is_some()).await;
    let page = client.history(&agent, &alex.address(), &Before::Cursor(String::new()), 50).await.unwrap();
    let raw = page.messages.iter().find(|m| m.env.id == r2).expect("r2 in the agent's history");
    assert_eq!((raw.env.body.as_str(), raw.env.meta.is_none()), ("> Anything else for the rc4 release notes?\nThanks", true));
    assert_eq!(g.store().message(&r2).unwrap().unwrap().body, "Thanks");
    println!("  ok  a single plain reply is sent as before, without meta");
}
