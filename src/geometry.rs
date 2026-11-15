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
}
