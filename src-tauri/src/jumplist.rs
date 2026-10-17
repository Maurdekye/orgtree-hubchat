//! Windows taskbar menu. Links launch this executable, so single-instance
//! routing reaches the running app without touching protocol registration.
use hubchat_core::store::ShortcutContact;
use hubchat_core::pictures::PictureMeta;
use std::{collections::{HashMap, HashSet, VecDeque}, path::PathBuf, sync::{Mutex, atomic::{AtomicBool, Ordering}}};
use tauri::{AppHandle, Manager};
use windows::{core::{BSTR, HSTRING, Interface, Result}, Win32::{
    Foundation::PROPERTYKEY,
    System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, StructuredStorage::PROPVARIANT},
    UI::Shell::{Common::{IObjectArray, IObjectCollection}, PropertiesSystem::{IPropertyStore, PSCoerceToCanonicalValue},
        DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink},
}};

const TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9), pid: 2,
};

#[derive(Clone, PartialEq)]
struct PictureStamp { peer: String, meta: Option<PictureMeta>, available: bool, avatar: Option<PathBuf>, background: Option<PathBuf> }

#[derive(Default)]
struct Avatars(Mutex<HashMap<(String, String), (String, String, PathBuf, bool)>>);

#[derive(Default)]
struct TaskIcons(Mutex<HashMap<String, PathBuf>>);

pub fn admit_task_icon(app: &AppHandle, view: &str, png: &str) -> std::result::Result<(), String> {
    if !matches!(view, "settings" | "directory" | "newchat") { return Err("Invalid shortcut task".into()); }
    let c = crate::core::get()?;
    let path = crate::jumplist_icons::avatar(&c.dir, png).ok_or("Invalid shortcut icon")?;
    if app.try_state::<TaskIcons>().is_none() { app.manage(TaskIcons::default()); }
    let state = app.state::<TaskIcons>();
    let different = state.0.lock().unwrap().insert(view.to_owned(), path.clone()).as_ref() != Some(&path);
    if different { c.invalidate_shortcuts(); changed(app); }
    Ok(())
}

fn contact_kind(directory: &[hubchat_core::store::Contact], peer: &str) -> String {
    directory.iter().find(|c|c.address == peer).map(|c|c.kind.clone()).filter(|k|k == "org" || k == "chat").unwrap_or_else(||"person".into())
}

fn avatar_eligible(rows: &[ShortcutContact], peer: &str, name: &str, meta: Option<&PictureMeta>) -> bool {
    rows.iter().any(|r|r.peer == peer && r.name == name) && !meta.is_some_and(|m|m.sha256.is_some())
}

pub fn admit_avatar(app: &AppHandle, owner: &str, peer: &str, name: &str, kind: &str, png: &str, background: bool) -> std::result::Result<(), String> {
    let c = crate::core::get()?;
    let path = crate::jumplist_icons::avatar(&c.dir, png).ok_or("Invalid shortcut avatar")?;
    if app.try_state::<Avatars>().is_none() { app.manage(Avatars::default()); }
    c.tray_with_identity(Some(owner), |_| {
        let rows = c.store.shortcut_contacts(true, None).map_err(|e|e.to_string())?;
        let meta = c.store.picture_meta(peer).map_err(|e|e.to_string())?;
        let directory = c.store.directory().map_err(|e|e.to_string())?;
        let eligible = if background { rows.iter().any(|r| r.peer == peer && r.name == name) && meta.as_ref().is_some_and(|m|m.sha256.is_some()) }
            else { avatar_eligible(&rows, peer, name, meta.as_ref()) };
        if !eligible || contact_kind(&directory, peer) != kind {
            return Err("Contact changed".into());
        }
        let state = app.state::<Avatars>();
        let mut icons = state.0.lock().unwrap();
        icons.retain(|(o,p), _| o == owner && rows.iter().any(|r| &r.peer == p));
        icons.insert((owner.to_owned(), peer.to_owned()), (name.to_owned(), kind.to_owned(), path, background));
        Ok(())
    })?;
    changed(app);
    Ok(())
}

