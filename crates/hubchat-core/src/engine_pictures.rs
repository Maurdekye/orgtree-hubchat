//! Optional picture synchronization shares the existing connected hub sessions.
//! No message/transfer dependency and no standalone saved-hub publisher.
use super::*;
use crate::pictures::PictureRecord;

impl Engine {
    /// Shared synchronous roster admission for poll, sync and setup refresh.
    /// `snapshot` preserves poll's empty-snapshot and sync's nonempty-first-page rules.
    pub(crate) fn admit_roster_batch(&self, client:&HubClient, entries:&[crate::hub::RosterEntry], snapshot:bool, removed:&[String], online:Option<&[String]>)->Result<()> {
        // Hold the originating generation for ALL namespace-changing effects.
        // No await or runtime-map acquisition is allowed in this closure.
        client.session_commit(|| {
        if self.stopped.load(std::sync::atomic::Ordering::Relaxed) {return Err(Error::Cancelled);}
        let url=client.address().as_str();
        let mut changed=false;
        if snapshot {self.store.set_roster(url,entries)?;changed=true;}
        else if !entries.is_empty() {self.store.upsert_roster(url,entries)?;changed=true;}
        if !removed.is_empty() {self.store.remove_roster(url,removed)?;changed=true;}
        self.observe_pictures(entries)?;
        self.picture_changed.send_replace(());
        if let Some(online)=online {self.store.set_online(url,online)?;changed=true;}
        if changed {self.host.event(Event::Directory);}
        Ok(())
        }).ok_or(Error::FilePaused)?
    }
    #[cfg(test)] // Earlier narrow-helper proof, separate from production batch proof.
    pub(crate) fn observe_pictures_for(&self, client:&HubClient, entries:&[crate::hub::RosterEntry])->Result<()> {
        client.session_commit(|| {
            self.observe_pictures(entries)?;
            self.picture_changed.send_replace(());
            Ok(())
        }).ok_or(Error::FilePaused)?
    }

