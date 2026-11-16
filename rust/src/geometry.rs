//! Geometry of the selected camera area.

use serde::{Deserialize, Serialize};

/// Smallest side of a selected area, as a fraction of the image.
pub const MIN_SIDE: f32 = 0.02;

/// A rectangle in normalized image coordinates (0.0 to 1.0).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    /// The full image.
    pub const FULL: NormRect = NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    /// Makes a rectangle from 2 drag points in any order. The points are
    /// clamped to the image and each side is at least `MIN_SIDE`.
    pub fn from_drag(a: (f32, f32), b: (f32, f32)) -> NormRect {
        let (x, w) = axis(a.0, b.0);
        let (y, h) = axis(a.1, b.1);
        NormRect { x, y, w, h }
    }

    /// Pixel width divided by pixel height of this area in an image of
    /// `img_w` x `img_h` pixels.
    pub fn aspect(&self, img_w: u32, img_h: u32) -> f32 {
        (self.w * img_w as f32) / (self.h * img_h as f32).max(f32::EPSILON)
    }
}

/// Returns the start and the length of 1 axis of a dragged rectangle.
fn axis(a: f32, b: f32) -> (f32, f32) {
    let lo = a.min(b).clamp(0.0, 1.0);
    let hi = a.max(b).clamp(0.0, 1.0);
    if hi - lo >= MIN_SIDE {
        return (lo, hi - lo);
    }
    let center = (lo + hi) / 2.0;
    let start = (center - MIN_SIDE / 2.0).clamp(0.0, 1.0 - MIN_SIDE);
    (start, MIN_SIDE)
}

/// Returns `(x, y, w, h)` of the largest rectangle with `aspect` that fits
/// centered in `container`.
pub fn fit_aspect(container: (f32, f32), aspect: f32) -> (f32, f32, f32, f32) {
    let (cw, ch) = container;
    let (w, h) = if cw / ch > aspect {
        (ch * aspect, ch)
    } else {
        (cw, cw / aspect)
    };
    ((cw - w) / 2.0, (ch - h) / 2.0, w, h)
}

/// Initial PiP window width in points.
pub const PIP_WIDTH: f32 = 320.0;

/// Smallest overlap, in pixels per axis, of a window with the desktop for
/// the window to count as visible.
const MIN_VISIBLE: f32 = 32.0;

/// Initial PiP size for an area with `aspect`: `PIP_WIDTH` wide, scaled down
/// to fit in `max` with the same aspect ratio.
pub fn pip_size(aspect: f32, max: (f32, f32)) -> (f32, f32) {
    let (w, h) = (PIP_WIDTH, PIP_WIDTH / aspect);
    let scale = (max.0 / w).min(max.1 / h).min(1.0);
    (w * scale, h * scale)
}

/// True if a window at `pos` with `size` overlaps `desktop` (x, y, w, h) by
/// at least `MIN_VISIBLE` pixels on each axis.
pub fn position_visible(pos: (f32, f32), size: (f32, f32), desktop: (f32, f32, f32, f32)) -> bool {
    let overlap = |p: f32, len: f32, d: f32, dlen: f32| (p + len).min(d + dlen) - p.max(d);
    let finite = [pos.0, pos.1, size.0, size.1].iter().all(|v| v.is_finite());
    finite
        && overlap(pos.0, size.0, desktop.0, desktop.2) >= MIN_VISIBLE
        && overlap(pos.1, size.1, desktop.1, desktop.3) >= MIN_VISIBLE
}

/// Where a pointer press on the PiP window acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Move,
    Resize,
}

/// The bottom-right `grip` square of a `w`×`h` window resizes, the rest moves.
pub fn hit_test(x: i32, y: i32, w: i32, h: i32, grip: i32) -> Hit {
    if x >= w - grip && y >= h - grip {
        Hit::Resize
    } else {
        Hit::Move
    }
}

/// The window size for the width `w` with the aspect kept, at least
/// `min_w`×`min_h`.
pub fn keep_aspect(w: i32, aspect: f32, min_w: i32, min_h: i32) -> (i32, i32) {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let min_w = min_w.max((min_h as f32 * aspect).ceil() as i32);
    let w = w.max(min_w);
    (w, ((w as f32 / aspect).round() as i32).max(min_h))
}

/// The area in frame pixels (x, y, width, height), at least 1×1 and inside
/// the frame.
pub fn crop_src(area: NormRect, w: u32, h: u32) -> (i32, i32, i32, i32) {
    let axis = |start: f32, len: f32, size: u32| {
        let size = size.max(1) as i32;
        let s = ((start * size as f32).round() as i32).clamp(0, size - 1);
        let l = ((len * size as f32).round() as i32).clamp(1, size - s);
        (s, l)
    };
    let (x, cw) = axis(area.x, area.w, w);
    let (y, ch) = axis(area.y, area.h, h);
    (x, y, cw, ch)
}

