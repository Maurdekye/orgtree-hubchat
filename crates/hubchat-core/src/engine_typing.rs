//! Typing (hub feature `typing`; Orgtree docket
//! show-agent-typing-in-hubchat-using-ordered-messa): heartbeats, both ways,
//! in memory only (user 2026-10-13 22:33Z and 22:34Z). There is no start and
//! no stop on the wire.
//!
//! **Shown.** Each heartbeat a sync answer relays tells the window once
//! (`Event::Typing`). The window shows "… is typing" while the last one
//! came within a second, by its own clock, so nothing here keeps or ends
//! it, and a writer that crashes can't leave it standing. A message from
//! that peer in the same answer wins: its heartbeat is dropped, and the
//! message itself ends the row. Never a notification.
//!
//! **Sent.** While our user writes a message (the composer reports each
//! edit), a heartbeat goes to the peer every `BEAT`, through the hub a
//! message to it would take now, when that hub lists `typing`. It goes on
//! until the last edit is `IDLE` old, or the composer stops it: sent,
//! cleared, chat left, app in the background. The device going out of use
//! stops it too, and so does sending a message to that peer. Only the
//! peer's address goes out, never a word of the draft.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Engine, Event};

/// How often a heartbeat goes while our user writes.
pub(super) const BEAT: Duration = Duration::from_millis(500);
/// A pause this long ends our user's typing (until the next edit).
pub(super) const IDLE: Duration = Duration::from_secs(3);
/// How long a sync waits after an answer that brought nothing but
/// heartbeats: the ones that come meanwhile wait at the hub and come
/// together, so a device takes at most four such answers a second.
pub(super) const PACE: Duration = Duration::from_millis(250);

/// The drafts our user is writing now, by peer.
#[derive(Default)]
pub(super) struct Composing {
    drafts: Mutex<HashMap<String, Draft>>,
    /// a number per draft, so that a loop ends when its draft does, even
    /// when a new one for the same peer starts at once
    next: std::sync::atomic::AtomicU64,
}

struct Draft {
    edited: Instant,
    run: u64,
}

impl Composing {
    /// An edit to the draft for `peer` at `now`. A new draft's number when
    /// a heartbeat loop must start for it (none runs).
    fn edit(&self, peer: &str, now: Instant) -> Option<u64> {
        let mut m = self.drafts.lock().unwrap();
        if let Some(d) = m.get_mut(peer) {
            d.edited = now;
            return None;
        }
        let run = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        m.insert(peer.to_string(), Draft { edited: now, run });
        Some(run)
    }

    /// The composer stopped writing to `peer`. Whether it was writing.
    fn stop(&self, peer: &str) -> bool {
        self.drafts.lock().unwrap().remove(peer).is_some()
    }

    fn stop_all(&self) {
        self.drafts.lock().unwrap().clear();
    }

    /// Does draft `run` for `peer` still beat at `now`? A draft whose last
    /// edit is `IDLE` old ends here.
    fn beats(&self, peer: &str, run: u64, now: Instant) -> bool {
        let mut m = self.drafts.lock().unwrap();
        match m.get(peer) {
            Some(d) if d.run == run && now.saturating_duration_since(d.edited) < IDLE => true,
            Some(d) if d.run == run => {
                m.remove(peer);
                false
            }
            _ => false,
        }
    }
}

impl Engine {
    /// A sync answer relayed typing heartbeats (`rows`): tell the window,
    /// once per peer, less `just_wrote` (their message in the same answer
    /// wins).
    pub(super) fn typing_beats(&self, rows: &[crate::hub_v2::TypingRow], just_wrote: &BTreeSet<String>) {
        let me = self.me.address();
        let mut told = BTreeSet::new();
        for r in rows {
            if r.from != me && !just_wrote.contains(&r.from) && told.insert(r.from.as_str()) {
                self.host.event(Event::Typing { peer: r.from.clone() });
            }
        }
    }

