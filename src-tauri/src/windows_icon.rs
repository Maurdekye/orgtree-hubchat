//! Keep the executable's native icon masks for Windows shell consumers.
//!
//! Tao 0.37.1's RGBA conversion passes an unpacked byte-per-pixel AND mask to
//! CreateIcon. Alpha-aware drawing hides this, but mask-based image lists show
//! white corruption. LoadImageW preserves the ICO resource's correct mask.

use std::{error::Error, io, ptr};
use tauri::Manager;
use windows_sys::Win32::{
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        DestroyIcon, GetSystemMetrics, LoadImageW, SendMessageW, HICON, ICON_BIG, ICON_SMALL,
        IMAGE_ICON, LR_DEFAULTCOLOR, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, WM_SETICON,
    },
};

// Store opaque handle values (never dereferenced), so Tauri's managed state can
// retain them until app teardown. Unlike LR_SHARED, owned icons can use both
// requested sizes independently. WM_SETICON borrows them rather than owning them.
struct NativeIcon(usize);

impl Drop for NativeIcon {
    fn drop(&mut self) {
        unsafe { DestroyIcon(self.0 as HICON) };
    }
}

struct WindowIcons {
    _small: NativeIcon,
    _big: NativeIcon,
}

fn load(width: i32, height: i32) -> io::Result<NativeIcon> {
    let handle = unsafe {
        let module = GetModuleHandleW(ptr::null());
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        LoadImageW(
            module,
            tauri::utils::platform::WINDOWS_APP_ICON_RESOURCE_ID as usize as *const u16,
            IMAGE_ICON,
            width,
            height,
            LR_DEFAULTCOLOR,
        )
    };
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(NativeIcon(handle as usize))
    }
}

pub fn install(window: &tauri::WebviewWindow) -> Result<(), Box<dyn Error>> {
    let hwnd = window.hwnd()?.0 as _;
    let small = load(unsafe { GetSystemMetrics(SM_CXSMICON) }, unsafe {
        GetSystemMetrics(SM_CYSMICON)
    })?;
    let big = load(unsafe { GetSystemMetrics(SM_CXICON) }, unsafe {
        GetSystemMetrics(SM_CYICON)
    })?;
    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small.0 as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big.0 as isize);
    }
    window.manage(WindowIcons {
        _small: small,
        _big: big,
    });
    Ok(())
}
