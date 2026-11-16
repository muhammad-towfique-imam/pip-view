//! Video modes and streams of a camera, and the choice of the default.

/// The largest frame that the app selects if a smaller one exists. The PiP
/// window is small, and a larger frame costs CPU time on each frame.
const MAX_PIXELS: u32 = 1920 * 1080;
/// A higher frame rate does not make the PiP better.
const MAX_USEFUL_FPS: u32 = 30;

/// 1 native mode of a camera stream. `subtype` is the Media Foundation
/// subtype as a FourCC (or a D3DFMT number).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoMode {
    pub subtype: u32,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
}

impl VideoMode {
    /// The frame rate, rounded. 0 if it is not known.
    pub fn fps(&self) -> u32 {
        if self.fps_den == 0 {
            return 0;
        }
        ((self.fps_num as f64 / self.fps_den as f64).round()) as u32
    }

    fn pixels(&self) -> u32 {
        self.width.saturating_mul(self.height)
    }

    /// For example "1920×1080 30 fps MJPG".
    pub fn label(&self) -> String {
        format!("{}×{} {} fps {}", self.width, self.height, self.fps(), fourcc_text(self.subtype))
    }
}

/// The FourCC as text, or the number in hex if it is not printable.
fn fourcc_text(subtype: u32) -> String {
    let bytes = subtype.to_le_bytes();
    if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        String::from_utf8_lossy(&bytes).trim_end().to_string()
    } else {
        format!("0x{subtype:08X}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamKind {
    Preview,
    Capture,
    Other,
}

/// 1 video stream of a camera. `color` is false for an infrared or depth stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamInfo {
    pub index: u32,
    pub kind: StreamKind,
    pub color: bool,
    pub modes: Vec<VideoMode>,
}

/// False for the gray and depth subtypes of infrared and depth streams.
pub fn is_color_subtype(subtype: u32) -> bool {
    const GRAY: [&[u8; 4]; 3] = [b"L8  ", b"L16 ", b"D16 "];
    // D3DFMT_L8, D3DFMT_D16, D3DFMT_L16.
    const D3D: [u32; 3] = [50, 80, 81];
    !GRAY.iter().any(|g| u32::from_le_bytes(**g) == subtype) && !D3D.contains(&subtype)
}

/// The mode indexes, best first: up to 1080p with the frame rate up to 30 fps
/// first, then the largest resolution; then the larger modes, smallest first.
pub fn rank_modes(modes: &[VideoMode]) -> Vec<usize> {
    let (mut small, mut large): (Vec<usize>, Vec<usize>) =
        (0..modes.len()).partition(|&i| modes[i].pixels() <= MAX_PIXELS);
    small.sort_by_key(|&i| {
        let m = &modes[i];
        std::cmp::Reverse((m.fps().min(MAX_USEFUL_FPS), m.pixels(), m.fps()))
    });
    large.sort_by_key(|&i| (modes[i].pixels(), std::cmp::Reverse(modes[i].fps().min(MAX_USEFUL_FPS))));
    small.extend(large);
    small
}

/// The stream to use: the saved one if it is a color stream of this camera,
/// else the first color Preview stream, else the first color stream.
pub fn default_stream(streams: &[StreamInfo], saved: Option<u32>) -> Option<u32> {
    let color = || streams.iter().filter(|s| s.color);
    saved
        .filter(|i| color().any(|s| s.index == *i))
        .or_else(|| color().find(|s| s.kind == StreamKind::Preview).map(|s| s.index))
        .or_else(|| color().next().map(|s| s.index))
}