    /// The composer: our user edited the draft for `peer` (`on`), or
    /// stopped writing to it (`on` false: sent, cleared, chat left, app in
    /// the background).
    pub fn composing(self: &Arc<Self>, peer: &str, on: bool) {
        if !on {
            self.composing.stop(peer);
            return;
        }
        if peer == self.me.address() || self.stopped.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let Some(run) = self.composing.edit(peer, Instant::now()) else { return };
        let me = self.clone();
        let peer = peer.to_string();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(BEAT);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                if me.stopped.load(std::sync::atomic::Ordering::Relaxed) || !me.composing.beats(&peer, run, Instant::now()) {
                    return;
                }
                me.typing_beat(&peer).await;
            }
        });
    }

    /// Nobody here is writing any more (the device went out of use, its
    /// hub loops were parked).
    pub(super) fn composing_ends(&self) {
        self.composing.stop_all();
    }

    /// Our user sent `peer` a message: that draft is done.
    pub(super) fn composing_sent(&self, peer: &str) {
        self.composing.stop(peer);
    }

    /// One heartbeat to `peer`, through the hub its next message would
    /// take, when that hub relays typing. Best effort: a heartbeat that
    /// fails is simply missing (the hub's sync session sees to the hub).
    async fn typing_beat(&self, peer: &str) {
        let next = match self.route(peer, &self.connected_hubs()) {
            Ok(r) => r.next,
            Err(_) => None,
        };
        let Some((url, client, _)) = next else { return };
        let relays = self.hubs.lock().unwrap().get(&url).is_some_and(|h| h.status.features.iter().any(|f| f == "typing"));
        if relays {
            let _ = client.typing(&self.me, &[peer.to_string()], BEAT).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_draft_beats_until_its_last_edit_is_idle_old() {
        let c = Composing::default();
        let t = Instant::now();
        let run = c.edit("ann", t).expect("the first edit starts a loop");
        // further edits keep it going and start no second loop
        assert_eq!(c.edit("ann", t + ms(400)), None);
        assert!(c.beats("ann", run, t + ms(500)));
        assert!(c.beats("ann", run, t + ms(400) + IDLE - ms(1)));
        // a pause of IDLE ends it, and the next edit starts a new one
        assert!(!c.beats("ann", run, t + ms(400) + IDLE));
        assert!(!c.beats("ann", run, t + ms(400) + IDLE + ms(500)), "it stays ended");
        assert!(c.edit("ann", t + ms(5000)).is_some_and(|r| r != run));
    }

    #[test]
    fn stopping_ends_the_draft_at_its_next_beat() {
        let c = Composing::default();
        let t = Instant::now();
        let run = c.edit("ann", t).unwrap();
        assert!(c.stop("ann"));
        assert!(!c.beats("ann", run, t + ms(500)));
        assert!(!c.stop("ann"), "stopping twice is nothing");
    }

    #[test]
    fn a_new_draft_at_once_ends_the_old_loop() {
        let c = Composing::default();
        let t = Instant::now();
        let first = c.edit("ann", t).unwrap();
        c.stop("ann"); // sent
        let second = c.edit("ann", t + ms(100)).expect("the next message's first edit");
        assert!(!c.beats("ann", first, t + ms(500)), "the old loop ends");
        assert!(c.beats("ann", second, t + ms(500)), "the new one beats");
        // and the old loop never ends the new draft
        assert!(c.beats("ann", second, t + ms(1000)));
    }

    #[test]
    fn drafts_to_two_peers_are_apart_and_end_together_when_the_device_rests() {
        let c = Composing::default();
        let t = Instant::now();
        let a = c.edit("ann", t).unwrap();
        let b = c.edit("bob", t).unwrap();
        c.stop("ann");
        assert!(!c.beats("ann", a, t + ms(500)));
        assert!(c.beats("bob", b, t + ms(500)));
        c.stop_all();
        assert!(!c.beats("bob", b, t + ms(1000)));
    }

    #[test]
    fn a_heartbeat_carries_the_addresses_and_nothing_else() {
        let body = crate::hub_v2::typing_body(&["ann.k3x9".to_string()]);
        assert_eq!(body, serde_json::json!({ "to": ["ann.k3x9"] }));
    }
}
