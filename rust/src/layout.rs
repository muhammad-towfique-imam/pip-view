//! The positions of the controls in the setup window, in physical pixels.

/// A rectangle: left, top, width, height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// A rectangle with a size of at least 1×1 and no negative position.
fn rect(x: i32, y: i32, w: i32, h: i32) -> R {
    R { x: x.max(0), y: y.max(0), w: w.max(1), h: h.max(1) }
}

/// The controls of the camera page and of the area page.
#[derive(Clone, Copy, Debug)]
pub struct SetupLayout {
    pub cam_label: R,
    pub cam_list: R,
    pub stream_label: R,
    pub stream_list: R,
    pub status: R,
    pub next: R,
    pub image: R,
    pub hint: R,
    pub back: R,
    pub full: R,
    pub confirm: R,
}

/// The layout for a client area of `width`×`height` at `dpi`.
pub fn layout_setup(width: i32, height: i32, dpi: u32) -> SetupLayout {
    let px = |v: i32| v * dpi as i32 / 96;
    let (m, g, bh, bw, lh) = (px(12), px(8), px(28), px(110), px(20));

    // Camera page: the 2 lists, then the status text and Next in the last row.
    let row_y = height - m - bh;
    let next = rect(width - m - bw, row_y, bw, bh);
    let status = rect(m, row_y, next.x - g - m, bh);
    let cam_label = rect(m, m, width - 2 * m, lh);
    let lists_top = m + lh + g;
    let lists_bottom = row_y - g;
    let avail = lists_bottom - lists_top - lh - 2 * g;
    let cam_h = avail * 2 / 5;
    let cam_list = rect(m, lists_top, width - 2 * m, cam_h);
    let stream_label = rect(m, lists_top + cam_h + g, width - 2 * m, lh);
    let stream_top = stream_label.y + lh + g;
    let stream_list = rect(m, stream_top, width - 2 * m, lists_bottom - stream_top);

    // Area page: the image, the hint, then Back, Full image, and Confirm.
    let back = rect(m, row_y, bw, bh);
    let full = rect(m + bw + g, row_y, bw, bh);
    let confirm = rect(width - m - bw, row_y, bw, bh);
    let hint = rect(m, row_y - g - lh, width - 2 * m, lh);
    let image = rect(m, m, width - 2 * m, hint.y - g - m);

    SetupLayout { cam_label, cam_list, stream_label, stream_list, status, next, image, hint, back, full, confirm }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(r: &R, w: i32, h: i32) -> bool {
        r.x >= 0 && r.y >= 0 && r.w > 0 && r.h > 0 && r.x + r.w <= w && r.y + r.h <= h
    }

    fn overlap(a: &R, b: &R) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    fn all(l: &SetupLayout) -> Vec<(&'static str, R)> {
        vec![
            ("cam_label", l.cam_label), ("cam_list", l.cam_list), ("stream_label", l.stream_label),
            ("stream_list", l.stream_list), ("status", l.status), ("next", l.next), ("image", l.image),
            ("hint", l.hint), ("back", l.back), ("full", l.full), ("confirm", l.confirm),
        ]
    }

    #[test]
    fn layout_fits_800x600_at_96dpi() {
        let l = layout_setup(800, 600, 96);
        for (name, r) in all(&l) {
            assert!(inside(&r, 800, 600), "{name}: {r:?}");
        }
        assert!(!overlap(&l.back, &l.full) && !overlap(&l.full, &l.confirm) && !overlap(&l.back, &l.confirm));
        assert!(!overlap(&l.image, &l.hint) && !overlap(&l.hint, &l.confirm));
        assert!(!overlap(&l.cam_list, &l.stream_list) && !overlap(&l.stream_list, &l.next));
    }

    #[test]
    fn layout_scales_with_dpi() {
        let a = layout_setup(800, 600, 96);
        let b = layout_setup(1200, 900, 144);
        assert_eq!(b.next.h * 2, a.next.h * 3);
        assert_eq!(b.next.w * 2, a.next.w * 3);
    }

    #[test]
    fn layout_small_window_has_no_negative_size() {
        let l = layout_setup(120, 90, 96);
        for (name, r) in all(&l) {
            assert!(r.w >= 1 && r.h >= 1, "{name}: {r:?}");
        }
    }
}
