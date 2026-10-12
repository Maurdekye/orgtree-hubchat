//! Optional picture synchronization shares the existing connected hub sessions.
//! No message/transfer dependency and no standalone saved-hub publisher.
use super::*;
use crate::pictures::PictureRecord;

impl Engine {
    pub(crate) fn observe_pictures(&self, entries:&[crate::hub::RosterEntry])->Result<()> {
        let owner=self.me.address();
        let mut changed=false;
        for e in entries {
            if matches!(e.kind.as_str(),"person"|"org") {
                if let Some(p)=&e.picture { changed|=self.store.observe_picture(&owner,&e.slug,p)?; }
            }
        }
        if changed { self.host.event(Event::Directory); self.picture_changed.send_replace(()); }
        Ok(())
    }
    /// Read before publishing on reconnect: learn any newer removal first.
    async fn reconcile_picture(&self, client:&HubClient, initial:bool)->Result<()> {
        if !matches!(self.profile().kind.as_str(),"person"|"org") { return Ok(()); }
        let owner=self.me.address();
        let ack_key=format!("picture.ack.{}",client.address());
        if initial || (self.store.picture_meta(&owner)?.is_some() && self.store.picture(&owner)?.is_none()) {
            if let Some(remote)=client.picture(&self.me,&owner).await? {
                if self.store.merge_picture(&owner,&owner,&remote)? { self.host.event(Event::Directory); self.picture_changed.send_replace(()); }
            }
        }
        let Some(local)=self.store.picture(&owner)? else { return Ok(()) };
        if !initial && self.store.meta(&ack_key)?.as_deref()==Some(&local.meta.revision) { return Ok(()); }
        let winning=client.set_picture(&self.me,&local).await?;
        if self.store.merge_picture(&owner,&owner,&winning)? { self.host.event(Event::Directory); self.picture_changed.send_replace(()); }
        self.store.picture_ack(&owner,&ack_key,&winning.meta.revision)?;
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
                self.store.merge_picture(&owner,peer,&remote)?;
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
