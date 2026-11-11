//! Reading a QR code from a camera frame (Hubchat Web scans the desktop
//! app's link QR; user 2026-11-11). An iPhone's browser has no barcode
//! reader of its own, so the page sends frames to the core's worker and
//! `rqrr` (pure Rust, MIT OR Apache-2.0, with ISC parts) reads them here.
//! Nothing of a frame is kept or logged.
//!
//! A phone pointed at a monitor gives a hard picture (user 2026-11-11
//! 11:53Z, the first real camera): the code is small in the frame, a little
//! soft, tilted, with the screen's pixel grid and glare. So a frame is read
//! several ways, cheapest first, until one finds a code.

// RED: see decode
#![allow(dead_code)]

/// One grey picture: `w` x `h` bytes, row by row, 0 black to 255 white.
struct Grey {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

/// The text of the first QR code `rqrr` finds in the picture.
fn read(w: usize, h: usize, px: &[u8]) -> Option<String> {
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| px[y * w + x]);
    image.detect_grids().into_iter().find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

/// Each pixel as the mean of itself and its neighbours: takes out sensor
/// noise and a screen's pixel grid.
fn smooth(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; px.len()];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(1), (x + 1).min(w - 1));
            let mut sum = 0u32;
            let mut n = 0u32;
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    sum += px[yy * w + xx] as u32;
                    n += 1;
                }
            }
            out[y * w + x] = (sum / n) as u8;
        }
    }
    out
}

/// The part `cw` x `ch` at (`left`, `top`), twice as large (each new pixel
/// weighed from its four nearest): a small, soft code gets enough pixels a
/// module for the reader.
fn enlarged(w: usize, px: &[u8], left: usize, top: usize, cw: usize, ch: usize) -> Grey {
    let (ow, oh) = (cw * 2, ch * 2);
    let mut out = vec![0u8; ow * oh];
    for y in 0..oh {
        // the centre of the new pixel, in the old picture's pixels
        let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
        let (ya, ty) = (fy as usize, fy.fract());
        let yb = (ya + 1).min(ch - 1);
        for x in 0..ow {
            let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (cw - 1) as f32);
            let (xa, tx) = (fx as usize, fx.fract());
            let xb = (xa + 1).min(cw - 1);
            let at = |xx: usize, yy: usize| px[(top + yy) * w + left + xx] as f32;
            let v = at(xa, ya) * (1.0 - tx) * (1.0 - ty) + at(xb, ya) * tx * (1.0 - ty) + at(xa, yb) * (1.0 - tx) * ty + at(xb, yb) * tx * ty;
            out[y * ow + x] = v.round() as u8;
        }
    }
    Grey { w: ow, h: oh, px: out }
}

/// More contrast at edges: each pixel pushed away from the mean around it
/// (`rounds` passes of `smooth`), `amount` times. Undoes some of a lens out
/// of focus.
fn sharpen(w: usize, h: usize, px: &[u8], rounds: usize, amount: f32) -> Vec<u8> {
    let mut soft = px.to_vec();
    for _ in 0..rounds {
        soft = smooth(w, h, &soft);
    }
    px.iter().zip(soft.iter()).map(|(&a, &b)| (a as f32 + amount * (a as f32 - b as f32)).clamp(0.0, 255.0) as u8).collect()
}