/// The first PiP window rectangle (x, y, width, height) in physical pixels.
/// The saved size is made smaller to fit the work area. Without a saved size,
/// the size comes from the area aspect (`PIP_WIDTH` points wide); without an
/// aspect, the caller must wait for the first frame (`None`). A saved position
/// that is not visible on the desktop is replaced by the work area center.
pub fn first_pip_rect(
    saved_pos: Option<(f32, f32)>,
    saved_size: Option<(f32, f32)>,
    frame_aspect: Option<f32>,
    desktop: (f32, f32, f32, f32),
    work_area: (f32, f32, f32, f32),
    dpi_scale: f32,
) -> Option<(i32, i32, i32, i32)> {
    let (wx, wy, ww, wh) = work_area;
    let (w, h) = match saved_size {
        Some((w, h)) if w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite() => {
            let s = (ww / w).min(wh / h).min(1.0);
            (w * s, h * s)
        }
        _ => {
            let aspect = frame_aspect?;
            let max = (ww * 0.8 / dpi_scale, wh * 0.8 / dpi_scale);
            let (w, h) = pip_size(aspect, max);
            (w * dpi_scale, h * dpi_scale)
        }
    };
    let (x, y) = match saved_pos {
        Some(pos) if position_visible(pos, (w, h), desktop) => pos,
        _ => (wx + (ww - w) / 2.0, wy + (wh - h) / 2.0),
    };
    Some((x.round() as i32, y.round() as i32, w.round() as i32, h.round() as i32))
}

