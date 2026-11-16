//! GDI drawing: the camera image, letterbox bars, the selection, the border,
//! and the grip mark. All drawing goes to a back buffer first (no flicker).

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::*;

use super::mf::Frame;
use crate::geometry::{crop_rows, fit_aspect};

pub fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

pub fn rect_wh(r: &RECT) -> (i32, i32) {
    (r.right - r.left, r.bottom - r.top)
}

/// A memory DC with a bitmap of the size of the target area.
pub struct BackBuffer {
    pub dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
}

impl BackBuffer {
    pub fn new(target: HDC, w: i32, h: i32) -> BackBuffer {
        let (w, h) = (w.max(1), h.max(1));
        // SAFETY: GDI objects are made for `target` and released in Drop.
        unsafe {
            let dc = CreateCompatibleDC(Some(target));
            let bitmap = CreateCompatibleBitmap(target, w, h);
            let old = SelectObject(dc, bitmap.into());
            BackBuffer { dc, bitmap, old, w, h }
        }
    }

    /// Copies the buffer to `target` at (x, y).
    pub fn blit(&self, target: HDC, x: i32, y: i32) {
        // SAFETY: both DCs are valid.
        unsafe {
            let _ = BitBlt(target, x, y, self.w, self.h, Some(self.dc), 0, 0, SRCCOPY);
        }
    }
}

impl Drop for BackBuffer {
    fn drop(&mut self) {
        // SAFETY: the objects were made in `new`.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

pub fn fill(dc: HDC, r: &RECT, color: COLORREF) {
    // SAFETY: valid DC; the brush is deleted after use.
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, r, brush);
        let _ = DeleteObject(brush.into());
    }
}

/// The largest rectangle with `aspect` in `container`, centered.
pub fn letterbox(container: &RECT, aspect: f32) -> RECT {
    let (cw, ch) = rect_wh(container);
    let (x, y, w, h) = fit_aspect((cw.max(1) as f32, ch.max(1) as f32), aspect);
    RECT {
        left: container.left + x.round() as i32,
        top: container.top + y.round() as i32,
        right: container.left + (x + w).round() as i32,
        bottom: container.top + (y + h).round() as i32,
    }
}

/// Draws the `src` part (x, y, width, height) of the frame into `dest`.
/// A part is copied first, so that GDI always draws a full top-down image.
pub fn paint_frame(dc: HDC, dest: &RECT, frame: &Frame, src: (i32, i32, i32, i32)) {
    if src == (0, 0, frame.width as i32, frame.height as i32) {
        draw_bits(dc, dest, &frame.bgra, frame.width, frame.height);
    } else {
        let part = crop_rows(&frame.bgra, frame.width, src);
        if !part.is_empty() {
            draw_bits(dc, dest, &part, src.2 as u32, src.3 as u32);
        }
    }
}

/// Draws a full top-down BGRA image of `w`×`h` into `dest`.
fn draw_bits(dc: HDC, dest: &RECT, bits: &[u8], w: u32, h: u32) {
    if bits.len() < w as usize * h as usize * 4 {
        return;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            // Negative: top-down rows.
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let (dw, dh) = rect_wh(dest);
    // SAFETY: `bits` has at least w × h × 4 bytes, as `info` says.
    unsafe {
        SetStretchBltMode(dc, HALFTONE);
        let _ = SetBrushOrgEx(dc, 0, 0, None);
        StretchDIBits(
            dc,
            dest.left,
            dest.top,
            dw,
            dh,
            0,
            0,
            w as i32,
            h as i32,
            Some(bits.as_ptr() as *const _),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

/// A line of `px` pixels inside the edge of `r`.
pub fn border(dc: HDC, r: &RECT, color: COLORREF, px: i32) {
    let px = px.max(1);
    let parts = [
        RECT { left: r.left, top: r.top, right: r.right, bottom: r.top + px },
        RECT { left: r.left, top: r.bottom - px, right: r.right, bottom: r.bottom },
        RECT { left: r.left, top: r.top, right: r.left + px, bottom: r.bottom },
        RECT { left: r.right - px, top: r.top, right: r.right, bottom: r.bottom },
    ];
    for p in &parts {
        fill(dc, p, color);
    }
}

/// An outline of 1 pixel around `r`.
pub fn outline(dc: HDC, r: &RECT, color: COLORREF) {
    border(dc, r, color, 1);
}

/// 2 short diagonal lines in the bottom-right corner of `r`.
pub fn grip_mark(dc: HDC, r: &RECT, scale: f32) {
    let s = |v: f32| (v * scale).round() as i32;
    // SAFETY: valid DC; the pen is deleted after use.
    unsafe {
        let pen = CreatePen(PS_SOLID, s(1.5).max(1), rgb(170, 170, 170));
        let old = SelectObject(dc, pen.into());
        for d in [5.0, 10.0] {
            let _ = MoveToEx(dc, r.right - s(d), r.bottom - s(2.0), None);
            let _ = LineTo(dc, r.right - s(2.0), r.bottom - s(d));
        }
        SelectObject(dc, old);
        let _ = DeleteObject(pen.into());
    }
}

/// Darkens `outer` outside `inner` (the selection).
pub fn dim_outside(dc: HDC, outer: &RECT, inner: &RECT) {
    let parts = [
        RECT { left: outer.left, top: outer.top, right: outer.right, bottom: inner.top },
        RECT { left: outer.left, top: inner.bottom, right: outer.right, bottom: outer.bottom },
        RECT { left: outer.left, top: inner.top, right: inner.left, bottom: inner.bottom },
        RECT { left: inner.right, top: inner.top, right: outer.right, bottom: inner.bottom },
    ];
    // SAFETY: a 1×1 black bitmap in a memory DC is stretched with AlphaBlend.
    unsafe {
        let src = CreateCompatibleDC(Some(dc));
        let bmp = CreateCompatibleBitmap(dc, 1, 1);
        let old = SelectObject(src, bmp.into());
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 140, AlphaFormat: 0 };
        for p in &parts {
            let (w, h) = rect_wh(p);
            if w > 0 && h > 0 {
                let _ = AlphaBlend(dc, p.left, p.top, w, h, src, 0, 0, 1, 1, blend);
            }
        }
        SelectObject(src, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(src);
    }
}

/// White text in the middle of `r`, word-wrapped.
pub fn center_text(dc: HDC, r: &RECT, text: &str, font: HFONT) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rc = *r;
    // SAFETY: valid DC and font; DrawTextW gets a valid buffer.
    unsafe {
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, rgb(255, 255, 255));
        // Measure the height, then center it vertically.
        let mut measure = rc;
        DrawTextW(dc, &mut wide, &mut measure, DT_CENTER | DT_WORDBREAK | DT_CALCRECT);
        let h = measure.bottom - measure.top;
        rc.top += ((r.bottom - r.top) - h).max(0) / 2;
        DrawTextW(dc, &mut wide, &mut rc, DT_CENTER | DT_WORDBREAK);
        SelectObject(dc, old);
    }
}
