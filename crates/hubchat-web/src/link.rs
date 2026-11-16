//! Device linking in Hubchat Web: the apps' link commands
//! (`src-tauri/src/link.rs`, scheme in `hubchat_core::link`) answered by the
//! web shell, under the same names and with the same answers.
//!
//! New browser: `hc_link_start` shows a code (or joins the code of a QR the
//! camera read) and listens on a throwaway address for up to ten minutes;
//! progress goes to the UI as `hc-link` events. Signed-in browser:
//! `hc_link_offer`, `hc_link_lookup` and `hc_link_approve` send another
//! device the sealed identity. Also the key QR and the key file.
//!
//! What differs from the apps, because a page differs:
//! - a page may only call https (spec R2.2): of the addresses a link or an
//!   arriving identity names for a hub, only those are tried and kept;
//! - when none of them is one a page can call, or none answers, the hub is
//!   looked for where it may also be (`second_round`): at the same host and
//!   port over https (a hub on a LAN that answers both there), and at the
//!   hubs this page's server suggests, which count only under the name the
//!   link gives its hub. An app's link names its hub by http, as the apps
//!   use it;
//! - a browser that has an identity does not switch to another one through
//!   a link (coordinator 2026-11-11): it says `HAS_IDENTITY`;
//! - the key file is text the page reads and saves (`text`, not a path).

use std::sync::Arc;
use std::time::Duration;

use hubchat_core::hub::{CancelFlag, Profile};
use hubchat_core::link::{self, Bundle, LinkUrl};
use hubchat_core::{rt, HubAddress, HubClient};
use serde::Serialize;
use serde_json::{json, Value};

use crate::shell::{arg, out, page_can_reach, s, Adoption, Probe, Shell, NEEDS_HTTPS, R};

/// A link whose hub has no address a page may call.
pub const LINK_NEEDS_HTTPS: &str =
    "This link's hub has no https:// address, and Hubchat Web can only reach a hub over HTTPS. Ask the hub's owner for its https:// address.";

/// A link would bring an identity to a browser that has one.
pub const HAS_IDENTITY: &str = "This browser already has an identity.";

/// The name the key file is saved under, as the apps suggest it.
pub const KEY_FILE_NAME: &str = "hubchat-key.hubchat-key";

const LINK_TTL: Duration = Duration::from_secs(600);

/// Progress of `hc_link_start`, as the apps emit it (event `hc-link`).
#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkEvent {
    Waiting { expires_in_s: u64 },
    /// This browser already IS that identity: nothing to do.
    Same { address: String },
    /// An identity arrived and waits in memory until `hc_link_confirm` or
    /// `hc_link_discard`. `from` is always null here (no switch on the web).
    Review { from: Option<String>, to: String, name: String, hubs: Vec<HubRow> },
    Failed { error: String },
    Expired,
}

/// One hub that comes with a linked identity, as the review shows it.
#[derive(Serialize, Clone)]
pub struct HubRow {
    /// The hub as the other device knows it.
    theirs: String,
    /// The best address for this browser: one that answered, else the first candidate.
    address: String,
    /// Addresses tried, likeliest first: only those a page can call, when there are any.
    candidates: Vec<String>,
    name: Option<String>,
    error: Option<String>,
    reachable: bool,
}

/// The address as the shell keeps it, when it is one a page can call.
fn reachable_form(input: &str) -> Option<String> {
    HubAddress::parse(input).ok().map(|a| a.to_string()).filter(|u| page_can_reach(u))
}

/// The hubs of `list` a page can call, each once, in order.
fn reachable_hubs(list: &[String]) -> Vec<String> {
    let mut keep: Vec<String> = Vec::new();
    for h in list.iter().filter_map(|h| reachable_form(h)) {
        if !keep.contains(&h) {
            keep.push(h);
        }
    }
    keep
}

/// What else a page tries for a hub none of whose addresses it can call (`named`): the same
/// host and port over https for each http one, and, when the link says what its hub is called
/// (`hub_is_named`), the hubs this page's server suggests. Likeliest first, each once.
fn second_round(named: &[String], suggested: &[String], hub_is_named: bool) -> Vec<String> {
    let mut more: Vec<String> = Vec::new();
    let twins = named.iter().filter(|h| reachable_form(h).is_none()).filter_map(|h| link::https_twin(h)).map(|a| a.to_string());
    // without the hub's name a suggested hub could not be told from any other hub
    let listed = suggested.iter().filter(|_| hub_is_named).filter_map(|h| reachable_form(h));
    for address in twins.chain(listed) {
        if !more.contains(&address) {
            more.push(address);
        }
    }
    more
}