    /// A completed response may race disable before its local effects. Keep
    /// the short store/event publication in the originating generation guard.
    fn admit_picture(&self, client:&HubClient, peer:&str, record:&PictureRecord, ack:Option<&str>)->Result<()> {
        client.session_commit(|| {
            if self.stopped.load(std::sync::atomic::Ordering::Relaxed) { return Err(Error::Cancelled); }
            let owner=self.me.address();
            if self.store.merge_picture(&owner,peer,record)? {
                self.host.event(Event::Directory);
                self.picture_changed.send_replace(());
            }
            if let Some(key)=ack { self.store.picture_ack(&owner,key,&record.meta.revision)?; }
            Ok(())
        }).ok_or(Error::FilePaused)?
    }
    pub(crate) fn observe_pictures(&self, entries:&[crate::hub::RosterEntry])->Result<()> {
        let owner=self.me.address();
        let mut changed=false;
        for e in entries {
            if matches!(e.kind.as_str(),"person"|"org") {
                if let Some(p)=&e.picture {
                    let mut p=p.clone();
                    p.picture_owner=if e.slug==owner {None} else {e.picture_owner.clone()};
                    changed|=self.store.observe_picture(&owner,&e.slug,&p)?;
                }
            }
        }
        if changed { self.host.event(Event::Directory); self.picture_changed.send_replace(()); }
        Ok(())
    }
    /// Read before publishing on reconnect: learn any newer removal first.
    async fn reconcile_picture(&self, client:&HubClient, initial:bool)->Result<()> {
        if !matches!(self.profile().kind.as_str(),"person"|"org") { return Ok(()); }
        let owner=self.me.address();
        let ack_key=crate::store::picture_ack_key(client.address().as_str());
        if initial || (self.store.picture_meta(&owner)?.is_some() && self.store.picture(&owner)?.is_none()) {
            if let Some(remote)=client.picture(&self.me,&owner).await? {
                self.admit_picture(client,&owner,&remote,None)?;
            }
        }
        let Some(local)=self.store.picture(&owner)? else { return Ok(()) };
        if !initial && self.store.meta(&ack_key)?.as_deref()==Some(&local.meta.revision) { return Ok(()); }
        let winning=client.set_picture(&self.me,&local).await?;
        self.admit_picture(client,&owner,&winning,Some(&ack_key))?;
        Ok(())
    }
    pub(crate) async fn picture_session(&self, client:&HubClient) {
        let mut changes=self.picture_changed.subscribe();
        let mut initial=true;
        loop {
            let failed=self.reconcile_picture(client,initial).await.is_err();
            initial=false;
            // Retry on an existing poll/update, never a new automatic cadence.
            if changes.changed().await.is_err() { return; }
            if failed { tokio::time::sleep(Duration::from_secs(8)).await; }
        }
    }
    pub fn edit_picture(&self, expected_owner:&str, normalized:Option<(Vec<u8>,u32)>)->Result<PictureRecord> {
        if self.stopped.load(std::sync::atomic::Ordering::Relaxed) || expected_owner!=self.me.address() || !matches!(self.profile().kind.as_str(),"person"|"org") {
            return Err(Error::Invalid("Profile identity changed or does not support pictures".into()));
        }
        let record=self.store.edit_picture(expected_owner,normalized)?;
        self.host.event(Event::Directory);
        self.picture_changed.send_replace(());
        Ok(record)
    }
    /// On-demand bytes from active capable hubs, checked against latest metadata.
    pub async fn picture(&self, peer:&str)->Result<Option<PictureRecord>> {
        if self.stopped.load(std::sync::atomic::Ordering::Relaxed) { return Err(Error::Cancelled); }
        let owner=self.me.address();
        if peer!=owner && !self.store.picture_eligible(peer)? { return Ok(None); }
        if let Some(p)=self.store.picture(peer)? { return Ok(Some(p)); }
        let hubs:Vec<HubClient>=self.hubs.lock().unwrap().values()
            .filter(|h|h.status.state==HubState::Connected && h.status.features.iter().any(|f|f=="profile_pictures"))
            .map(|h|h.client.clone()).collect();
        let reaching=self.store.hubs_reaching(peer)?;
        for client in hubs {
            if peer!=owner && !reaching.iter().any(|(url,_)|url==client.address().as_str()) { continue; }
            if let Ok(Some(remote))=client.picture(&self.me,peer).await {
                self.admit_picture(&client,peer,&remote,None)?;
                if let Some(p)=self.store.picture(peer)? { return Ok(Some(p)); }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    use crate::pictures::{next_revision,from_png};
    struct HostStub;
    impl Host for HostStub {
        fn event(&self,_:Event) {}
        fn open_source(&self,_:&str)->std::io::Result<std::fs::File> { Err(std::io::ErrorKind::NotFound.into()) }
        fn download_dir(&self)->PathBuf { PathBuf::new() }
    }
    fn engine()->Arc<Engine> {
        let s=Arc::new(Store::open_in_memory().unwrap());let me=Identity::generate("picture-proof").unwrap();
        s.set_meta("identity.address",&me.address()).unwrap();
        Engine::new(s,me,Profile{kind:"person".into(),..Profile::default()},Arc::new(HostStub))
    }
    fn image()->PictureRecord {
        let mut bytes=Vec::new();
        {let mut p=png::Encoder::new(&mut bytes,1,1);p.set_color(png::ColorType::Rgba);p.set_depth(png::BitDepth::Eight);p.write_header().unwrap().write_image_data(&[1,2,3,255]).unwrap();}
        from_png(next_revision(None).unwrap(),bytes,1).unwrap()
    }
    fn attach(e:&Engine, base:HubClient)->HubClient {
        let stop=CancelFlag::default();let client=base.bind_stop(stop.clone());let url=client.address().to_string();
        e.store.add_hub(&url,"picture-proof").unwrap();
        e.hubs.lock().unwrap().insert(url.clone(),HubRuntime {
            client:client.clone(),stop,retry_now:Arc::new(Notify::new()),kick:Arc::new(Notify::new()),
            status:HubStatus {url,name:"picture-proof".into(),state:HubState::Connected,error:None,retry_at_ms:None,
                max_attachment_bytes:1_000_000,features:vec!["profile_pictures".into()],version:None,
                answered_ms:None,waiting_since_ms:None,clock_offset_ms:None},
        });
        client
    }

    #[tokio::test]
    async fn picture_disabled_clones_refuse_get_post_and_reenabled_generation_works() {
        let e=engine();e.set_suspended(true);let record=image();
        let (base,proof)=hub(vec![serde_json::json!({"picture":record}),serde_json::json!({"picture":record})]).await;
        let client=attach(&e,base);let url=client.address().to_string();
        e.set_hub_enabled(&url,false).unwrap();
        assert!(matches!(client.picture(&e.me,&e.me.address()).await,Err(Error::FilePaused)));
        assert!(matches!(client.set_picture(&e.me,&record).await,Err(Error::FilePaused)));
        e.set_hub_enabled(&url,true).unwrap();
        let fresh=e.hub_client(&url).unwrap();
        assert!(matches!(client.picture(&e.me,&e.me.address()).await,Err(Error::FilePaused)));
        assert_eq!(fresh.picture(&e.me,&e.me.address()).await.unwrap(),Some(record.clone()));
        assert_eq!(fresh.set_picture(&e.me,&record).await.unwrap(),record);
        let requests=proof.await.unwrap();assert_eq!(requests.len(),2);assert_eq!(requests[0].0,"GET");assert_eq!(requests[1].0,"POST");
        e.shutdown();
    }

    async fn stalled_picture_request(post:bool) {
        let e=engine();let record=image();let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr=HubAddress::parse(&format!("http://{}",listener.local_addr().unwrap())).unwrap();
        let client=attach(&e,HubClient::new(addr));let url=client.address().to_string();
        let arrived=Arc::new(Notify::new());let reached=arrived.clone();
        let server=tokio::spawn(async move {
            let (mut socket,_)=listener.accept().await.unwrap();let mut buffer=[0;8192];let mut bytes=Vec::new();
            let split=loop {let n=socket.read(&mut buffer).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);if let Some(at)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {break at+4;}};
            let headers=String::from_utf8_lossy(&bytes[..split]).to_string();
            assert!(headers.starts_with(if post {"POST "} else {"GET "}));
            let len=headers.lines().find_map(|l|l.to_ascii_lowercase().strip_prefix("content-length:").map(|n|n.trim().parse::<usize>().unwrap())).unwrap_or(0);
            while bytes.len()<split+len {let n=socket.read(&mut buffer).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);}
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 200\r\n\r\n{").await.unwrap();
            reached.notify_one();std::future::pending::<()>().await;
        });
        let request={let e=e.clone();tokio::spawn(async move {if post {client.set_picture(&e.me,&record).await.map(Some)} else {client.picture(&e.me,&e.me.address()).await}})};
        tokio::time::timeout(Duration::from_secs(2),arrived.notified()).await.unwrap();
        e.set_hub_enabled(&url,false).unwrap();
        let result=tokio::time::timeout(Duration::from_secs(1),request).await.unwrap().unwrap();
        assert!(matches!(result,Err(Error::FilePaused)));
        assert!(e.store.picture(&e.me.address()).unwrap().is_none());
        server.abort();let _=server.await;e.shutdown();
    }
    #[tokio::test]
    async fn picture_get_body_is_cancelled_after_disabled_runtime() {stalled_picture_request(false).await;}
    #[tokio::test]
    async fn picture_post_ack_body_is_cancelled_after_disabled_runtime() {stalled_picture_request(true).await;}

    #[tokio::test]
    async fn picture_completed_old_generation_cannot_publish_pixels_ack_or_roster() {
        let e=engine();let owner=e.me.address();let record=image();
        let base=HubClient::new(HubAddress::parse("http://127.0.0.1:7996").unwrap());
        let old=attach(&e,base.clone());let url=base.address().to_string();
        e.set_hub_enabled(&url,false).unwrap();
        let ack="picture.ack.proof";
        assert!(matches!(e.admit_picture(&old,&owner,&record,Some(ack)),Err(Error::FilePaused)));
        let entry:crate::hub::RosterEntry=serde_json::from_value(serde_json::json!({"slug":"peer.aaaaaa","kind":"person","picture":record.meta})).unwrap();
        assert!(matches!(e.observe_pictures_for(&old,&[entry]),Err(Error::FilePaused)));
        assert!(e.store.picture(&owner).unwrap().is_none());assert!(e.store.meta(ack).unwrap().is_none());
        assert!(e.store.picture_meta("peer.aaaaaa").unwrap().is_none());
        let fresh=attach(&e,base);
        e.admit_picture(&fresh,&owner,&record,Some(ack)).unwrap();
        assert_eq!(e.store.picture(&owner).unwrap(),Some(record.clone()));assert_eq!(e.store.meta(ack).unwrap(),Some(record.meta.revision));
        e.shutdown();
    }
    async fn hub(answers:Vec<serde_json::Value>)->(HubClient,tokio::task::JoinHandle<Vec<(String,serde_json::Value)>>) {
        let l=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client=HubClient::new(HubAddress::parse(&format!("http://{}",l.local_addr().unwrap())).unwrap());
        let handle=tokio::spawn(async move {
            let mut requests=Vec::new();
            for answer in answers {
                let (mut socket,_)=tokio::time::timeout(Duration::from_secs(5),l.accept()).await.unwrap().unwrap();
                let mut bytes=Vec::new();let mut buf=[0;8192];
                let split=loop {let n=socket.read(&mut buf).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);
                    assert!(bytes.len()<crate::pictures::MAX_WIRE_BYTES+8192);
                    if let Some(at)=bytes.windows(4).position(|s|s==b"\r\n\r\n") {break at+4;}
                };
                let headers=String::from_utf8_lossy(&bytes[..split]).to_string();
                let len=headers.lines().find_map(|l|l.to_ascii_lowercase().strip_prefix("content-length:").map(|n|n.trim().parse::<usize>().unwrap())).unwrap_or(0);
                while bytes.len()<split+len {let n=socket.read(&mut buf).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);}
                let body=if len>0 {serde_json::from_slice(&bytes[split..split+len]).unwrap()} else {serde_json::Value::Null};
                requests.push((headers.lines().next().unwrap().split_whitespace().next().unwrap().to_owned(),body));
                let data=answer.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",data.len(),data).as_bytes()).await.unwrap();
            }
            requests
        });
        (client,handle)
    }
    #[tokio::test]
    async fn picture_http_owner_must_match_current_roster_before_caching() {
        let e=engine();let record=image();let a="a".repeat(64);let b="b".repeat(64);
        let (base,proof)=hub(vec![serde_json::json!({"picture":record,"picture_owner":a}),serde_json::json!({"picture":record,"picture_owner":b})]).await;
        let client=attach(&e,base);let url=client.address().to_string();
        let entry=serde_json::from_value(serde_json::json!({"slug":"peer.aaaaaa","kind":"person","picture_owner":b})).unwrap();
        e.store.set_roster(&url,&[entry]).unwrap();
        assert!(e.picture("peer.aaaaaa").await.unwrap().is_none());
        assert!(e.store.picture("peer.aaaaaa").unwrap().is_none());
        let received=e.picture("peer.aaaaaa").await.unwrap().unwrap();
        assert_eq!(received.meta.picture_owner.as_deref(),Some(b.as_str()));
        assert_eq!(proof.await.unwrap().len(),2);e.shutdown();
    }
    // Deterministic native admission schedule: the old worker passes its early
    // check, pauses, and resumes the exact production batch boundary only after
    // disable/re-enable and fresh ownership admission. No simulated SQL guard.
    async fn picture_roster_preemption(mode:u8) {
        let e=engine();e.set_suspended(true);let a="a".repeat(64);let b="b".repeat(64);let peer="peer.aaaaaa";
        let old=attach(&e,HubClient::new(HubAddress::parse("http://127.0.0.1:7997").unwrap()));
        let url=old.address().to_string();
        let entry=|token:&str,name:&str,picture:Option<crate::pictures::PictureMeta>| -> crate::hub::RosterEntry {
            serde_json::from_value(serde_json::json!({"slug":peer,"kind":"person","org_name":name,"picture_owner":token,"picture":picture})).unwrap()
        };
        let mut pixels=image();pixels.meta.picture_owner=Some(a.clone());
        let old_entry=entry(&a,"old owner",Some(pixels.meta.clone()));
        e.admit_roster_batch(&old,&[old_entry.clone()],true,&[],None).unwrap();
        e.admit_picture(&old,peer,&pixels,None).unwrap();
        assert!(e.store.picture(peer).unwrap().unwrap().data.is_some());
        let (arrived_tx,arrived_rx)=std::sync::mpsc::channel();let (resume_tx,resume_rx)=std::sync::mpsc::channel();
        let worker={let e=e.clone();std::thread::spawn(move || {
            assert!(!old.session_cancelled());arrived_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            match mode {
                0|1=>e.admit_roster_batch(&old,&[old_entry],true,&[],None),
                2=>e.admit_roster_batch(&old,&[old_entry],false,&[],None),
                _=>e.admit_roster_batch(&old,&[],false,&[peer.into()],Some(&[])),
            }
        })};
        arrived_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        e.set_hub_enabled(&url,false).unwrap();e.set_hub_enabled(&url,true).unwrap();
        let fresh=e.hub_client(&url).unwrap();
        let removed=PictureRecord::removed(next_revision(None).unwrap());
        let fresh_entry=entry(&b,"fresh owner",Some(removed.meta.clone()));
        e.admit_roster_batch(&fresh,&[fresh_entry.clone()],true,&[],None).unwrap();
        assert!(e.store.picture(peer).unwrap().unwrap().data.is_none());
        resume_tx.send(()).unwrap();let result=worker.join().unwrap();
        assert!(matches!(result,Err(Error::FilePaused)));
        let rows=e.store.directory().unwrap();assert_eq!(rows.len(),1,"old removal escaped cancellation");
        assert_eq!(rows[0].org_name,"fresh owner","old roster replaced fresh provenance");
        assert_eq!(e.store.picture(peer).unwrap().unwrap().meta.picture_owner.as_deref(),Some(b.as_str()));
        assert!(e.store.picture(peer).unwrap().unwrap().data.is_none());
        // Positive control through the same mode: a fresh generation still works.
        if mode==3 {e.admit_roster_batch(&fresh,&[],false,&[peer.into()],Some(&[])).unwrap();assert!(e.store.directory().unwrap().is_empty());}
        else {let mut updated=fresh_entry;updated.org_name="fresh update".into();e.admit_roster_batch(&fresh,&[updated],mode<2,&[],None).unwrap();assert_eq!(e.store.directory().unwrap()[0].org_name,"fresh update");}
        e.shutdown();
    }
    #[tokio::test]
    async fn picture_poll_batch_after_early_check_rejects_cancelled_generation() {picture_roster_preemption(0).await;}
    #[tokio::test]
    async fn picture_sync_snapshot_after_early_check_rejects_cancelled_generation() {picture_roster_preemption(1).await;}
    #[tokio::test]
    async fn picture_sync_upsert_after_early_check_rejects_cancelled_generation() {picture_roster_preemption(2).await;}
    #[tokio::test]
    async fn picture_sync_remove_after_early_check_rejects_cancelled_generation() {picture_roster_preemption(3).await;}
    #[tokio::test]
    async fn picture_reconnect_learns_removal_before_publish_and_relays_it_to_stale_hub() {
        let e=engine();let old=image();let owner=e.me.address();e.store.merge_picture(&owner,&owner,&old).unwrap();
        let removal=PictureRecord::removed(next_revision(Some(&old.meta)).unwrap());
        let (a,proof)=hub(vec![serde_json::json!({"picture":removal}),serde_json::json!({"picture":removal})]).await;
        e.reconcile_picture(&a,true).await.unwrap();
        let requests=proof.await.unwrap();assert_eq!(requests[0].0,"GET");assert_eq!(requests[1].0,"POST");
        assert_eq!(requests[1].1["picture"],serde_json::to_value(&removal).unwrap());
        let (b,proof)=hub(vec![serde_json::json!({"picture":old}),serde_json::json!({"picture":removal})]).await;
        e.reconcile_picture(&b,true).await.unwrap();
        assert_eq!(proof.await.unwrap()[1].1["picture"],serde_json::to_value(&removal).unwrap());
        assert_eq!(e.store.picture(&owner).unwrap(),Some(removal));
        // The mock listener is closed: this succeeds only if the ack avoids IO.
        e.reconcile_picture(&b,false).await.unwrap();
    }
    #[tokio::test]
    async fn picture_linked_device_fetches_without_minting_and_chat_metadata_is_ignored() {
        let e=engine();let owner=e.me.address();let image=image();
        let (h,proof)=hub(vec![serde_json::json!({"picture":image}),serde_json::json!({"picture":image})]).await;
        e.reconcile_picture(&h,true).await.unwrap();
        assert_eq!(proof.await.unwrap()[1].1["picture"]["revision"],image.meta.revision);
        assert_eq!(e.store.picture(&owner).unwrap(),Some(image.clone()));
        let entry:crate::hub::RosterEntry=serde_json::from_value(serde_json::json!({"slug":"direct.aaaaaa","kind":"chat","picture":image.meta})).unwrap();
        e.observe_pictures(&[entry]).unwrap();assert!(e.store.picture_meta("direct.aaaaaa").unwrap().is_none());
        e.stopped.store(true,std::sync::atomic::Ordering::Relaxed);assert!(e.edit_picture(&owner,None).is_err());
    }
}
