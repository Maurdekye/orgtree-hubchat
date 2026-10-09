//! The clipboard for the desktop right-click menu's Paste (user 2026-10-09
//! 05:54Z). Read natively: the WebView asks the user before a page may read
//! the clipboard, and Ctrl+V needs nothing of the kind.

type R<T> = Result<T, String>;

/// The clipboard's text, if it holds any.
#[tauri::command]
pub async fn hc_clipboard_text() -> R<Option<String>> {
    #[cfg(desktop)]
    {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        Ok(cb.get_text().ok().filter(|t| !t.is_empty()))
    }
    #[cfg(not(desktop))]
    Err("not on this platform".into())
}

/// The clipboard's picture as a PNG, if it holds one.
#[tauri::command]
pub async fn hc_clipboard_image() -> R<tauri::ipc::Response> {
    #[cfg(desktop)]
    {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let img = cb.get_image().map_err(|_| "no picture on the clipboard".to_string())?;
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, img.width as u32, img.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().map_err(|e| e.to_string())?;
            w.write_image_data(&img.bytes).map_err(|e| e.to_string())?;
        }
        Ok(tauri::ipc::Response::new(out))
    }
    #[cfg(not(desktop))]
    Err("not on this platform".into())
}
