//! Picture metadata is durable; only the bounded peer byte cache is evictable.
//! Existing meta storage leaves reserved SQLite migration16 untouched.
use super::*;
use crate::pictures::{PictureMeta, PictureRecord, next_revision};

fn key(part: &str, peer: &str) -> String { format!("picture.{part}.{peer}") }
fn read_meta(c: &Connection, peer: &str) -> rusqlite::Result<Option<PictureMeta>> {
    let raw: Option<String> = c.query_row("SELECT value FROM meta WHERE key=?",[key("meta",peer)],|r|r.get(0)).optional()?;
    Ok(raw.and_then(|s|serde_json::from_str::<PictureMeta>(&s).ok()).filter(PictureMeta::valid))
}
fn save_meta(c: &Connection, peer: &str, incoming: &PictureMeta) -> rusqlite::Result<bool> {
    if read_meta(c,peer)?.is_some_and(|old|old.revision>=incoming.revision) { return Ok(false); }
    c.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key("meta",peer),serde_json::to_string(incoming).unwrap()])?;
    c.execute("DELETE FROM meta WHERE key=?",[key("data",peer)])?;
    Ok(true)
}
fn save_data(c: &Connection, peer: &str, record: &PictureRecord, owner: &str) -> rusqlite::Result<()> {
    if let Some(data)=&record.data {
        c.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key("data",peer),data])?;
        // Stable FIFO replacement is sufficient here. Metadata is never evicted:
        // an old delayed fetch must not resurrect after its byte entry is gone.
        c.execute("DELETE FROM meta WHERE key IN (SELECT key FROM meta WHERE key LIKE 'picture.data.%' AND key!=? ORDER BY rowid DESC LIMIT -1 OFFSET 80)",[key("data",owner)])?;
    }
    Ok(())
}
fn owner_matches(c: &Connection, owner: &str) -> rusqlite::Result<bool> {
    Ok(c.query_row("SELECT value FROM meta WHERE key='identity.address'",[],|r|r.get::<_,String>(0)).optional()?.as_deref()==Some(owner))
}

