//! The hub's door against a real mail hub v2.0.1 (gap 5). Set
//! HUBCHAT_DOOR_HUB to a scratch hub with phone access on, its door on
//! loopback (e.g. 127.0.0.1:7403 with door 7404); skipped when unset. Set
//! HUBCHAT_DOOR_OFF_HUB too (a v2.0.1 hub with phone access off, e.g.
//! 127.0.0.1:7401) to check that case.

use std::time::Duration;

use hubchat_core::door::{find_door, qr_hubs, Door, DOOR_PORTS};
use hubchat_core::{HubAddress, HubClient};

const T: Duration = Duration::from_secs(2);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_real_hub_names_its_door() {
    let Ok(hub) = std::env::var("HUBCHAT_DOOR_HUB") else {
        eprintln!("SKIPPED: set HUBCHAT_DOOR_HUB to a scratch mail hub v2.0.1 with phone access on");
        return;
    };
    let hub = HubAddress::parse(&hub).unwrap().to_string();
    let door = HubClient::new(HubAddress::parse(&hub).unwrap())
        .healthz()
        .await
        .unwrap()
        .door
        .expect("the hub names no door: is phone access on?");
    let d = find_door(&hub, &[], &DOOR_PORTS, T).await;
    assert_eq!(d, Door::Found { port: door.port, on: vec!["127.0.0.1".parse().unwrap()] });
    // the door answers on loopback only, so the QR names just the hub
    assert_eq!(qr_hubs(&hub, Some("home-pc.invalid"), &[], &d, T).await, vec![hub.clone()]);

    if let Ok(off) = std::env::var("HUBCHAT_DOOR_OFF_HUB") {
        let off = HubAddress::parse(&off).unwrap().to_string();
        assert_eq!(find_door(&off, &[], &DOOR_PORTS, T).await, Door::Off);
    }
}
