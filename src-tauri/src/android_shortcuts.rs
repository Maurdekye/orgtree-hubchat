//! Android's bounded, identity-guarded contact/avatar projection. No WebView needed.
use hubchat_core::store::{Store, ShortcutContact};
use serde::Serialize;

#[cfg(target_os = "android")]
static LAST: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

#[cfg(target_os = "android")]
pub fn theme_changed() {
    *LAST.lock().unwrap() = None;
    changed();
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct ContactIcon {
    peer: String,
    name: String,
    kind: String,
    png: Option<String>,
}

fn project(store: &Store, rows: Vec<ShortcutContact>) -> Result<(Vec<ContactIcon>, Vec<String>), String> {
    let directory = store.directory().map_err(|e| e.to_string())?;
    let mut missing = vec![];
    let icons = rows.into_iter().take(4).map(|row| {
        let kind = directory.iter().find(|c| c.address == row.peer).map(|c| c.kind.as_str()).unwrap_or("person");
        let meta = store.picture_meta(&row.peer).map_err(|e| e.to_string())?;
        let record = store.picture(&row.peer).map_err(|e| e.to_string())?;
        let png = record.filter(|r| Some(&r.meta) == meta.as_ref())
            .filter(|r| r.decoded().ok().flatten().is_some()).and_then(|r| r.data);
        if png.is_none() && meta.as_ref().is_some_and(|m| m.sha256.is_some()) { missing.push(row.peer.clone()); }
        Ok(ContactIcon { peer: row.peer, name: row.name, kind: if kind == "org" { "org" } else { "person" }.into(), png })
    }).collect::<Result<Vec<_>, String>>()?;
    Ok((icons, missing))
}

#[cfg(target_os = "android")]
pub fn changed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static DIRTY: AtomicBool = AtomicBool::new(false);
    DIRTY.store(true, Ordering::Release);
    if RUNNING.swap(true, Ordering::AcqRel) { return; }
    let Ok(c) = crate::core::get() else { RUNNING.store(false, Ordering::Release); return; };
    c.rt.spawn(async move {
        // At most four sequential picture fetches per projection; updates from
        // admitted bytes are coalesced, never awaited while identity is locked.
        let mut fetched = std::collections::HashSet::new();
        loop {
            DIRTY.store(false, Ordering::Release);
            let jobs = c.rt.spawn_blocking(|| {
                c.with_shortcut_data(false, Some(4), |owner, rows| {
                    let (icons, missing) = project(&c.store, rows)?;
                    let json = serde_json::to_string(&icons).map_err(|_| "Shortcut data unavailable")?;
                    let mut last = LAST.lock().unwrap();
                    if last.as_ref() != Some(&(owner.to_owned(), json.clone())) {
                        crate::android::update_shortcuts(owner, &json)?;
                        *last = Some((owner.to_owned(), json));
                    }
                    Ok(missing.into_iter().map(|peer| (owner.to_owned(), peer)).collect::<Vec<_>>())
                })
            }).await;
            if let Ok(Ok(jobs)) = jobs {
                for (owner, peer) in jobs {
                    if fetched.insert((owner.clone(), peer.clone())) {
                        if let Ok(engine) = c.engine() {
                            if engine.me().address() == owner { let _ = engine.picture(&peer).await; }
                        }
                    }
                }
            }
            RUNNING.store(false, Ordering::Release);
            if !DIRTY.swap(false, Ordering::AcqRel) || RUNNING.swap(true, Ordering::AcqRel) { break; }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use hubchat_core::pictures::{from_png, next_revision, PictureRecord};
    fn picture(color: [u8; 4]) -> PictureRecord {
        let mut bytes = vec![];
        { let mut enc = png::Encoder::new(&mut bytes, 1, 1); enc.set_color(png::ColorType::Rgba);
          enc.set_depth(png::BitDepth::Eight); enc.write_header().unwrap().write_image_data(&color).unwrap(); }
        from_png(next_revision(None).unwrap(), bytes, 1).unwrap()
    }
    #[test]
    fn android_shortcut_icons_refresh_picture_removal_and_missing_bytes_without_reordering() {
        let store = Store::open_in_memory().unwrap();
        let owner = "me.123456";
        store.set_meta("identity.address", owner).unwrap();
        let rows = vec![ShortcutContact { peer: owner.into(), name: "Example Person".into() }];
        let (fallback, missing) = project(&store, rows.clone()).unwrap();
        assert!(fallback[0].png.is_none() && missing.is_empty());
        let first = picture([255, 0, 0, 255]);
        store.merge_picture(owner, owner, &first).unwrap();
        let (with_picture, _) = project(&store, rows.clone()).unwrap();
        assert_eq!(with_picture[0].png, first.data);
        assert_ne!(fallback, with_picture);
        let mut second = picture([0, 0, 255, 255]);
        second.meta.revision = next_revision(Some(&first.meta)).unwrap();
        store.merge_picture(owner, owner, &second).unwrap();
        let (changed, _) = project(&store, rows.clone()).unwrap();
        assert_ne!(with_picture, changed);
        let removed = PictureRecord::removed(next_revision(Some(&second.meta)).unwrap());
        store.merge_picture(owner, owner, &removed).unwrap();
        assert_eq!(project(&store, rows.clone()).unwrap().0, fallback);
        let mut newer = picture([0, 255, 0, 255]);
        newer.meta.revision = next_revision(Some(&removed.meta)).unwrap();
        store.observe_picture(owner, owner, &newer.meta).unwrap();
        let (pending, missing) = project(&store, rows).unwrap();
        assert_eq!(pending, fallback);
        assert_eq!(missing, vec![owner]);
    }
    #[test]
    fn android_shortcut_projection_is_bounded_and_name_changes_refresh_fallbacks() {
        let store = Store::open_in_memory().unwrap();
        let rows: Vec<_> = (0..8).map(|i| ShortcutContact { peer: format!("p{i}.123456"), name: format!("Name {i}") }).collect();
        let first = project(&store, rows.clone()).unwrap().0;
        assert_eq!(first.len(), 4);
        assert_eq!(first.iter().map(|r| &r.peer).collect::<Vec<_>>(), rows[..4].iter().map(|r| &r.peer).collect::<Vec<_>>());
        let mut renamed = rows; renamed[0].name = "New Name".into();
        assert_ne!(first, project(&store, renamed).unwrap().0);
        assert!(project(&store, vec![]).unwrap().0.is_empty());
    }
}
