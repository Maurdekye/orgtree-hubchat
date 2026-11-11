//! Reading a QR code from a camera frame (Hubchat Web scans the desktop
//! app's link QR; user 2026-11-11). An iPhone's browser has no barcode
//! reader of its own, so the page sends frames to the core's worker and
//! `rqrr` (pure Rust, MIT OR Apache-2.0, with ISC parts) reads them here.
//! Nothing of a frame is kept or logged.

/// The text of the first QR code found in a frame of `width` x `height`
/// grey pixels (one byte each, row by row, 0 black to 255 white). None: no
/// code in it, or a frame that is not what it says.
pub fn decode(width: usize, height: usize, grey: &[u8]) -> Option<String> {
    if width == 0 || height == 0 || width.checked_mul(height) != Some(grey.len()) {
        return None;
    }
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| grey[y * width + x]);
    image.detect_grids().into_iter().find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` as a QR code in a grey frame: `scale` pixels a module, on a
    /// lighter-than-white background as a camera sees a screen, at an offset.
    fn frame(text: &str, scale: usize, width: usize, height: usize, left: usize, top: usize) -> Vec<u8> {
        let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
        let n = code.width();
        let dark = code.to_colors();
        let mut grey = vec![200u8; width * height];
        // the quiet zone the code needs around it: white, four modules wide
        let quiet = 4 * scale;
        for y in 0..(n * scale + 2 * quiet) {
            for x in 0..(n * scale + 2 * quiet) {
                let inside = x >= quiet && y >= quiet && x < quiet + n * scale && y < quiet + n * scale;
                let black = inside && dark[((y - quiet) / scale) * n + (x - quiet) / scale] == qrcode::Color::Dark;
                grey[(top + y) * width + left + x] = if black { 24 } else { 236 };
            }
        }
        grey
    }

    #[test]
    fn a_link_qr_in_a_camera_frame_is_read() {
        let link = "hubchat://link?code=K7QD4MXP9TRA2HZE&hub=https%3A%2F%2Fpc.example.ts.net%3A8443&hub=http%3A%2F%2F100.64.0.7%3A7378&name=home&role=give";
        let grey = frame(link, 4, 640, 480, 150, 60);
        assert_eq!(decode(640, 480, &grey).as_deref(), Some(link));
        // small on the frame, as a phone held far from the screen sees it
        let grey = frame(link, 2, 640, 480, 300, 200);
        assert_eq!(decode(640, 480, &grey).as_deref(), Some(link));
    }

    #[test]
    fn a_frame_without_a_code_reads_as_nothing() {
        assert_eq!(decode(640, 480, &vec![128u8; 640 * 480]), None);
        // noise that is no code
        let noise: Vec<u8> = (0..640 * 480).map(|i: usize| ((i * 2654435761) >> 7) as u8).collect();
        assert_eq!(decode(640, 480, &noise), None);
        // a frame that is not the size it says is refused, not read out of bounds
        assert_eq!(decode(640, 480, &[0u8; 100]), None);
        assert_eq!(decode(0, 0, &[]), None);
        assert_eq!(decode(usize::MAX, 2, &[0u8; 4]), None);
    }
}