/// For example "Preview — 1920×1080 30 fps MJPG" (the best mode).
pub fn stream_label(s: &StreamInfo) -> String {
    let name = match s.kind {
        StreamKind::Preview => "Preview".to_string(),
        StreamKind::Capture => "Capture".to_string(),
        StreamKind::Other => format!("Stream {}", s.index),
    };
    match rank_modes(&s.modes).first() {
        Some(&best) => format!("{name} — {}", s.modes[best].label()),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MJPG: u32 = u32::from_le_bytes(*b"MJPG");
    const YUY2: u32 = u32::from_le_bytes(*b"YUY2");
    const L8: u32 = u32::from_le_bytes(*b"L8  ");

    fn m(w: u32, h: u32, fps: u32, subtype: u32) -> VideoMode {
        VideoMode { subtype, width: w, height: h, fps_num: fps, fps_den: 1 }
    }

    fn stream(index: u32, kind: StreamKind, color: bool) -> StreamInfo {
        StreamInfo { index, kind, color, modes: vec![m(1280, 720, 30, MJPG)] }
    }

    #[test]
    fn rank_takes_1080p30_over_4k() {
        let modes = [m(3840, 2160, 30, MJPG), m(1920, 1080, 30, MJPG), m(1280, 720, 30, MJPG)];
        assert_eq!(rank_modes(&modes), vec![1, 2, 0]);
    }

    #[test]
    fn rank_takes_30fps_over_resolution() {
        let modes = [m(1920, 1080, 5, YUY2), m(1280, 720, 30, MJPG)];
        assert_eq!(rank_modes(&modes)[0], 1);
    }

    #[test]
    fn rank_takes_1080p30_over_480p60() {
        let modes = [m(640, 480, 60, MJPG), m(1920, 1080, 30, MJPG)];
        assert_eq!(rank_modes(&modes)[0], 1);
    }

    #[test]
    fn rank_all_large_smallest_first() {
        let modes = [m(3840, 2160, 30, MJPG), m(2560, 1440, 30, MJPG)];
        assert_eq!(rank_modes(&modes), vec![1, 0]);
    }

    #[test]
    fn rank_empty() {
        assert!(rank_modes(&[]).is_empty());
    }

    #[test]
    fn default_uses_saved() {
        let s = [stream(0, StreamKind::Capture, true), stream(1, StreamKind::Preview, true)];
        assert_eq!(default_stream(&s, Some(0)), Some(0));
    }

    #[test]
    fn default_ignores_saved_infrared() {
        let s = [stream(0, StreamKind::Other, false), stream(1, StreamKind::Capture, true)];
        assert_eq!(default_stream(&s, Some(0)), Some(1));
    }

    #[test]
    fn default_ignores_missing_saved() {
        let s = [stream(0, StreamKind::Capture, true)];
        assert_eq!(default_stream(&s, Some(7)), Some(0));
    }

    #[test]
    fn default_prefers_preview() {
        let s = [stream(0, StreamKind::Capture, true), stream(1, StreamKind::Preview, true)];
        assert_eq!(default_stream(&s, None), Some(1));
    }

    #[test]
    fn default_skips_infrared_first_stream() {
        let s = [stream(0, StreamKind::Other, false), stream(1, StreamKind::Capture, true)];
        assert_eq!(default_stream(&s, None), Some(1));
    }

    #[test]
    fn default_none_without_color() {
        assert_eq!(default_stream(&[stream(0, StreamKind::Other, false)], None), None);
    }

    #[test]
    fn label_mjpg() {
        assert_eq!(m(1280, 720, 30, MJPG).label(), "1280×720 30 fps MJPG");
    }

    #[test]
    fn label_hex_subtype() {
        assert_eq!(m(640, 480, 30, 21).label(), "640×480 30 fps 0x00000015");
    }

    #[test]
    fn fps_rounding() {
        let mode = VideoMode { subtype: MJPG, width: 1, height: 1, fps_num: 30000, fps_den: 1001 };
        assert_eq!(mode.fps(), 30);
        assert_eq!(VideoMode { fps_den: 0, ..mode }.fps(), 0);
    }

    #[test]
    fn is_color_l8_false() {
        assert!(!is_color_subtype(L8));
        assert!(!is_color_subtype(50));
        assert!(is_color_subtype(MJPG));
    }

    #[test]
    fn stream_label_kinds() {
        assert_eq!(stream_label(&stream(0, StreamKind::Preview, true)), "Preview — 1280×720 30 fps MJPG");
        assert_eq!(stream_label(&stream(1, StreamKind::Capture, true)), "Capture — 1280×720 30 fps MJPG");
        assert_eq!(stream_label(&stream(3, StreamKind::Other, true)), "Stream 3 — 1280×720 30 fps MJPG");
    }
}