impl Store {
    pub fn picture_eligible(&self, peer:&str)->Result<bool> {
        self.with(|c|c.query_row("SELECT EXISTS(SELECT 1 FROM roster WHERE address=? AND kind IN ('person','org'))",[peer],|r|r.get(0)))
    }
    pub fn picture_ack(&self,owner:&str,key:&str,revision:&str)->Result<()> {
        self.with(|c| { let tx=c.transaction()?;
            if owner_matches(&tx,owner)? { tx.execute("INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[key,revision])?; }
            tx.commit()
        })
    }
    pub fn picture_meta(&self, peer: &str) -> Result<Option<PictureMeta>> { self.with(|c|read_meta(c,peer)) }

    /// Missing data means absent/evicted, never a synthetic removal.
    pub fn picture(&self, peer: &str) -> Result<Option<PictureRecord>> {
        self.with(|c| {
            let Some(meta)=read_meta(c,peer)? else { return Ok(None) };
            let data=c.query_row("SELECT value FROM meta WHERE key=?",[key("data",peer)],|r|r.get::<_,String>(0)).optional()?;
            if meta.sha256.is_some() && data.is_none() { return Ok(None); }
            Ok(Some(PictureRecord{meta,data}))
        })
    }

    /// Admit metadata immediately (including removal), before fetching bytes.
    pub fn observe_picture(&self, owner: &str, peer: &str, meta: &PictureMeta) -> Result<bool> {
        if !meta.valid() { return Ok(false); }
        self.with(|c| {
            let tx=c.transaction()?;
            if !owner_matches(&tx,owner)? { return Ok(false); }
            let changed=save_meta(&tx,peer,meta)?;
            tx.commit()?;
            Ok(changed)
        })
    }

    /// Late fetches, parallel hub sessions and identity switches cannot replace
    /// a later revision. Byte verification happens before taking the DB lock.
    pub fn merge_picture(&self, owner: &str, peer: &str, record: &PictureRecord) -> Result<bool> {
        record.decoded()?;
        self.with(|c| {
            let tx=c.transaction()?;
            if !owner_matches(&tx,owner)? { return Ok(false); }
            if let Some(old)=read_meta(&tx,peer)? {
                if old.revision>record.meta.revision || (old.revision==record.meta.revision && old!=record.meta) { return Ok(false); }
            }
            let changed=save_meta(&tx,peer,&record.meta)?;
            let had_data: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM meta WHERE key=?)",[key("data",peer)],|r|r.get(0))?;
            save_data(&tx,peer,record,owner)?;
            tx.commit()?;
            Ok(changed || (!had_data && record.data.is_some()))
        })
    }

    /// Only explicit user edits mint a revision, atomically above the latest
    /// observed metadata. A departing engine cannot modify an adopted identity.
    pub fn edit_picture(&self, owner: &str, normalized: Option<(Vec<u8>,u32)>) -> Result<PictureRecord> {
        let prepared=normalized.map(|(bytes,size)|crate::pictures::from_png(next_revision(None)?,bytes,size)).transpose()?;
        let mut c=self.con.lock().unwrap_or_else(|p|p.into_inner());
        let tx=c.transaction().map_err(db)?;
        if !owner_matches(&tx,owner).map_err(db)? { return Err(Error::Invalid("Profile identity changed".into())); }
        let revision=next_revision(read_meta(&tx,owner).map_err(db)?.as_ref())?;
        let mut record=prepared.unwrap_or_else(||PictureRecord::removed(revision.clone()));
        record.meta.revision=revision;
        save_meta(&tx,owner,&record.meta).map_err(db)?;
        save_data(&tx,owner,&record,owner).map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pictures::from_png;
    fn image(revision:String)->PictureRecord {
        let mut bytes=Vec::new();
        { let mut enc=png::Encoder::new(&mut bytes,1,1); enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight); enc.write_header().unwrap().write_image_data(&[1,2,3,255]).unwrap(); }
        from_png(revision,bytes,1).unwrap()
    }
    fn store()->Store { let s=Store::open_in_memory().unwrap();s.set_meta("identity.address","me").unwrap();s }
    fn roster(s:&Store,hub:&str,identity:&str) {
        let entry=serde_json::from_value(serde_json::json!({"slug":"peer","kind":"person","org_name":identity,"picture_owner":identity})).unwrap();
        s.upsert_roster(hub,&[entry]).unwrap();
    }
    fn scoped(mut record:PictureRecord,identity:&str)->PictureRecord {
        let mut v=serde_json::to_value(&record).unwrap();v["picture_owner"]=serde_json::json!(identity);
        record=serde_json::from_value(v).unwrap();record
    }
    #[test]
    fn picture_reused_address_does_not_inherit_bytes_or_high_water_or_late_response() {
        let s=store();let a="a".repeat(64);let b="b".repeat(64);
        roster(&s,"hub-a",&a);
        let old=scoped(image(format!("{:020}-{}",99,"1".repeat(32))),&a);
        assert!(s.merge_picture("me","peer",&old).unwrap());
        s.remove_roster("hub-a",&["peer".into()]).unwrap();assert!(s.directory().unwrap().is_empty());
        roster(&s,"hub-a",&b);assert_eq!(s.directory().unwrap()[0].org_name,b);
        assert!(s.picture("peer").unwrap().is_none(),"new owner inherited old pixels");
        assert!(s.directory().unwrap()[0].picture.is_none(),"new owner inherited high-water metadata");
        assert!(!s.merge_picture("me","peer",&old).unwrap(),"late previous owner was admitted");
        let fresh=scoped(image(next_revision(None).unwrap()),&b);
        assert!(s.merge_picture("me","peer",&fresh).unwrap());
        assert_eq!(s.picture("peer").unwrap(),Some(fresh));
    }
    #[test]
    fn picture_same_identity_rejoin_retains_removal_high_water() {
        let s=store();let a="a".repeat(64);roster(&s,"hub-a",&a);
        let old=scoped(image(next_revision(None).unwrap()),&a);
        let gone=scoped(PictureRecord::removed(next_revision(Some(&old.meta)).unwrap()),&a);
        s.merge_picture("me","peer",&old).unwrap();s.observe_picture("me","peer",&gone.meta).unwrap();
        s.remove_roster("hub-a",&["peer".into()]).unwrap();roster(&s,"hub-a",&a);
        assert!(!s.merge_picture("me","peer",&old).unwrap());assert_eq!(s.picture("peer").unwrap(),Some(gone));
    }
    #[test]
    fn picture_conflicting_hub_owners_fall_back_without_combining_versions() {
        let s=store();let a="a".repeat(64);let b="b".repeat(64);roster(&s,"hub-a",&a);
        let old=scoped(image(next_revision(None).unwrap()),&a);s.merge_picture("me","peer",&old).unwrap();
        roster(&s,"hub-b",&b);
        assert!(s.picture("peer").unwrap().is_none());assert!(s.directory().unwrap()[0].picture.is_none());
        assert!(!s.merge_picture("me","peer",&old).unwrap());
        s.remove_roster("hub-b",&["peer".into()]).unwrap();assert_eq!(s.picture("peer").unwrap(),Some(old));
    }
    #[test]
    fn removal_observation_blocks_late_fetch_and_automatic_replay() {
        let s=store(); let a=image(next_revision(None).unwrap());
        assert!(s.merge_picture("me","peer",&a).unwrap());
        let b=PictureRecord::removed(next_revision(Some(&a.meta)).unwrap());
        assert!(s.observe_picture("me","peer",&b.meta).unwrap());
        assert!(!s.merge_picture("me","peer",&a).unwrap());
        assert_eq!(s.picture("peer").unwrap(),Some(b));
    }
    #[test]
    fn explicit_edits_advance_observed_revision_and_identity_wipe_rejects_late_results() {
        let s=store(); let a=image(next_revision(None).unwrap());
        s.observe_picture("me","me",&a.meta).unwrap();
        let b=s.edit_picture("me",None).unwrap(); assert!(b.meta.revision>a.meta.revision);
        assert!(!s.merge_picture("me","me",&a).unwrap());
        s.set_meta("identity.address","new").unwrap();
        assert!(!s.merge_picture("me","peer",&a).unwrap());
        assert!(s.edit_picture("me",None).is_err());
        assert!(s.picture("peer").unwrap().is_none());
    }
    #[test]
    fn equal_revision_conflicts_do_not_replace_metadata_or_pixels() {
        let s=store(); let a=image(next_revision(None).unwrap());
        s.merge_picture("me","peer",&a).unwrap();
        let b=PictureRecord::removed(a.meta.revision.clone());
        assert!(!s.merge_picture("me","peer",&b).unwrap());
        assert_eq!(s.picture("peer").unwrap(),Some(a));
    }
    #[test]
    fn byte_cache_is_bounded_and_keeps_metadata_and_own_picture() {
        let s=store();let a=image(next_revision(None).unwrap());s.merge_picture("me","me",&a).unwrap();
        for n in 0..90 { s.merge_picture("me",&format!("peer{n}"),&a).unwrap(); }
        let bytes:i64=s.with(|c|c.query_row("SELECT COUNT(*) FROM meta WHERE key LIKE 'picture.data.%'",[],|r|r.get(0))).unwrap();
        assert_eq!(bytes,81);assert!(s.picture("me").unwrap().is_some());
        assert!(s.picture("peer0").unwrap().is_none());assert_eq!(s.picture_meta("peer0").unwrap(),Some(a.meta));
    }
}
