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

#[cfg(test)]
mod tests {
    use super::*;

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