#[derive(Default)]
struct FetchQueue { pending: VecDeque<(String, String)>, known: HashSet<(String, String)> }

#[derive(Default)]
struct Pictures {
    last: Mutex<Option<(String, Vec<PictureStamp>)>>,
    fetch: Mutex<FetchQueue>,
    fetching: AtomicBool,
}

pub fn chat_link(owner: &str, peer: &str) -> String {
    let mut url = url::Url::parse("hubchat://chat").expect("constant URL");
    url.query_pairs_mut().append_pair("to", peer).append_pair("owner", owner);
    url.into()
}

fn link(exe: &str, title: &str, target: &str, icon: Option<&std::path::Path>) -> Result<IShellLinkW> {
    // Only our URL builder supplies target: quoting cannot become another argument.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe))?;
        link.SetArguments(&HSTRING::from(format!("\"{target}\"")))?;
        let icon = icon.map(|path|path.to_string_lossy().into_owned()).unwrap_or_else(||exe.to_owned());
        link.SetIconLocation(&HSTRING::from(icon), 0)?;
        let props: IPropertyStore = link.cast()?;
        let mut title = PROPVARIANT::from(title.replace('\0', "").as_str());
        PSCoerceToCanonicalValue(&TITLE, &mut title)?;
        props.SetValue(&TITLE, &title)?;
        props.Commit()?;
        Ok(link)
    }
}

struct Apartment;
impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }

struct ContactLink { name: String, target: String, icon: Option<std::path::PathBuf> }

/// Both collections are testable without creating or publishing a destination list.
unsafe fn collections(exe: &str, contacts: &[ContactLink], icons: &HashMap<String, PathBuf>) -> Result<(IObjectArray, IObjectArray)> {
    let recent: IObjectCollection = unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)? };
    let tasks: IObjectCollection = unsafe { CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)? };
    for contact in contacts {
        unsafe { recent.AddObject(&link(exe, &contact.name, &contact.target, contact.icon.as_deref())?)?; }
    }
    for (name, view) in [("Settings", "settings"), ("Directory", "directory"), ("New chat", "newchat")] {
        unsafe { tasks.AddObject(&link(exe, name, &format!("hubchat://view?name={view}"), icons.get(view).map(|p|p.as_path()))?)?; }
    }
    Ok((recent.cast()?, tasks.cast()?))
}

fn arguments(link: &IShellLinkW) -> Result<String> {
    let mut args = [0u16; 4096];
    unsafe { link.GetArguments(&mut args)?; }
    Ok(String::from_utf16_lossy(&args[..args.iter().position(|&c|c==0).unwrap_or(args.len())]))
}

fn publish(app: &AppHandle, owner: &str, rows: &[ShortcutContact], pictures: &[PictureStamp], c: &crate::core::Core) -> Result<()> {
    let Ok(exe) = std::env::current_exe() else { return Ok(()); };
    // Shell state belongs to this build's AppUserModelID, including .test builds.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let list: ICustomDestinationList = CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        list.SetAppID(&HSTRING::from(app.config().identifier.as_str()))?;
        let mut slots = 0;
        let removed: IObjectArray = list.BeginList(&mut slots)?;
        let result = (|| {
            // Windows requires a custom category to honor destinations the user removed.
            let mut removed_args = HashSet::new();
            for i in 0..removed.GetCount()? {
                if let Ok(item) = removed.GetAt::<IShellLinkW>(i) {
                    if let Ok(args) = arguments(&item) { removed_args.insert(args); }
                }
            }
            let exe = exe.to_string_lossy();
            let mut contacts = Vec::new();
            for row in rows {
                let target = chat_link(owner, &row.peer);
                if removed_args.contains(&format!("\"{target}\"")) { continue; }
                let stamp = pictures.iter().find(|p|p.peer == row.peer);
                let icon = stamp.and_then(|p|p.meta.as_ref()).and_then(|meta| {
                    let record = c.store.picture(&row.peer).ok().flatten();
                    stamp.and_then(|p|p.background.as_deref()).and_then(|background|
                        crate::jumplist_icons::profile_on(&c.dir, meta, record.as_ref(), background))
                }).or_else(||stamp.and_then(|p|p.avatar.clone()));
                contacts.push(ContactLink { name: row.name.clone(), target, icon });
            }
            let icons = app.try_state::<TaskIcons>().map(|state| {
                let icons = state.0.lock().unwrap().clone();
                icons
            }).unwrap_or_default();
            let (recent, tasks) = collections(&exe, &contacts, &icons)?;
            if recent.GetCount()? > 0 { list.AppendCategory(&HSTRING::from("Recent"), &recent)?; }
            list.AddUserTasks(&tasks)?;
            list.CommitList()
        })();
        if result.is_err() { let _ = list.AbortList(); }
        result
    }
}

