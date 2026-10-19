//! The Windows overlay is a separate transparent icon, not a replacement
//! for Hubchat's native executable/window icon. Colours match the tray dot.
pub const SIDE: u32 = 16;

pub fn rgba() -> Vec<u8> {
    let mut bytes = vec![0; (SIDE * SIDE * 4) as usize];
    let center = SIDE as f64 / 2.;
    let radius = center - 1.;
    for y in 0..SIDE { for x in 0..SIDE {
        let d = (x as f64 + 0.5 - center).hypot(y as f64 + 0.5 - center);
        let coverage = (radius + 0.5 - d).clamp(0., 1.);
        if coverage == 0. { continue; }
        let at = ((y * SIDE + x) * 4) as usize;
        let colour = if d > radius - 1.5 { [242, 242, 242] } else { [229, 72, 77] };
        bytes[at..at+3].copy_from_slice(&colour);
        bytes[at+3] = (255. * coverage).round() as u8;
    }}
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taskbar_overlay_has_transparent_corners_and_a_red_center_with_light_rim() {
        let bytes = rgba();
        assert_eq!(bytes.len(), (SIDE * SIDE * 4) as usize);
        for at in [0, (SIDE-1)*4, (SIDE*(SIDE-1))*4, (SIDE*SIDE-1)*4] {
            assert_eq!(&bytes[at as usize..at as usize+4], &[0,0,0,0]);
        }
        assert_eq!(&bytes[((8*SIDE+8)*4) as usize..((8*SIDE+8)*4+4) as usize], &[229,72,77,255]);
        assert!(bytes.chunks_exact(4).any(|p|p == [242,242,242,255]));
        assert!(bytes.chunks_exact(4).any(|p|p[3] > 0 && p[3] < 255));
        if let Ok(path) = std::env::var("HUBCHAT_TASKBAR_RGBA") { std::fs::write(path, bytes).unwrap(); }
    }
}
