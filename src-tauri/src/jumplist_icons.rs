//! Content-addressed profile icons in Hubchat's own app data. A new picture
//! gets a new path, so Explorer cannot retain the old icon for the same link.
use std::{io::Cursor, path::{Path, PathBuf}};
use hubchat_core::pictures::{PictureMeta, PictureRecord};

pub fn path(data: &Path, meta: &PictureMeta) -> Option<PathBuf> {
    if !meta.valid() { return None; }
    meta.sha256.as_ref().map(|hash| data.join("jump-list-icons").join(format!("{hash}.ico")))
}

pub fn cached(data: &Path, meta: &PictureMeta) -> Option<PathBuf> {
    path(data, meta).filter(|p| p.is_file())
}

/// Renderer rasterizes the shared client avatar. Admit only a small square PNG,
/// strip its metadata, then use the existing content-addressed ICO cache.
pub fn avatar(data: &Path, png_url: &str) -> Option<PathBuf> {
    use base64::Engine as _;
    if png_url.len() > 48 * 1024 { return None; }
    let bytes = base64::engine::general_purpose::STANDARD.decode(png_url.strip_prefix("data:image/png;base64,")?).ok()?;
    let dimensions = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png).into_dimensions().ok()?;
    if dimensions != (128, 128) { return None; }
    let pixels = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let mut normalized = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut normalized, 128, 128);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(pixels.as_raw()).ok()?;
    }
    let record = hubchat_core::pictures::from_png(hubchat_core::pictures::next_revision(None).ok()?, normalized, 128).ok()?;
    resolve(data, &record.meta, Some(&record))
}

/// Prior behavior control: task glyphs inherited the app page's colour.
pub fn task(data: &Path, png_url: &str, _dark: bool) -> Option<PathBuf> {
    avatar(data, png_url)
}

/// Only the store's currently admitted record can supply the requested pixels.
/// Missing bytes, removals and failed conversion keep the executable fallback.
pub fn resolve(data: &Path, meta: &PictureMeta, record: Option<&PictureRecord>) -> Option<PathBuf> {
    if let Some(path) = cached(data, meta) { return Some(path); }
    let record = record.filter(|record| record.meta == *meta)?;
    let bytes = record.decoded().ok()??;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?;
    let image = if image.width() > 256 || image.height() > 256 { image.thumbnail(256, 256) } else { image };
    let mut ico = Cursor::new(Vec::new());
    image.write_to(&mut ico, image::ImageFormat::Ico).ok()?;
    let path = path(data, meta)?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let temporary = path.with_extension("ico.part");
    std::fs::write(&temporary, ico.into_inner()).ok()?;
    if std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return cached(data, meta);
    }
    Some(path)
}

#[cfg(test)]
pub fn fixture(size: u32, color: [u8; 4]) -> PictureRecord {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, size, size);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&color.repeat((size * size) as usize)).unwrap();
    }
    hubchat_core::pictures::from_png(hubchat_core::pictures::next_revision(None).unwrap(), bytes, size).unwrap()
}

#[cfg(test)]
pub fn test_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("hubchat-jump-icons-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_glyph_icos_contrast_with_light_and_dark_windows_panels() {
        let dir = test_dir();
        let source = fixture(128, [232, 232, 232, 192]);
        let url = format!("data:image/png;base64,{}", source.data.as_ref().unwrap());
        let mut paths = vec![];
        for (dark, rgb, panel) in [(false, 48u8, 255u8), (true, 232u8, 32u8)] {
            let path = task(&dir, &url, dark).unwrap();
            let pixel = image::open(&path).unwrap().to_rgba8().get_pixel(64, 64).0;
            assert_eq!(pixel, [rgb, rgb, rgb, 192], "theme colour changes without altering the glyph alpha mask");
            let luminance = |v: u8| { let s = f64::from(v) / 255.; if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) } };
            // Check actual alpha-composited RGB, not just the nominal ink.
            let visible = ((f64::from(rgb) * 192. + f64::from(panel) * 63.) / 255.).round() as u8;
            let a = luminance(visible); let b = luminance(panel);
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
            paths.push(path);
        }
        assert_ne!(paths[0], paths[1], "theme variants have different content-addressed cache paths");
    }
    #[test]
    fn avatar_cache_preserves_transparency_and_versions_changed_pixels() {
        let dir = test_dir();
        let url = |r: &PictureRecord|format!("data:image/png;base64,{}", r.data.as_ref().unwrap());
        let first = fixture(128, [90, 100, 110, 128]);
        let a = avatar(&dir, &url(&first)).unwrap();
        assert_eq!(image::open(&a).unwrap().to_rgba8().dimensions(), (128, 128));
        assert_eq!(avatar(&dir, &url(&first)).unwrap(), a);
        assert_eq!(image::open(&a).unwrap().to_rgba8().get_pixel(64, 64).0, [90, 100, 110, 128]);
        assert_ne!(avatar(&dir, &url(&fixture(128, [20, 30, 40, 255]))).unwrap(), a);
        assert!(avatar(&dir, &url(&fixture(512, [0, 0, 0, 255]))).is_none());
        assert!(avatar(&dir, "data:image/png;base64,not-a-picture").is_none());
        assert!(avatar(&dir, &"x".repeat(48 * 1024 + 1)).is_none());
    }
    #[test]
    fn profile_revisions_refresh_icon_paths_and_missing_pictures_use_fallback() {
        let dir = test_dir();
        let first = fixture(32, [255, 0, 0, 255]);
        assert!(resolve(&dir, &first.meta, None).is_none());
        let a = resolve(&dir, &first.meta, Some(&first)).unwrap();
        assert_eq!(image::open(&a).unwrap().to_rgba8().get_pixel(0, 0).0, [255, 0, 0, 255]);
        // Cached ICO remains usable after the bounded SQLite byte cache evicts it.
        assert_eq!(resolve(&dir, &first.meta, None).unwrap(), a);
        let second = fixture(512, [0, 0, 255, 255]);
        let b = resolve(&dir, &second.meta, Some(&second)).unwrap();
        assert_ne!(a, b);
        let pixels = image::open(&b).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), (256, 256));
        assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 255, 255]);
        let removed = PictureRecord::removed(hubchat_core::pictures::next_revision(Some(&second.meta)).unwrap());
        assert!(resolve(&dir, &removed.meta, Some(&removed)).is_none());
    }
    #[test]
    fn missing_corrupt_or_stale_records_never_replace_current_profile_icon() {
        let dir = test_dir();
        let first = fixture(8, [1, 2, 3, 255]);
        let second = fixture(8, [4, 5, 6, 255]);
        assert!(resolve(&dir, &second.meta, Some(&first)).is_none());
        let mut corrupt = second.clone(); corrupt.data = Some("not PNG".into());
        assert!(resolve(&dir, &second.meta, Some(&corrupt)).is_none());
        let mut bad_path = first.meta.clone(); bad_path.sha256 = Some("../../outside".into());
        assert!(path(&dir, &bad_path).is_none());
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
    }
}