pub fn changed(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Ok(c) = crate::core::get() {
            if handle.try_state::<Pictures>().is_none() { handle.manage(Pictures::default()); }
            let state = handle.state::<Pictures>();
            let engine = c.engine().ok();
            let owner = engine.as_ref().map(|e|e.me().address()).unwrap_or_default();
            let read = || -> std::result::Result<Vec<PictureStamp>, String> {
                let directory = c.store.directory().map_err(|e|e.to_string())?;
                c.store.shortcut_contacts(true, None).map_err(|e|e.to_string())?.into_iter().map(|row| {
                    let rendered = handle.try_state::<Avatars>().and_then(|state| {
                        let avatar = state.0.lock().unwrap().get(&(owner.clone(), row.peer.clone()))
                            .filter(|(name,kind,path,_)|name == &row.name && kind == &contact_kind(&directory, &row.peer) && path.is_file())
                            .map(|(_,_,path,background)|(path.clone(), *background));
                        avatar
                    });
                    Ok(PictureStamp {
                        avatar: rendered.as_ref().filter(|(_,bg)| !bg).map(|(p,_)|p.clone()),
                        background: rendered.filter(|(_,bg)| *bg).map(|(p,_)|p),
                        meta: c.store.picture_meta(&row.peer).map_err(|e|e.to_string())?,
                        available: c.store.picture(&row.peer).map_err(|e|e.to_string())?.is_some_and(|r|r.data.is_some()),
                        peer: row.peer,
                    })
                }).collect()
            };
            let pictures = if owner.is_empty() { Ok(vec![]) } else { c.tray_with_identity(Some(&owner), |_|read()) };
            let Ok(pictures) = pictures else { return; };
            let key = (owner.clone(), pictures.clone());
            if state.last.lock().unwrap().as_ref() != Some(&key) { c.invalidate_shortcuts(); }
            let result = c.with_shortcuts(true, None, |current, rows| {
                if current != owner { return Err("Identity changed".into()); }
                publish(&handle, current, &rows, &pictures, c).map_err(|_|"Jump List unavailable".to_owned())
            });
            if result.is_ok() { *state.last.lock().unwrap() = Some(key); }
            // Retrieval runs outside the UI/identity guard and uses the engine's
            // existing admitted picture path. Hub events retry unavailable bytes.
            let missing = pictures.iter().filter(|p| !p.available && p.meta.as_ref().is_some_and(|meta|
                meta.sha256.is_some() && crate::jumplist_icons::cached(&c.dir, meta).is_none()))
                .map(|p|(owner.clone(), p.peer.clone())).collect::<Vec<_>>();
            fetch_missing(&handle, c, missing);
            if result.is_err() { tracing::warn!("could not update taskbar shortcuts"); }
        }
    });
}

