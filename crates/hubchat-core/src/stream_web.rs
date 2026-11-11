//! Hubchat Web's side of `stream.rs` (the hub stream): the same `Carrier` and
//! `TransportStatus`, but `HubClient::sync` is always carried by the HTTP long
//! poll. `stream.rs` opens its socket with tokio's network stack and sends the
//! credentials in a header; a page has neither. A carrier over the browser's
//! WebSocket is Hubchat Web's later work; until then nothing changes for the
//! caller: the same request, the same `SyncResult`, the same cursor rules.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::hub::HubClient;
use crate::hub_v2::SyncResult;
use crate::{Identity, Result};

/// The same shape as `stream.rs`'s, so the hub's status reads alike everywhere.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TransportStatus {
    /// always "long poll" here
    pub transport: String,
    pub transport_since_ms: Option<u64>,
    /// always "browser" here
    pub fallback_reason: Option<String>,
    pub next_probe_ms: Option<u64>,
}

#[derive(Default)]
pub(crate) struct Carrier;

impl Carrier {
    pub(crate) async fn status(&self) -> TransportStatus {
        TransportStatus { transport: "long poll".into(), fallback_reason: Some("browser".into()), ..Default::default() }
    }

    pub(crate) async fn sync(&self, client: &HubClient, me: &Identity, body: Value, wait: u64) -> Result<SyncResult> {
        client.post_json(me, "/api/sync", &body, Duration::from_secs(wait + 30)).await
    }
}