/// The text of the first QR code in the picture, with black decided here:
/// a pixel darker than `percent` of the mean of the square of side `side`
/// around it. Glare and a dark window around the code move that mean with
/// them, where `rqrr`'s own rule looks along one row only.
fn read_local(w: usize, h: usize, px: &[u8], side: usize, percent: u64) -> Option<String> {
    // sums[y][x]: everything above and left of (x, y); a frame is far under 2^32 / 255 pixels
    let mut sums = vec![0u32; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0u32;
        for x in 0..w {
            row += px[y * w + x] as u32;
            sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
        }
    }
    let r = (side / 2).max(1);
    let mut image = rqrr::PreparedImage::prepare_from_bitmap(w, h, |x, y| {
        let (x0, y0, x1, y1) = (x.saturating_sub(r), y.saturating_sub(r), (x + r + 1).min(w), (y + r + 1).min(h));
        let sum = (sums[y1 * (w + 1) + x1] + sums[y0 * (w + 1) + x0]) as u64 - (sums[y0 * (w + 1) + x1] + sums[y1 * (w + 1) + x0]) as u64;
        let n = ((x1 - x0) * (y1 - y0)) as u64;
        (px[y * w + x] as u64) * n * 100 < sum * percent
    });
    image.detect_grids().into_iter().find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

/// The largest frame read: 4096 x 4096. A camera gives far less; this keeps
/// the sums above in 32 bits and a hostile size from taking the memory.
const MAX_PIXELS: usize = 1 << 24;

/// The text of the first QR code found in a frame of `width` x `height`
/// grey pixels (one byte each, row by row, 0 black to 255 white). None: no
/// code in it, or a frame that is not what it says.
pub fn decode(width: usize, height: usize, grey: &[u8]) -> Option<String> {
    if width == 0 || height == 0 || width.checked_mul(height) != Some(grey.len()) || grey.len() > MAX_PIXELS {
        return None;
    }
    // 1. as it is: a clear picture needs no more
    if let Some(text) = read(width, height, grey) {
        return Some(text);
    }
    // RED: the other ways to read a frame are written, not used yet
    None
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

    /// What a phone's camera makes of a QR code on a monitor: the code
    /// `module` pixels a module with its centre at (`cx`, `cy`), turned by
    /// `degrees`, on a dark window with a bright band of glare; then out of
    /// focus (`blur` passes of a 3x3 mean) and with sensor noise of +-`noise`.
    #[allow(clippy::too_many_arguments)]
    fn photo(text: &str, module: f32, width: usize, height: usize, cx: f32, cy: f32, degrees: f32, blur: usize, noise: i32) -> Vec<u8> {
        let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
        let n = code.width() as f32;
        let dark = code.to_colors();
        let (sin, cos) = degrees.to_radians().sin_cos();
        let half = (n / 2.0 + 4.0) * module;
        let mut grey = vec![0u8; width * height];
        for y in 0..height {
            for x in 0..width {
                // nine samples a pixel: edges come out grey, as a lens gives them
                let mut sum = 0.0f32;
                for sy in 0..3 {
                    for sx in 0..3 {
                        let px = x as f32 + (sx as f32 + 0.5) / 3.0 - cx;
                        let py = y as f32 + (sy as f32 + 0.5) / 3.0 - cy;
                        // back into the code's own, unturned, picture
                        let u = px * cos + py * sin;
                        let v = -px * sin + py * cos;
                        sum += if u.abs() < half && v.abs() < half {
                            let (mx, my) = ((u / module + n / 2.0).floor(), (v / module + n / 2.0).floor());
                            let inside = mx >= 0.0 && my >= 0.0 && mx < n && my < n;
                            if inside && dark[my as usize * n as usize + mx as usize] == qrcode::Color::Dark { 30.0 } else { 225.0 }
                        } else {
                            // the dark window around it, lighter towards the top (glare)
                            45.0 + 60.0 * (1.0 - y as f32 / height as f32).powi(3)
                        };
                    }
                }
                grey[y * width + x] = (sum / 9.0) as u8;
            }
        }
        for _ in 0..blur {
            grey = smooth(width, height, &grey);
        }
        // the same noise every run
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        for p in grey.iter_mut() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let d = (seed % (2 * noise as u64 + 1)) as i32 - noise;
            *p = (*p as i32 + d).clamp(0, 255) as u8;
        }
        grey
    }

    /// The link the desktop app showed in the user's first try: an https hub
    /// address and the PC's other addresses.
    const LINK: &str = "hubchat://link?code=9RRR3Y6TR2PW72AN&hub=https%3A%2F%2Fdesk-pc.tail38da1c.ts.net%3A9444&hub=http%3A%2F%2F192.168.1.20%3A7391&hub=http%3A%2F%2F100.101.102.103%3A7391&name=trial&role=give";

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
    fn a_photo_of_a_code_on_a_monitor_is_read() {
        // as the user's phone saw it (2026-11-11): a third of a 1080p frame's width (6.5 pixels a
        // module), out of focus, a little turned, noisy, on a dark window with glare
        let grey = photo(LINK, 6.5, 1080, 1920, 540.0, 960.0, 3.0, 2, 10);
        assert_eq!(decode(1080, 1920, &grey).as_deref(), Some(LINK));
        // the reader alone, as it was used before, does not read that frame
        assert_eq!(read(1080, 1920, &grey), None);
        // held further away, turned the other way, softer
        let grey = photo(LINK, 5.0, 1080, 1920, 540.0, 960.0, -7.0, 3, 10);
        assert_eq!(decode(1080, 1920, &grey).as_deref(), Some(LINK));
        // small in the frame and well turned
        let grey = photo(LINK, 4.0, 1080, 1920, 540.0, 960.0, 15.0, 2, 10);
        assert_eq!(decode(1080, 1920, &grey).as_deref(), Some(LINK));
        // the phone held sideways, close
        let grey = photo(LINK, 8.0, 1920, 1080, 960.0, 540.0, -7.0, 3, 16);
        assert_eq!(decode(1920, 1080, &grey).as_deref(), Some(LINK));
    }

    /// Not a test of the reader's promise: prints which sizes, turns and
    /// blurs it reads, for whoever tunes it. Give the reader an optimised
    /// build or it takes minutes: `cargo test -p hubchat-web --lib --config
    /// profile.dev.package.rqrr.opt-level=3 --config
    /// profile.dev.package.hubchat-web.opt-level=2
    /// scan::tests::what_the_reader_manages -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn what_the_reader_manages() {
        let (w, h) = (1080usize, 1920usize);
        for module in [2.5f32, 3.0, 4.0, 5.0, 6.5, 8.0, 12.0] {
            for blur in [0usize, 1, 2, 3] {
                let mut line = format!("module {module:>4} px, blur {blur}:");
                for (degrees, noise) in [(3.0f32, 10), (-7.0, 10), (15.0, 10), (3.0, 4), (-7.0, 16)] {
                    let grey = photo(LINK, module, w, h, 540.0, 960.0, degrees, blur, noise);
                    let plain = read(w, h, &grey).is_some();
                    let all = decode(w, h, &grey).is_some();
                    line += &format!("  {degrees:>4}/{noise:<2} {}", if plain { "plain" } else if all { "tried" } else { "-----" });
                }
                println!("{line}");
            }
        }
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
        // larger than any camera's frame
        assert_eq!(decode(4097, 4096, &vec![0u8; 4097 * 4096]), None);
    }
}