/// Copies the `src` area (x, y, width, height) of a top-down 4-byte-per-pixel
/// frame `width` pixels wide into a new tight buffer. Empty if the frame
/// buffer is too short. (GDI reads the source y of a top-down DIB in a way that
/// differs between versions; a full-image draw of a copy avoids the question.)
pub fn crop_rows(frame: &[u8], width: u32, src: (i32, i32, i32, i32)) -> Vec<u8> {
    let (x, y, w, h) = (src.0.max(0) as usize, src.1.max(0) as usize, src.2.max(0) as usize, src.3.max(0) as usize);
    let stride = width as usize * 4;
    if x + w > width as usize || (y + h) * stride > frame.len() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let start = row * stride + x * 4;
        out.extend_from_slice(&frame[start..start + w * 4]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: (f32, f32, f32, f32) = (0.0, 0.0, 1920.0, 1080.0);
    const WORK: (f32, f32, f32, f32) = (0.0, 0.0, 1920.0, 1040.0);

    #[test]
    fn first_rect_uses_saved_when_visible() {
        let r = first_pip_rect(Some((100.0, 100.0)), Some((320.0, 180.0)), None, DESKTOP, WORK, 1.0);
        assert_eq!(r, Some((100, 100, 320, 180)));
    }

    #[test]
    fn first_rect_ignores_offscreen_pos() {
        let r = first_pip_rect(Some((5000.0, 5000.0)), Some((320.0, 180.0)), None, DESKTOP, WORK, 1.0);
        assert_eq!(r, Some((800, 430, 320, 180)));
    }

    #[test]
    fn first_rect_clamps_large_size() {
        let r = first_pip_rect(Some((0.0, 0.0)), Some((4000.0, 2000.0)), None, DESKTOP, WORK, 1.0);
        assert_eq!(r, Some((0, 0, 1920, 960)));
    }

    #[test]
    fn first_rect_waits_for_frame_without_size() {
        assert_eq!(first_pip_rect(None, None, None, DESKTOP, WORK, 1.0), None);
    }

    #[test]
    fn first_rect_from_aspect_uses_dpi_scale() {
        let r = first_pip_rect(None, None, Some(16.0 / 9.0), DESKTOP, WORK, 1.5);
        assert_eq!(r, Some((720, 385, 480, 270)));
    }

    #[test]
    fn crop_rows_takes_the_area() {
        // A 3x2 frame, 4 bytes per pixel; each pixel holds its index.
        let frame: Vec<u8> = (0..6u8).flat_map(|i| [i; 4]).collect();
        let out = crop_rows(&frame, 3, (1, 1, 2, 1));
        assert_eq!(out, vec![4, 4, 4, 4, 5, 5, 5, 5]);
    }

    #[test]
    fn crop_rows_full_frame_is_a_copy() {
        let frame: Vec<u8> = (0..6u8).flat_map(|i| [i; 4]).collect();
        assert_eq!(crop_rows(&frame, 3, (0, 0, 3, 2)), frame);
    }

    #[test]
    fn crop_rows_short_buffer_is_empty() {
        assert!(crop_rows(&[0u8; 8], 3, (0, 0, 3, 2)).is_empty());
    }

    #[test]
    fn hit_corner_is_resize() {
        assert_eq!(hit_test(310, 170, 320, 180, 16), Hit::Resize);
    }

    #[test]
    fn hit_elsewhere_is_move() {
        assert_eq!(hit_test(100, 170, 320, 180, 16), Hit::Move);
        assert_eq!(hit_test(310, 10, 320, 180, 16), Hit::Move);
    }

    #[test]
    fn keep_aspect_basic() {
        assert_eq!(keep_aspect(300, 16.0 / 9.0, 64, 48), (300, 169));
    }

    #[test]
    fn keep_aspect_min() {
        let (w, h) = keep_aspect(10, 16.0 / 9.0, 64, 48);
        assert!(w >= 64 && h >= 48, "{w}x{h}");
    }

    #[test]
    fn crop_src_full() {
        assert_eq!(crop_src(NormRect::FULL, 640, 480), (0, 0, 640, 480));
    }

    #[test]
    fn crop_src_area() {
        let a = NormRect { x: 0.25, y: 0.5, w: 0.5, h: 0.25 };
        assert_eq!(crop_src(a, 640, 480), (160, 240, 320, 120));
    }

    #[test]
    fn crop_src_tiny_area_is_1px() {
        let a = NormRect { x: 1.0, y: 1.0, w: 0.0, h: 0.0 };
        let (x, y, w, h) = crop_src(a, 640, 480);
        assert!(w >= 1 && h >= 1 && x + w <= 640 && y + h <= 480, "{x} {y} {w} {h}");
    }

    #[test]
    fn forward_drag() {
        let r = NormRect::from_drag((0.1, 0.2), (0.5, 0.6));
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(r.x, 0.1) && close(r.y, 0.2) && close(r.w, 0.4) && close(r.h, 0.4), "{r:?}");
    }

    #[test]
    fn reverse_drag_equals_forward() {
        assert_eq!(
            NormRect::from_drag((0.5, 0.6), (0.1, 0.2)),
            NormRect::from_drag((0.1, 0.2), (0.5, 0.6))
        );
    }

    #[test]
    fn drag_outside_is_clamped() {
        let r = NormRect::from_drag((-0.3, 0.5), (1.4, 2.0));
        assert_eq!(r, NormRect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 });
    }

    #[test]
    fn click_gets_min_size() {
        let r = NormRect::from_drag((0.5, 0.5), (0.5, 0.5));
        assert!(r.w >= MIN_SIDE && r.h >= MIN_SIDE);
    }

    #[test]
    fn min_size_stays_inside_at_edge() {
        let r = NormRect::from_drag((1.0, 1.0), (1.0, 1.0));
        assert!(r.x + r.w <= 1.0 + 1e-6 && r.y + r.h <= 1.0 + 1e-6 && r.w >= MIN_SIDE);
    }

    #[test]
    fn aspect_uses_pixels() {
        let r = NormRect { x: 0.0, y: 0.0, w: 0.5, h: 0.5 };
        assert!((r.aspect(1920, 1080) - 16.0 / 9.0).abs() < 1e-4);
    }

    #[test]
    fn fit_wide_container_letterboxes_sides() {
        assert_eq!(fit_aspect((200.0, 100.0), 1.0), (50.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn fit_tall_container_letterboxes_top() {
        assert_eq!(fit_aspect((100.0, 200.0), 1.0), (0.0, 50.0, 100.0, 100.0));
    }

    #[test]
    fn pip_size_normal_area_is_320_wide() {
        assert_eq!(pip_size(16.0 / 9.0, (1536.0, 864.0)), (320.0, 180.0));
    }

    #[test]
    fn pip_size_thin_vertical_strip_fits_screen() {
        let aspect = 0.089;
        let (w, h) = pip_size(aspect, (1536.0, 864.0));
        assert!(h <= 864.0, "h = {h}");
        assert!((w / h - aspect).abs() < 1e-3);
    }

    #[test]
    fn pip_size_wide_strip_fits_narrow_max() {
        let (w, h) = pip_size(20.0, (200.0, 864.0));
        assert_eq!((w, h), (200.0, 10.0));
    }

    #[test]
    fn position_on_screen_is_visible() {
        assert!(position_visible((100.0, 100.0), (320.0, 180.0), (0.0, 0.0, 1920.0, 1080.0)));
    }

    #[test]
    fn position_on_left_monitor_is_visible() {
        assert!(position_visible((-1500.0, 200.0), (320.0, 180.0), (-1920.0, 0.0, 3840.0, 1080.0)));
    }

    #[test]
    fn position_on_removed_monitor_is_not_visible() {
        assert!(!position_visible((2400.0, 100.0), (320.0, 180.0), (0.0, 0.0, 1920.0, 1080.0)));
    }

    #[test]
    fn position_with_tiny_overlap_is_not_visible() {
        assert!(!position_visible((1910.0, 100.0), (320.0, 180.0), (0.0, 0.0, 1920.0, 1080.0)));
    }

    #[test]
    fn position_not_finite_is_not_visible() {
        assert!(!position_visible((f32::NAN, 0.0), (320.0, 180.0), (0.0, 0.0, 1920.0, 1080.0)));
    }
}