fn fetch_missing(app: &AppHandle, c: &'static crate::core::Core, jobs: Vec<(String, String)>) {
    let state = app.state::<Pictures>();
    {
        let mut queue = state.fetch.lock().unwrap();
        for job in jobs { if queue.known.insert(job.clone()) { queue.pending.push_back(job); } }
        if queue.pending.is_empty() || state.fetching.swap(true, Ordering::AcqRel) { return; }
    }
    let app = app.clone();
    c.rt.spawn(async move {
        loop {
            let job = {
                let state = app.state::<Pictures>();
                let mut queue = state.fetch.lock().unwrap();
                match queue.pending.pop_front() {
                    Some(job) => job,
                    None => { state.fetching.store(false, Ordering::Release); return; }
                }
            };
            if let Ok(engine) = c.engine() {
                if engine.me().address() == job.0 { let _ = engine.picture(&job.1).await; }
            }
            app.state::<Pictures>().fetch.lock().unwrap().known.remove(&job);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avatar_admission_rejects_renames_closed_contacts_and_current_profile_pictures() {
        let rows = [ShortcutContact { peer: "p.123456".into(), name: "New Name".into() }];
        assert!(avatar_eligible(&rows, "p.123456", "New Name", None));
        assert!(!avatar_eligible(&rows, "p.123456", "Old Name", None));
        assert!(!avatar_eligible(&[], "p.123456", "New Name", None));
        let record = crate::jumplist_icons::fixture(128, [0, 1, 2, 255]);
        assert!(!avatar_eligible(&rows, "p.123456", "New Name", Some(&record.meta)));
        let removed = hubchat_core::pictures::PictureRecord::removed(hubchat_core::pictures::next_revision(None).unwrap());
        assert!(avatar_eligible(&rows, "p.123456", "New Name", Some(&removed.meta)));
    }

    #[test]
    #[ignore = "requires synthetic WebView PNG from tools/proof-shortcut-avatars.mjs"]
    fn client_rendered_avatar_is_a_real_loadable_shell_icon() {
        use base64::Engine as _;
        let png = std::fs::read(std::env::var("HUBCHAT_AVATAR_PNG").expect("renderer fixture path")).unwrap();
        let data = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png));
        let dir = crate::jumplist_icons::test_dir();
        let icon = crate::jumplist_icons::avatar(&dir, &data).unwrap();
        let pixels = image::open(&icon).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), (128, 128));
        assert_eq!(pixels.get_pixel(0, 0).0[3], 0, "circle has transparent corners");
        assert!(pixels.pixels().any(|p|p.0[3] != 0), "avatar has visible pixels");
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let _apartment = Apartment;
            let item = link(r"C:\scratch\hubchat.exe", "Example Person", &chat_link("me.123456", "p.123456"), Some(&icon)).unwrap();
            let mut path = [0u16;4096]; let mut index = -1;
            item.GetIconLocation(&mut path, &mut index).unwrap();
            assert_eq!(PathBuf::from(String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap()])), icon);
            assert_eq!(index, 0);
            use windows_sys::Win32::UI::WindowsAndMessaging::{LoadImageW, DestroyIcon, IMAGE_ICON, LR_LOADFROMFILE};
            let loaded = LoadImageW(std::ptr::null_mut(), path.as_ptr(), IMAGE_ICON, 32, 32, LR_LOADFROMFILE);
            assert!(!loaded.is_null(), "Windows loads the client-rendered initials ICO");
            assert_ne!(DestroyIcon(loaded), 0);
            let org_png = std::fs::read(fixture_dir.join("org-initials.png")).unwrap();
            let org_icon = crate::jumplist_icons::avatar(&dir, &format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(org_png))).unwrap();
            let org_background = image::open(fixture_dir.join("profile-bg-org.png")).unwrap().to_rgba8();
            let org_pixels = image::open(&org_icon).unwrap().to_rgba8();
            assert_eq!(org_pixels.get_pixel(64,15), org_background.get_pixel(64,15), "initials and picture share the same opaque matte");
            assert_eq!(org_pixels.get_pixel(64,15)[3], 255);
            assert_eq!(org_pixels.get_pixel(0,0)[3], 0);
            let org_item = link(r"C:\scratch\hubchat.exe", "Org initials", &chat_link("me.123456", "org.123456"), Some(&org_icon)).unwrap();
            org_item.GetIconLocation(&mut path, &mut index).unwrap();
            let loaded = LoadImageW(std::ptr::null_mut(), path.as_ptr(), IMAGE_ICON, 32, 32, LR_LOADFROMFILE);
            assert!(!loaded.is_null(), "Windows loads the opaque-matte initials icon");
            assert_ne!(DestroyIcon(loaded), 0);
            let fixture_dir = PathBuf::from(std::env::var("HUBCHAT_AVATAR_PNG").unwrap()).parent().unwrap().to_path_buf();
            let mut icons = HashMap::new();
            for view in ["settings", "directory", "newchat"] {
                let png = std::fs::read(fixture_dir.join(format!("{view}.png"))).unwrap();
                let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png));
                icons.insert(view.to_owned(), crate::jumplist_icons::avatar(&dir, &url).unwrap());
            }
            let (_, tasks) = collections(r"C:\scratch\hubchat.exe", &[], &icons).unwrap();
            for (i, view) in ["settings", "directory", "newchat"].iter().enumerate() {
                let item = tasks.GetAt::<IShellLinkW>(i as u32).unwrap();
                item.GetIconLocation(&mut path, &mut index).unwrap();
                let actual = PathBuf::from(String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap()]));
                assert_eq!(actual, icons[*view]);
                assert_eq!(arguments(&item).unwrap(), format!("\"hubchat://view?name={view}\""));
                let loaded = LoadImageW(std::ptr::null_mut(), path.as_ptr(), IMAGE_ICON, 32, 32, LR_LOADFROMFILE);
                assert!(!loaded.is_null(), "Windows loads the client task glyph");
                assert_ne!(DestroyIcon(loaded), 0);
            }
            let png = std::fs::read(fixture_dir.join("profile-bg.png")).unwrap();
            let background = crate::jumplist_icons::avatar(&dir, &format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))).unwrap();
            let transparent = crate::jumplist_icons::fixture(128, [255, 0, 0, 0]);
            let composed = crate::jumplist_icons::profile_on(&dir, &transparent.meta, Some(&transparent), &background).unwrap();
            let pixels = image::open(&composed).unwrap().to_rgba8();
            assert_eq!(pixels.get_pixel(0,0).0[3], 0);
            assert_eq!(pixels.get_pixel(64,64), image::open(&background).unwrap().to_rgba8().get_pixel(64,64));
            let item = link(r"C:\scratch\hubchat.exe", "Transparent profile", &chat_link("me.123456", "p.123456"), Some(&composed)).unwrap();
            item.GetIconLocation(&mut path, &mut index).unwrap();
            let loaded = LoadImageW(std::ptr::null_mut(), path.as_ptr(), IMAGE_ICON, 32, 32, LR_LOADFROMFILE);
            assert!(!loaded.is_null(), "Windows loads the real client-background profile composite");
            assert_ne!(DestroyIcon(loaded), 0);
        }
    }
    #[test]
    fn chat_activation_encodes_identity_and_contact_as_one_argument() {
        let link = chat_link("me.123456", "peer.123456&other=\"x");
        assert!(!link.contains('"'));
        let url = url::Url::parse(&link).unwrap();
        let pairs = url.query_pairs().collect::<std::collections::HashMap<_,_>>();
        assert_eq!(pairs.get("to").unwrap(), "peer.123456&other=\"x");
        assert_eq!(pairs.get("owner").unwrap(), "me.123456");
    }
    #[test]
    fn shell_link_keeps_title_path_and_activation_without_publishing() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let _apartment = Apartment;
            let item = link(r"C:\scratch\hubchat.exe", "A contact", "hubchat://chat?to=peer.123456&owner=me.123456", None).unwrap();
            let mut args = [0u16; 1024];
            item.GetArguments(&mut args).unwrap();
            let args = String::from_utf16_lossy(&args[..args.iter().position(|&c|c==0).unwrap()]);
            assert_eq!(args, "\"hubchat://chat?to=peer.123456&owner=me.123456\"");
            let props: IPropertyStore = item.cast().unwrap();
            let title = props.GetValue(&TITLE).unwrap();
            assert_eq!(BSTR::try_from(&title).unwrap().to_string(), "A contact");
        }
    }

    #[test]
    fn recent_and_fixed_collections_are_separate_and_icons_are_real_loadable_profile_files() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let _apartment = Apartment;
            let dir = crate::jumplist_icons::test_dir();
            let record = crate::jumplist_icons::fixture(64, [12, 34, 56, 255]);
            let icon = crate::jumplist_icons::resolve(&dir, &record.meta, Some(&record)).unwrap();
            let exe = r"C:\scratch\hubchat.exe";
            let contacts = [ContactLink { name: "A contact".into(), target: chat_link("me.123456", "peer.123456"), icon: Some(icon.clone()) }];
            let (recent, tasks) = collections(exe, &contacts, &HashMap::new()).unwrap();
            assert_eq!(recent.GetCount().unwrap(), 1);
            assert_eq!(tasks.GetCount().unwrap(), 3);
            let item = recent.GetAt::<IShellLinkW>(0).unwrap();
            let mut path = [0u16; 4096]; let mut index = -1;
            item.GetIconLocation(&mut path, &mut index).unwrap();
            let actual = String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap()]);
            assert_eq!(std::path::PathBuf::from(actual), icon);
            assert_eq!(index, 0);
            use windows_sys::Win32::UI::WindowsAndMessaging::{LoadImageW, DestroyIcon, IMAGE_ICON, LR_LOADFROMFILE};
            let loaded = LoadImageW(std::ptr::null_mut(), path.as_ptr(), IMAGE_ICON, 32, 32, LR_LOADFROMFILE);
            assert!(!loaded.is_null(), "Windows can load the cached profile ICO");
            assert_ne!(DestroyIcon(loaded), 0);
            for (i, view) in ["settings", "directory", "newchat"].iter().enumerate() {
                let task = tasks.GetAt::<IShellLinkW>(i as u32).unwrap();
                assert_eq!(arguments(&task).unwrap(), format!("\"hubchat://view?name={view}\""));
                task.GetIconLocation(&mut path, &mut index).unwrap();
                let actual = String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap()]);
                assert_eq!(actual, exe, "fixed tasks retain the default icon");
            }
            let no_picture = link(exe, "No picture", &chat_link("me.123456", "none.123456"), None).unwrap();
            no_picture.GetIconLocation(&mut path, &mut index).unwrap();
            assert_eq!(String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap()]), exe);
        }
    }

    #[test]
    fn picture_metadata_and_byte_arrival_change_the_windows_projection_key() {
        let first = crate::jumplist_icons::fixture(8, [1, 2, 3, 255]);
        let mut stamp = PictureStamp { peer: "peer.123456".into(), meta: Some(first.meta.clone()), available: false, avatar: None, background: None };
        let before = stamp.clone();
        stamp.available = true;
        assert!(stamp != before, "late admitted bytes refresh the fallback icon");
        let before = stamp.clone();
        stamp.meta = Some(crate::jumplist_icons::fixture(8, [4, 5, 6, 255]).meta);
        assert!(stamp != before, "new profile picture refreshes an unchanged contact row");
        let before = stamp.clone();
        stamp.meta = Some(hubchat_core::pictures::PictureRecord::removed(hubchat_core::pictures::next_revision(None).unwrap()).meta);
        assert!(stamp != before, "removal refreshes the default icon");
    }
}
