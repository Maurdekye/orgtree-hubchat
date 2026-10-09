//! Scan setup code, end to end on a real scratch hub: a scripted org plays
//! Orgtree's part (it reads the code on the first message's last line and
//! answers with the agreed reply line). The phone's engine joins the hub
//! from the setup link, sends the code, and records the org's answer; a
//! look-alike answer from anyone else, or naming another code, is ignored.

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hubchat_core::engine::{Engine, Event, Host};
use hubchat_core::hub::{Outgoing, Profile};
use hubchat_core::setup::{Outcome, SetupLink};
use hubchat_core::store::Store;
use hubchat_core::{HubAddress, HubClient, Identity};

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

fn phone(id: &str, dir: &std::path::Path) -> Arc<Engine> {
    let host = Arc::new(TestHost {
        events: Mutex::new(Vec::new()),
        downloads: dir.join(id),
    });
    let profile = Profile {
        kind: "person".into(),
        org_name: "Alex".into(),
        username: id.into(),
        blurb: String::new(),
    };
    Engine::new(
        Arc::new(Store::open_in_memory().unwrap()),
        Identity::generate(id).unwrap(),
        profile,
        host,
    )
}

/// A registered sender on the hub: the org, or an impostor.
async fn member(hub: &HubAddress, id: &str, kind: &str, name: &str) -> (HubClient, Identity) {
    let c = HubClient::new(hub.clone());
    let me = Identity::generate(id).unwrap();
    let p = Profile {
        kind: kind.into(),
        org_name: name.into(),
        username: id.into(),
        blurb: String::new(),
    };
    c.register(&me, &p).await.unwrap();
    (c, me)
}

/// The org's side: wait for a message whose last line names a code; return
/// (sender, code).
async fn read_code(c: &HubClient, me: &Identity) -> (String, String) {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(30) {
        let p = c.poll(me, 2).await.unwrap();
        let ids: Vec<String> = p.messages.iter().map(|m| m.id.clone()).collect();
        if !ids.is_empty() {
            c.ack(me, &ids).await.unwrap();
        }
        for m in p.messages {
            let last = m.body.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            if let Some(code) = last.trim().strip_prefix("Setup code: ") {
                return (m.from, code.to_owned());
            }
        }
    }
    panic!("the org got no setup code");
}

async fn until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !f() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `hub`: host:port.
fn link(hub: &str, org: &str, code: &str) -> SetupLink {
    SetupLink::parse(&format!(
        "hubchat://setup?v=1&hub=http%3A%2F%2F{}&org={org}&orgname=My%20Org\
         &pc=home-pc&ts=alex%40example.com&code={code}&net=tailscale&hubname=testhub",
        hub.replace(':', "%3A")
    ))
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_code_links_only_on_the_orgs_answer() {
    let Some(hub) = common::start_hub().await else {
        eprintln!("SKIPPED: no mailhub source");
        return;
    };
    run(&format!("127.0.0.1:{}", hub.port)).await;
}

/// The same on a mail hub v2.0, where the phone gets its mail through sync.
/// Set HUBCHAT_V2_HUB to a scratch v2 hub (e.g. 127.0.0.1:7397); skipped
/// when unset.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_code_links_on_a_v2_hub() {
    let Ok(hub) = std::env::var("HUBCHAT_V2_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_V2_HUB to a scratch mail hub v2.0");
        return;
    };
    run(hub.trim_start_matches("http://")).await;
}

async fn run(hub: &str) {
    let addr = HubAddress::parse(hub).unwrap();
    let (org_c, org) = member(&addr, "my-org", "org", "My Org").await;
    let (imp_c, imp) = member(&addr, "impostor", "org", "Not My Org").await;
    let dir = tempfile::tempdir().unwrap();

    // linked
    let alex = phone("alex", dir.path());
    alex.start().unwrap();
    let l = link(hub, &org.address(), "k7qd-4mxp");
    let peer = alex.setup_join(&l, "Alex", Duration::from_secs(20)).await.unwrap();
    assert_eq!(peer, org.address());
    let st = alex.setup_state(&peer).unwrap();
    assert_eq!((st.outcome, st.code.as_str()), (None, "K7QD-4MXP"));

    let (from, code) = read_code(&org_c, &org).await;
    assert_eq!(from, alex.me().address());
    assert_eq!(code, "K7QD-4MXP");
    let first = alex.store().chat(&peer, None, None, 10).unwrap();
    assert!(first[0].body.starts_with("Hi My Org, this is Alex, linking Hubchat on my phone."), "{}", first[0].body);

    // look-alikes first: someone else naming our code, the org naming another code
    imp_c
        .send(&imp, &Outgoing::new(&from, "Setup code: K7QD-4MXP linked"))
        .await
        .unwrap();
    org_c
        .send(&org, &Outgoing::new(&from, "Hello!\n\nSetup code: AAAA-BBBB linked"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(alex.setup_state(&peer).unwrap().outcome, None, "a look-alike answer counted");
    assert!(alex.setup_state(&imp.address()).is_none());

    org_c
        .send(&org, &Outgoing::new(&from, &format!("Welcome, Alex. I know this address is you now.\n\nSetup code: {code} linked")))
        .await
        .unwrap();
    until("the linked answer", 30, || {
        alex.setup_state(&peer).is_some_and(|s| s.outcome == Some(Outcome::Linked))
    })
    .await;
    let st = alex.setup_state(&peer).unwrap();
    assert!(st.at.is_some() && st.reply_id.is_some());

    // expired: a second phone with a stale code
    let pat = phone("pat", dir.path());
    pat.start().unwrap();
    let peer = pat
        .setup_join(&link(hub, &org.address(), "ZZZZ-9999"), "Pat", Duration::from_secs(20))
        .await
        .unwrap();
    let (from, code) = read_code(&org_c, &org).await;
    assert_eq!(from, pat.me().address());
    org_c
        .send(&org, &Outgoing::new(&from, &format!("That code didn't work.\nSetup code: {code} expired")))
        .await
        .unwrap();
    until("the expired answer", 30, || {
        pat.setup_state(&peer).is_some_and(|s| s.outcome == Some(Outcome::Expired))
    })
    .await;

    // an org that isn't on the hub: a clear error, nothing sent
    let sam = phone("sam", dir.path());
    sam.start().unwrap();
    let err = sam
        .setup_join(&link(hub, "nobody.alex.000000", "CCCC-DDDD"), "Sam", Duration::from_secs(20))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("isn't on home-pc's hub"), "{err}");

    // an unreachable hub: couldn't join, after the wait
    let t0 = Instant::now();
    let err = sam
        .setup_join(&link("127.0.0.1:1", &org.address(), "CCCC-DDDD"), "Sam", Duration::from_secs(2))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("couldn't join home-pc's hub"), "{err}");
    assert!(t0.elapsed() < Duration::from_secs(5));

    alex.shutdown();
    pat.shutdown();
    sam.shutdown();
}