impl Adoption {
    /// The identity a link, a key QR or a key file brought, with the hubs to add.
    fn from_bundle(bundle: &Bundle, hubs: Vec<String>) -> R<Adoption> {
        let mut meta = vec![("profile.name", bundle.name.clone()), ("profile.about", bundle.about.clone())];
        if bundle.recovery_saved {
            meta.push(("recovery.saved", "yes".into()));
        }
        Ok(Adoption { me: bundle.identity().map_err(s)?, brand_new: false, meta, hubs })
    }
}

impl Shell {
    fn emit_link(&self, ev: &LinkEvent) {
        if let Ok(json) = serde_json::to_string(ev) {
            self.emit("hc-link", &json);
        }
    }

    /// The identity commands of linking (`hc_link_confirm`, `hc_restore_qr`,
    /// `hc_key_file_import`): what to take, for `Shell::plan_identity`.
    pub(crate) fn plan_link(&self, cmd: &str, args: &Value) -> R<Option<Adoption>> {
        if !matches!(cmd, "hc_link_confirm" | "hc_restore_qr" | "hc_key_file_import") {
            return Ok(None);
        }
        if self.engine().is_ok() {
            return Err(HAS_IDENTITY.into());
        }
        let plan = match cmd {
            "hc_link_confirm" => {
                // exactly the hubs the user kept in the review; checked before anything is taken
                let mut keep: Vec<String> = Vec::new();
                for h in arg::<Vec<String>>(args, "hubs")?.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
                    let a = HubAddress::parse(h).map_err(s)?.to_string();
                    if !page_can_reach(&a) {
                        return Err(NEEDS_HTTPS.into());
                    }
                    if !keep.contains(&a) {
                        keep.push(a);
                    }
                }
                let bundle = self.pending.lock().unwrap().take().ok_or("nothing to confirm: link again")?;
                Adoption::from_bundle(&bundle, keep)?
            }
            "hc_restore_qr" => {
                let b = link::from_qr(&arg::<String>(args, "text")?).map_err(s)?;
                Adoption::from_bundle(&b, reachable_hubs(&link::hubs_for_device(&b.hubs, None, &[])))?
            }
            _ => {
                let (text, passphrase): (String, String) = (arg(args, "text")?, arg(args, "passphrase")?);
                let b = link::from_key_file(&text, &passphrase).map_err(s)?;
                Adoption::from_bundle(&b, reachable_hubs(&link::hubs_for_device(&b.hubs, None, &[])))?
            }
        };
        Ok(Some(plan))
    }

    fn my_bundle(&self) -> R<Bundle> {
        let e = self.engine()?;
        Ok(Bundle {
            id: e.me().id().into(),
            secret: e.me().secret().into(),
            name: self.meta("profile.name"),
            about: self.meta("profile.about"),
            hubs: self.store().hubs().map_err(s)?.into_iter().map(|h| h.url).collect(),
            recovery_saved: self.meta("recovery.saved") == "yes",
        })
    }

    /// A link's hub under each address the link names: one a page can call that answers (as
    /// `name`, when given). When none does, the hub where it may also be (`second_round`).
    /// Failing both, the first address's failure, or that the link's hub has no https address.
    async fn probe_first(&self, hubs: &[String], name: Option<&str>, limit: Duration, origin: Option<&link::Reception>) -> Probe {
        if hubs.is_empty() {
            return Probe::Invalid { error: "the link names no hub".into() };
        }
        let failure = match self.first_answer(&reachable_hubs(hubs), name, limit, origin).await {
            Some(Ok(connected)) => return connected,
            Some(Err(failure)) => failure,
            None => Probe::Invalid { error: LINK_NEEDS_HTTPS.into() },
        };
        let suggested = self.suggested.lock().unwrap().clone();
        match self.first_answer(&second_round(hubs, &suggested, name.is_some()), name, limit, origin).await {
            Some(Ok(connected)) => connected,
            _ => failure,
        }
    }

    /// The likeliest of `addresses` (the earliest named) that answers as a hub (as `name`, when
    /// given). All are tried at once, `limit` each, and the wait ends as soon as one connected
    /// and every likelier one has failed. `Err`: none did, with the first address's failure.
    /// `None`: there was no address.
    async fn first_answer(&self, addresses: &[String], name: Option<&str>, limit: Duration, origin: Option<&link::Reception>) -> Option<Result<Probe, Probe>> {
        use futures_util::stream::{FuturesUnordered, StreamExt};
        let mut tries: FuturesUnordered<_> =
            addresses.iter().enumerate().map(|(i, h)| async move { (i, self.probe_with(h, name, limit, origin.cloned()).await) }).collect();
        let mut answers: Vec<Option<Probe>> = addresses.iter().map(|_| None).collect();
        while let Some((i, probe)) = tries.next().await {
            answers[i] = Some(probe);
            // the answers in from the start of the list on: the first of them that connected is the one
            let known = answers.iter().take_while(|a| a.is_some()).count();
            if let Some(best) = answers[..known].iter().position(|a| matches!(a, Some(Probe::Connected { .. }))) {
                return answers[best].take().map(Ok);
            }
        }
        answers.into_iter().next().flatten().map(Err)
    }

    /// The identity a link carries just arrived. This browser's own: nothing
    /// to do. Otherwise it waits in memory (an unadopted key is never
    /// written anywhere) while the user reviews its hubs, each tried here.
    async fn arrived(&self, bundle: Bundle, via: &str, aliases: &[String], reception: &link::Reception) -> (LinkEvent, Option<Bundle>) {
        let current = self.engine().ok().map(|e| e.me().address());
        let to = match bundle.identity() {
            Ok(me) => me.address(),
            Err(e) => return (LinkEvent::Failed { error: s(e) }, None),
        };
        if current.as_deref() == Some(to.as_str()) {
            return (LinkEvent::Same { address: to }, None);
        }
        if current.is_some() {
            return (LinkEvent::Failed { error: HAS_IDENTITY.into() }, None);
        }
        let rows = link::review_hubs(&bundle.hubs, via, aliases);
        let probed: Vec<(HubRow, Option<link::HubKey>)> = futures_util::future::join_all(rows.into_iter().map(|(theirs, all)| async move {
            // the addresses a page can call; for a hub known by http only, the same over https
            let own = reachable_hubs(&all);
            let reach = if own.is_empty() { second_round(&all, &[], false) } else { own.clone() };
            match self.probe_first(&all, None, Duration::from_secs(5), Some(reception)).await {
                Probe::Connected { url, name, version, orgs, .. } => {
                    let key = (name.clone(), version, orgs);
                    (HubRow { theirs, address: url, candidates: reach, name: Some(name), error: None, reachable: true }, Some(key))
                }
                // no address a page can call, and nothing answered over https at the same one
                _ if own.is_empty() => {
                    let address = all.first().cloned().unwrap_or_default();
                    (HubRow { theirs, address, candidates: all, name: None, error: Some(NEEDS_HTTPS.into()), reachable: false }, None)
                }
                Probe::Unreachable { error, .. } | Probe::NotAHub { error, .. } | Probe::Invalid { error } => {
                    (HubRow { theirs, address: own[0].clone(), candidates: own, name: None, error: Some(error), reachable: false }, None)
                }
            }
        }))
        .await;
        // one row per hub: the same hub under two names merges
        let keyed: Vec<(String, Vec<String>, Option<link::HubKey>)> = probed.iter().map(|(r, k)| (r.theirs.clone(), r.candidates.clone(), k.clone())).collect();
        let hubs: Vec<HubRow> = link::merge_same_hubs(&keyed, &bundle.hubs, via)
            .into_iter()
            .map(|(i, theirs, candidates)| HubRow { theirs, candidates, ..probed[i].0.clone() })
            .collect();
        let name = bundle.name.clone();
        (LinkEvent::Review { from: None, to, name, hubs }, Some(bundle))
    }

    /// What a link brought, checked and offered for review: the tests' way in, without a hub.
    #[cfg(test)]
    pub(crate) async fn receive(&self, bundle: Bundle, via: &str, aliases: &[String], reception: &link::Reception) {
        let (event, pending) = self.arrived(bundle, via, aliases, reception).await;
        if let Some(bundle) = pending {
            *self.pending.lock().unwrap() = Some(bundle);
        }
        self.emit_link(&event);
    }

    async fn link_start(self: &Arc<Self>, args: &Value) -> R<Value> {
        if self.engine().is_ok() {
            return Err(HAS_IDENTITY.into());
        }
        let (hub, device_name): (String, String) = (arg(args, "hub")?, arg(args, "deviceName")?);
        let (code, aliases): (Option<String>, Option<Vec<String>>) = (arg(args, "code")?, arg(args, "aliases")?);
        let addr = HubAddress::parse(&hub).map_err(s)?;
        if !page_can_reach(addr.as_str()) {
            return Err(NEEDS_HTTPS.into());
        }
        // a code from a scanned or typed link (the other device made it), or our own
        let code = match code {
            Some(c) => link::format_code(&link::normalize_code(&c).map_err(s)?),
            None => link::new_link_code(),
        };
        let temp = link::link_identity(&code).map_err(s)?;
        let client = link::start_client(self.store(), addr.clone(), || None).map_err(s)?;
        let name = if device_name.trim().is_empty() { self.device_name() } else { device_name.trim().chars().take(48).collect() };
        let profile = Profile { kind: "chat".into(), org_name: name, username: "link".into(), blurb: "Waiting to be linked to a Hubchat identity".into() };
        client.register(&temp, &profile).await.map_err(s)?;
        let stop = CancelFlag::default();
        if let Some(old) = self.link_stop.lock().unwrap().replace(stop.clone()) {
            old.cancel();
        }
        let (shell, code2, via, aliases) = (self.clone(), code.clone(), addr.to_string(), aliases.unwrap_or_default());
        rt::spawn(async move {
            let reception = link::Reception::new(client.clone(), stop.clone());
            let t0 = rt::Instant::now();
            reception.commit(|| shell.emit_link(&LinkEvent::Waiting { expires_in_s: LINK_TTL.as_secs() }));
            let outcome = loop {
                if stop.is_cancelled() || client.session_cancelled() {
                    break None;
                }
                if t0.elapsed() > LINK_TTL {
                    break Some(LinkEvent::Expired);
                }
                let p = match reception.work(client.poll(&temp, 20)).await {
                    Ok(p) => p,
                    Err(_) if stop.is_cancelled() || client.session_cancelled() => break None,
                    Err(_) => {
                        rt::time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                };
                let ids: Vec<String> = p.messages.iter().map(|m| m.id.clone()).collect();
                let found = p.messages.iter().find_map(|m| link::open_from_link(&code2, &m.body).ok().map(|b| (m.id.clone(), b)));
                if let Some((id, bundle)) = found {
                    // hubs that keep history would keep our copy of the sealed identity: delete it
                    let cleanup = async {
                        if !ids.is_empty() {
                            if let Err(e @ (hubchat_core::Error::Cancelled | hubchat_core::Error::FilePaused)) = client.ack(&temp, &ids).await {
                                return Err(e);
                            }
                        }
                        if let Err(e @ (hubchat_core::Error::Cancelled | hubchat_core::Error::FilePaused)) = client.delete_message(&temp, &id).await {
                            return Err(e);
                        }
                        Ok(())
                    };
                    let _ = reception
                        .complete(cleanup, async { Ok(shell.arrived(bundle, &via, &aliases, &reception).await) }, |(event, pending)| {
                            if let Some(bundle) = pending {
                                *shell.pending.lock().unwrap() = Some(bundle);
                            }
                            shell.emit_link(&event);
                        })
                        .await;
                    break None;
                }
                if !ids.is_empty() {
                    let _ = client.ack(&temp, &ids).await;
                }
            };
            let _ = client.unregister(&temp).await;
            if let Some(ev) = outcome {
                reception.commit(|| shell.emit_link(&ev));
            }
        });
        let hub = addr.to_string();
        let qr = LinkUrl { code: link::normalize_code(&code).map_err(s)?, hubs: vec![hub.clone()], hub_name: None, role: Some("take".into()) }.to_url();
        Ok(json!({ "code": code, "qr": qr, "hub": hub, "phone_access_off": false }))
    }

    /// A link code this browser offered or approved is done with: after
    /// `delay`, take the waiting address it names off every hub we use.
    fn forget_link_address(&self, code: &str, delay: Duration) {
        let (Ok(engine), Ok(temp)) = (self.engine(), link::link_identity(code)) else { return };
        let hubs: Vec<HubClient> = engine.hub_statuses().iter().filter_map(|h| engine.hub_client(&h.url)).collect();
        rt::spawn(async move {
            rt::time::sleep(delay).await;
            for client in hubs {
                let _ = client.unregister(&temp).await;
            }
        });
    }

    /// The link, QR and key-file commands. None: not one of them.
    pub(crate) async fn link_call(self: &Arc<Self>, cmd: &str, args: &Value) -> Option<R<Value>> {
        Some(match cmd {
            "hc_parse_link" => (|| {
                let l = LinkUrl::parse(&arg::<String>(args, "input")?).map_err(s)?;
                Ok(json!({ "code": link::format_code(&l.code), "hub": l.hubs.first(), "hubs": l.hubs, "hub_name": l.hub_name, "role": l.role }))
            })(),
            "hc_probe_link_hubs" => match (arg::<Vec<String>>(args, "hubs"), arg::<Option<String>>(args, "name")) {
                (Ok(hubs), Ok(name)) => out(self.probe_first(&hubs, name.as_deref(), Duration::from_secs(5), None).await),
                (Err(e), _) | (_, Err(e)) => Err(e),
            },
            "hc_link_start" => self.link_start(args).await,
            "hc_link_cancel" => {
                if let Some(f) = self.link_stop.lock().unwrap().take() {
                    f.cancel();
                }
                Ok(Value::Null)
            }
            "hc_link_discard" => {
                self.pending.lock().unwrap().take();
                Ok(Value::Null)
            }
            // a caller without a key store of its own (tests); the worker plans first (wasm.rs)
            "hc_link_confirm" | "hc_restore_qr" | "hc_key_file_import" => match self.plan_link(cmd, args) {
                Ok(Some(plan)) => self.adopt(plan).map(Value::String),
                Ok(None) => return None,
                Err(e) => Err(e),
            },
            // ------------------------------------------- signed-in browser
            "hc_link_offer" => self.link_offer(args),
            "hc_link_lookup" => self.link_lookup(args),
            "hc_link_approve" => self.link_approve(args).await,
            "hc_link_forget" => (|| {
                let code = link::normalize_code(&arg::<String>(args, "code")?).map_err(s)?;
                self.forget_link_address(&code, Duration::ZERO);
                Ok(Value::Null)
            })(),
            // the offline QR (shown behind a warning in the UI)
            "hc_key_qr" => self.my_bundle().map(|b| Value::String(link::to_qr(&b))),
            // The apps write the passphrase-locked key file themselves (`hc_key_file_export`); a
            // page gets its text and hands it to the browser (src/web/webApi.ts).
            "hc_key_file_text" => (|| {
                let text = link::to_key_file(&self.my_bundle()?, &arg::<String>(args, "passphrase")?).map_err(s)?;
                Ok(json!({ "name": KEY_FILE_NAME, "text": text }))
            })(),
            _ => return None,
        })
    }

    /// Signed-in browser: a one-time code (and QR) for a NEW device to scan or type.
    fn link_offer(&self, args: &Value) -> R<Value> {
        let e = self.engine()?;
        let statuses = e.hub_statuses();
        let hub = match arg::<Option<String>>(args, "hub")? {
            Some(h) => HubAddress::parse(&h).map_err(s)?.to_string(),
            None => statuses
                .iter()
                .find(|h| matches!(h.state, hubchat_core::engine::HubState::Connected))
                .or_else(|| statuses.iter().find(|h| h.state != hubchat_core::engine::HubState::Disabled))
                .map(|h| h.url.clone())
                .ok_or("add a hub first: the new device links through one")?,
        };
        e.hub_client(&hub).ok_or("Enable this saved hub on this device before linking.")?;
        let hub_name = statuses.iter().find(|h| h.url == hub).map(|h| h.name.clone()).filter(|n| !n.trim().is_empty());
        // a page knows no other address of its hub than the one it calls
        let hubs = link::hub_aliases(&hub, None, &[], None);
        let code = link::new_link_code();
        let qr = LinkUrl { code: link::normalize_code(&code).map_err(s)?, hubs: hubs.clone(), hub_name, role: Some("give".into()) }.to_url();
        Ok(json!({ "code": code, "qr": qr, "hub": hub, "hubs": hubs, "phone_access_off": false }))
    }

    fn link_lookup(&self, args: &Value) -> R<Value> {
        let l = LinkUrl::parse(&arg::<String>(args, "input")?).map_err(s)?;
        let address = link::link_identity(&l.code).map_err(s)?.address();
        let entry = self.store().directory().map_err(s)?.into_iter().find(|d| d.address == address);
        let mine: Vec<String> = self.store().hubs().map_err(s)?.into_iter().map(|h| h.url).collect();
        let ours = |h: &String| HubAddress::parse(h).map(|a| mine.contains(&a.to_string())).unwrap_or(false);
        let unknown_hub = if l.hubs.iter().any(ours) { None } else { l.hubs.first().cloned() };
        Ok(json!({
            "code": l.code,
            "address": address,
            "device_name": entry.as_ref().map(|e| e.org_name.clone()),
            "hubs": entry.map(|e| e.hubs).unwrap_or_default(),
            "unknown_hub": unknown_hub,
        }))
    }

    async fn link_approve(&self, args: &Value) -> R<Value> {
        let e = self.engine()?;
        let code = link::normalize_code(&arg::<String>(args, "code")?).map_err(s)?;
        let to = link::link_identity(&code).map_err(s)?.address();
        let body = link::seal_for_link(&code, &self.my_bundle()?).map_err(s)?;
        let hub = e.send_unlisted(&to, &body).await.map_err(s)?;
        // the new device takes its message and leaves the hub within seconds; if it
        // crashed first, its waiting address would stay on the hub's list for good
        self.forget_link_address(&code, Duration::from_secs(180));
        Ok(Value::String(hub))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::tests::{fresh, take};
    use hubchat_core::Identity;

    fn bundle(id: &str, hubs: &[&str]) -> Bundle {
        let me = Identity::generate(id).unwrap();
        Bundle { id: me.id().into(), secret: me.secret().into(), name: "Maya R".into(), about: "on call".into(), hubs: hubs.iter().map(|h| h.to_string()).collect(), recovery_saved: true }
    }

    fn events(seen: &Arc<std::sync::Mutex<Vec<(String, String)>>>) -> Vec<Value> {
        seen.lock().unwrap().iter().filter(|(n, _)| n == "hc-link").map(|(_, j)| serde_json::from_str(j).unwrap()).collect()
    }

    /// A hub on loopback that answers every request with /healthz of a hub called `name`.
    async fn fake_hub(name: &str) -> String {
        fake_hub_after(name, Duration::ZERO).await
    }

    /// The same, answering only after `wait`.
    async fn fake_hub_after(name: &str, wait: Duration) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body = json!({ "ok": true, "name": name, "orgs": 1, "version": "2.5.1", "features": ["sync"], "max_attachment_bytes": 1000000 }).to_string();
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = l.accept().await else { return };
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = s.read(&mut buf).await;
                    tokio::time::sleep(wait).await;
                    let r = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
                    let _ = s.write_all(r.as_bytes()).await;
                });
            }
        });
        url
    }

    #[tokio::test]
    async fn of_the_addresses_that_answer_the_first_named_is_taken_even_when_it_is_slower() {
        let (sh, _) = fresh();
        // the address a link was reached by (first) is slower than another way to the same hub
        let (slow, quick) = (fake_hub_after("home", Duration::from_millis(400)).await, fake_hub("home").await);
        let p = sh.call("hc_probe_link_hubs", &json!({ "hubs": [slow, quick], "name": "home" })).await.unwrap();
        assert_eq!((p["result"].clone(), p["url"].clone()), (json!("connected"), json!(slow)), "{p}");
        // when the first named does not answer as that hub, the next one is taken
        let stranger = fake_hub("other-hub").await;
        let p = sh.call("hc_probe_link_hubs", &json!({ "hubs": [stranger, quick], "name": "home" })).await.unwrap();
        assert_eq!((p["result"].clone(), p["url"].clone()), (json!("connected"), json!(quick)), "{p}");
        sh.shutdown();
    }

    #[test]
    fn what_else_a_page_tries_for_a_hub_it_has_no_address_for() {
        let l = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<String>>();
        let named = l(&["http://192.168.1.200:7378", "http://lan-hub.invalid", "http://192.168.1.200:7378/"]);
        let suggested = l(&["https://192.168.1.200:7379", "http://192.168.1.9:7378", "https://192.168.1.200:7378", "http://127.0.0.1:7370"]);
        // each http address over https at the same host and port, then the suggested hubs a page can call
        assert_eq!(
            second_round(&named, &suggested, true),
            l(&["https://192.168.1.200:7378", "https://lan-hub.invalid:7370", "https://192.168.1.200:7379", "http://127.0.0.1:7370"])
        );
        // a link that does not say what its hub is called: no suggested hub could be told from another
        assert_eq!(second_round(&named, &suggested, false), l(&["https://192.168.1.200:7378", "https://lan-hub.invalid:7370"]));
        // an https address has no twin
        assert_eq!(second_round(&l(&["https://pc.example.ts.net:8443"]), &[], true), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_link_that_names_only_http_is_completed_through_a_suggested_hub_of_that_name() {
        let (sh, _) = fresh();
        // the page's server suggests two hubs (a page may call http on its own machine)
        let (lan, other) = (fake_hub("lan-hub").await, fake_hub("other-hub").await);
        sh.call("hc_web_suggested_hubs", &json!({ "addresses": [other, lan] })).await.unwrap();
        // an app's link: its hub by an http address only (.invalid: nothing answers there, over https either)
        let link = json!(["http://lan-hub.invalid:7378"]);
        let p = sh.call("hc_probe_link_hubs", &json!({ "hubs": link, "name": "lan-hub" })).await.unwrap();
        assert_eq!((p["result"].clone(), p["url"].clone(), p["name"].clone()), (json!("connected"), json!(lan), json!("lan-hub")), "{p}");
        // a link to a hub of another name is not sent to a suggested hub: it says it needs https, as before
        let p = sh.call("hc_probe_link_hubs", &json!({ "hubs": link, "name": "home" })).await.unwrap();
        assert_eq!(p, json!({ "result": "invalid", "error": LINK_NEEDS_HTTPS }));
        // nor a link that does not say what its hub is called
        let p = sh.call("hc_probe_link_hubs", &json!({ "hubs": link, "name": null })).await.unwrap();
        assert_eq!(p, json!({ "result": "invalid", "error": LINK_NEEDS_HTTPS }));
        // a server that suggests nothing: as before
        sh.call("hc_web_suggested_hubs", &json!({ "addresses": [] })).await.unwrap();
        let p = sh.call("hc_probe_link_hubs", &json!({ "hubs": link, "name": "lan-hub" })).await.unwrap();
        assert_eq!(p, json!({ "result": "invalid", "error": LINK_NEEDS_HTTPS }));
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_link_is_read_and_only_https_addresses_are_tried() {
        let (sh, _) = fresh();
        let url = LinkUrl { code: link::normalize_code(&link::new_link_code()).unwrap(), hubs: vec!["http://100.64.0.7:7378".into(), "https://pc.example.ts.net:8443".into()], hub_name: Some("home".into()), role: Some("give".into()) }.to_url();
        let p = sh.call("hc_parse_link", &json!({ "input": url })).await.unwrap();
        assert_eq!(p["role"], "give");
        assert_eq!(p["hub_name"], "home");
        assert_eq!(p["hub"], "http://100.64.0.7:7378");
        assert_eq!(p["hubs"], json!(["http://100.64.0.7:7378", "https://pc.example.ts.net:8443"]));
        assert_eq!(p["code"].as_str().unwrap().len(), 19, "{p}");
        // a link whose hub answers at no https address says so (`.invalid`: over https at the same address neither)
        let none = sh.call("hc_probe_link_hubs", &json!({ "hubs": ["http://tailnet-pc.invalid:7378", "http://pc.invalid:7378"], "name": "home" })).await.unwrap();
        assert_eq!(none, json!({ "result": "invalid", "error": LINK_NEEDS_HTTPS }));
        assert_eq!(sh.call("hc_probe_link_hubs", &json!({ "hubs": [], "name": null })).await.unwrap()["error"], "the link names no hub");
        // with an https address among them, that one is tried (nothing answers at .invalid)
        let tried = sh.call("hc_probe_link_hubs", &json!({ "hubs": ["http://tailnet-pc.invalid:7378", "https://hub.invalid"], "name": null })).await.unwrap();
        assert_eq!((tried["result"].clone(), tried["url"].clone()), (json!("unreachable"), json!("https://hub.invalid")), "{tried}");
        // and a link is not started through an address a page cannot call
        let e = sh.call("hc_link_start", &json!({ "hub": "http://100.64.0.7:7378", "deviceName": "", "code": null, "aliases": null })).await.unwrap_err();
        assert_eq!(e, NEEDS_HTTPS);
        sh.shutdown();
    }

    #[tokio::test]
    async fn what_a_link_brings_waits_for_the_review_and_is_taken_on_confirm() {
        let (sh, seen) = fresh();
        let b = bundle("maya", &["http://localhost:7370", "https://hub.invalid"]);
        let address = b.identity().unwrap().address();
        let reception = link::Reception::new(HubClient::new(HubAddress::parse("https://hub.invalid").unwrap()), CancelFlag::default());
        sh.receive(b, "https://hub.invalid", &[], &reception).await;
        let ev = events(&seen);
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert_eq!((ev[0]["state"].clone(), ev[0]["from"].clone(), ev[0]["to"].clone(), ev[0]["name"].clone()), (json!("review"), Value::Null, json!(address), json!("Maya R")));
        // every row names an address a page can call, or says why it cannot
        for row in ev[0]["hubs"].as_array().unwrap() {
            let a = row["address"].as_str().unwrap();
            assert!(page_can_reach(a) || row["error"] == NEEDS_HTTPS, "{row}");
        }
        assert!(ev[0]["hubs"].as_array().unwrap().iter().any(|r| r["address"] == "https://hub.invalid"), "{}", ev[0]["hubs"]);
        // nothing is taken before the user confirms
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"], Value::Null);
        // a hub a page cannot call is refused, and the identity still waits
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": ["http://10.0.0.5:7378"] })).await.unwrap_err(), NEEDS_HTTPS);
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": ["https://hub.invalid", " https://hub.invalid/ "] })).await.unwrap(), json!(address));
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!((st["me"]["address"].clone(), st["me"]["name"].clone(), st["me"]["about"].clone(), st["recovery_saved"].clone()), (json!(address), json!("Maya R"), json!("on call"), json!(true)));
        assert_eq!(st["hubs"].as_array().unwrap().iter().map(|h| h["url"].clone()).collect::<Vec<_>>(), vec![json!("https://hub.invalid")]);
        // it was taken once
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": [] })).await.unwrap_err(), HAS_IDENTITY);
        sh.shutdown();

        // dropped: nothing to confirm afterwards
        let (sh, _) = fresh();
        sh.receive(bundle("omar", &["https://hub.invalid"]), "https://hub.invalid", &[], &reception).await;
        sh.call("hc_link_discard", &json!({})).await.unwrap();
        assert_eq!(sh.call("hc_link_confirm", &json!({ "hubs": [] })).await.unwrap_err(), "nothing to confirm: link again");
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_browser_with_an_identity_does_not_switch_through_a_link() {
        let (sh, seen) = fresh();
        let mine = take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        assert_eq!(sh.call("hc_link_start", &json!({ "hub": "https://hub.invalid", "deviceName": "", "code": null, "aliases": null })).await.unwrap_err(), HAS_IDENTITY);
        let reception = link::Reception::new(HubClient::new(HubAddress::parse("https://hub.invalid").unwrap()), CancelFlag::default());
        sh.receive(bundle("omar", &["https://hub.invalid"]), "https://hub.invalid", &[], &reception).await;
        assert_eq!(events(&seen), vec![json!({ "state": "failed", "error": HAS_IDENTITY })]);
        assert_eq!(sh.call("hc_restore_qr", &json!({ "text": link::to_qr(&bundle("omar", &[])) })).await.unwrap_err(), HAS_IDENTITY);
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"]["address"], json!(mine));
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_key_qr_and_a_key_file_bring_the_identity_with_its_https_hubs() {
        let b = bundle("maya", &["http://192.168.1.20:7370", "https://hub.invalid"]);
        let address = b.identity().unwrap().address();

        let (sh, _) = fresh();
        assert!(sh.call("hc_restore_qr", &json!({ "text": "hubchat-key:nonsense" })).await.is_err());
        assert_eq!(take(&sh, "hc_restore_qr", json!({ "text": link::to_qr(&b) })).await, address);
        let st = sh.call("hc_state", &json!({})).await.unwrap();
        assert_eq!((st["me"]["name"].clone(), st["recovery_saved"].clone()), (json!("Maya R"), json!(true)));
        assert_eq!(st["hubs"].as_array().unwrap().iter().map(|h| h["url"].clone()).collect::<Vec<_>>(), vec![json!("https://hub.invalid")]);
        sh.shutdown();

        let (sh, _) = fresh();
        let file = link::to_key_file(&b, "correct horse").unwrap();
        assert!(sh.call("hc_key_file_import", &json!({ "text": file, "passphrase": "wrong" })).await.is_err());
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"], Value::Null);
        assert_eq!(take(&sh, "hc_key_file_import", json!({ "text": file, "passphrase": "correct horse" })).await, address);
        assert_eq!(sh.call("hc_state", &json!({})).await.unwrap()["me"]["address"], json!(address));
        sh.shutdown();
    }

    #[tokio::test]
    async fn a_signed_in_browser_offers_a_link_and_gives_its_key() {
        let (sh, _) = fresh();
        let mine = take(&sh, "hc_create_identity", json!({ "id": "maya", "name": "Maya" })).await;
        // no hub yet: a new device links through one
        assert_eq!(sh.call("hc_link_offer", &json!({ "hub": null })).await.unwrap_err(), "add a hub first: the new device links through one");
        sh.call("hc_add_hub", &json!({ "input": "https://hub.invalid" })).await.unwrap();
        let offer = sh.call("hc_link_offer", &json!({ "hub": null })).await.unwrap();
        assert_eq!((offer["hub"].clone(), offer["hubs"].clone(), offer["phone_access_off"].clone()), (json!("https://hub.invalid"), json!(["https://hub.invalid"]), json!(false)));
        let l = LinkUrl::parse(offer["qr"].as_str().unwrap()).unwrap();
        assert_eq!((l.role.as_deref(), l.hubs.clone(), link::format_code(&l.code)), (Some("give"), vec!["https://hub.invalid".to_string()], offer["code"].as_str().unwrap().to_string()));
        // the device that scans it waits under the code's address; nobody is there yet
        let look = sh.call("hc_link_lookup", &json!({ "input": offer["qr"] })).await.unwrap();
        assert_eq!((look["address"].clone(), look["device_name"].clone(), look["unknown_hub"].clone()), (json!(link::link_identity(&l.code).unwrap().address()), Value::Null, Value::Null));
        sh.call("hc_link_forget", &json!({ "code": offer["code"] })).await.unwrap();
        // the key as a QR and as a passphrase-locked file: both give this identity back
        let qr = sh.call("hc_key_qr", &json!({})).await.unwrap();
        assert_eq!(link::from_qr(qr.as_str().unwrap()).unwrap().identity().unwrap().address(), mine);
        let file = sh.call("hc_key_file_text", &json!({ "passphrase": "correct horse" })).await.unwrap();
        assert_eq!(file["name"], KEY_FILE_NAME);
        let back = link::from_key_file(file["text"].as_str().unwrap(), "correct horse").unwrap();
        assert_eq!((back.identity().unwrap().address(), back.hubs), (mine, vec!["https://hub.invalid".to_string()]));
        assert!(link::from_key_file(file["text"].as_str().unwrap(), "wrong").is_err());
        sh.shutdown();
    }
}
