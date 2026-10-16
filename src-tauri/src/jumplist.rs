//! Windows taskbar menu. Links launch this executable, so single-instance
//! routing reaches the running app without touching protocol registration.
use hubchat_core::store::ShortcutContact;
use tauri::AppHandle;
use windows::{core::{BSTR, HSTRING, Interface, Result}, Win32::{
    Foundation::PROPERTYKEY,
    System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, StructuredStorage::PROPVARIANT},
    UI::Shell::{Common::{IObjectArray, IObjectCollection}, PropertiesSystem::{IPropertyStore, PSCoerceToCanonicalValue},
        DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink},
}};

const TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9), pid: 2,
};

pub fn chat_link(owner: &str, peer: &str) -> String {
    let mut url = url::Url::parse("hubchat://chat").expect("constant URL");
    url.query_pairs_mut().append_pair("to", peer).append_pair("owner", owner);
    url.into()
}

fn link(exe: &str, title: &str, target: &str) -> Result<IShellLinkW> {
    // Only our URL builder supplies target: quoting cannot become another argument.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe))?;
        link.SetArguments(&HSTRING::from(format!("\"{target}\"")))?;
        link.SetIconLocation(&HSTRING::from(exe), 0)?;
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

fn publish(app: &AppHandle, owner: &str, rows: &[ShortcutContact]) -> Result<()> {
    let Ok(exe) = std::env::current_exe() else { return Ok(()); };
    // Shell state belongs to this build's AppUserModelID, including .test builds.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let list: ICustomDestinationList = CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        list.SetAppID(&HSTRING::from(app.config().identifier.as_str()))?;
        let mut slots = 0;
        let _removed: IObjectArray = list.BeginList(&mut slots)?;
        let result = (|| {
            let tasks: IObjectCollection = CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
            let exe = exe.to_string_lossy();
            for row in rows {
                tasks.AddObject(&link(&exe, &row.name, &chat_link(owner, &row.peer))?)?;
            }
            for (name, view) in [("Settings", "settings"), ("Directory", "directory"), ("New chat", "newchat")] {
                tasks.AddObject(&link(&exe, name, &format!("hubchat://view?name={view}"))?)?;
            }
            let array: IObjectArray = tasks.cast()?;
            list.AddUserTasks(&array)?;
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
            let result = c.with_shortcuts(true, None, |owner, rows|
                publish(&handle, owner, &rows).map_err(|_|"Jump List unavailable".to_owned()));
            if result.is_err() { tracing::warn!("could not update taskbar shortcuts"); }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
            let item = link(r"C:\scratch\hubchat.exe", "A contact", "hubchat://chat?to=peer.123456&owner=me.123456").unwrap();
            let mut args = [0u16; 1024];
            item.GetArguments(&mut args).unwrap();
            let args = String::from_utf16_lossy(&args[..args.iter().position(|&c|c==0).unwrap()]);
            assert_eq!(args, "\"hubchat://chat?to=peer.123456&owner=me.123456\"");
            let props: IPropertyStore = item.cast().unwrap();
            let title = props.GetValue(&TITLE).unwrap();
            assert_eq!(BSTR::try_from(&title).unwrap().to_string(), "A contact");
        }
    }
}
