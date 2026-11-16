//! The setup window: the camera page (camera and stream lists) and the area
//! page (the live image with the selection).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;

use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::app::{apply, with_app, wide, Op, Page};
use super::capture::{WM_APP_FRAME, WM_APP_STATUS};
use super::draw;
use crate::geometry::crop_src;
use crate::layout::{layout_setup, R};
use crate::log::log;
use crate::watchdog;

pub const CLASS: PCWSTR = w!("WebcamViewPipSetup");
const TIMER_ID: usize = 1;

/// Static control styles (in `Win32_System_SystemServices`, not imported for 2 values).
const SS_LEFT: u32 = 0x0;
const SS_CENTERIMAGE: u32 = 0x200;

const ID_CAM_LIST: i32 = 101;
const ID_STREAM_LIST: i32 = 102;
const ID_NEXT: i32 = 103;
const ID_BACK: i32 = 104;
const ID_FULL: i32 = 105;
const ID_CONFIRM: i32 = 106;

fn loword(v: usize) -> i32 {
    (v & 0xFFFF) as i32
}

fn hiword(v: usize) -> u32 {
    ((v >> 16) & 0xFFFF) as u32
}

/// The x and y of a mouse message (signed, for multiple monitors).
fn mouse_xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

pub fn register(hinst: HINSTANCE) {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(proc),
        hInstance: hinst,
        // SAFETY: a system cursor.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as usize as *mut _),
        lpszClassName: CLASS,
        ..Default::default()
    };
    // SAFETY: a valid class description.
    unsafe {
        RegisterClassExW(&wc);
    }
}

fn child(parent: HWND, hinst: HINSTANCE, class: PCWSTR, text: &str, style: WINDOW_STYLE, id: i32) -> HWND {
    let t = wide(text);
    // SAFETY: valid parent and class; the id goes in the menu parameter of a child.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(t.as_ptr()),
            WS_CHILD | style,
            0,
            0,
            1,
            1,
            Some(parent),
            Some(HMENU(id as usize as *mut _)),
            Some(hinst),
            None,
        )
    }
    .unwrap_or_default()
}

/// Makes the child controls of the setup window.
pub fn create_controls(parent: HWND, hinst: HINSTANCE) -> super::app::Controls {
    let list = WS_VSCROLL | WS_BORDER | WS_TABSTOP | WINDOW_STYLE((LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32);
    let button = WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32);
    let label = WINDOW_STYLE(SS_LEFT);
    super::app::Controls {
        cam_label: child(parent, hinst, w!("STATIC"), "Select camera", label, 0),
        cam_list: child(parent, hinst, w!("LISTBOX"), "", list, ID_CAM_LIST),
        stream_label: child(parent, hinst, w!("STATIC"), "Stream", label, 0),
        stream_list: child(parent, hinst, w!("LISTBOX"), "", list, ID_STREAM_LIST),
        status: child(parent, hinst, w!("STATIC"), "", label | WINDOW_STYLE(SS_CENTERIMAGE), 0),
        next: child(parent, hinst, w!("BUTTON"), "Next", button, ID_NEXT),
        hint: child(parent, hinst, w!("STATIC"), "Drag on the image to select the area.", label, 0),
        back: child(parent, hinst, w!("BUTTON"), "Back", button, ID_BACK),
        full: child(parent, hinst, w!("BUTTON"), "Full image", button, ID_FULL),
        confirm: child(parent, hinst, w!("BUTTON"), "Confirm", button, ID_CONFIRM),
    }
}

/// The message font of the system for `dpi`.
pub fn message_font(dpi: u32) -> HFONT {
    use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;
    let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
    // SAFETY: a valid NONCLIENTMETRICSW with its size set.
    unsafe {
        let _ = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut _),
            0,
            dpi,
        );
        CreateFontIndirectW(&ncm.lfMessageFont)
    }
}

/// Moves the controls to the layout for the current size and sets the font.
pub fn relayout(hwnd: HWND) {
    let Some((controls, font, dpi)) = with_app(|a| (a.controls, a.font, a.dpi)) else { return };
    let mut rc = RECT::default();
    // SAFETY: valid window handles of this thread.
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
        let l = layout_setup(rc.right, rc.bottom, dpi);
        let place = |h: HWND, r: R| {
            let _ = MoveWindow(h, r.x, r.y, r.w, r.h, true);
            SendMessageW(h, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        };
        place(controls.cam_label, l.cam_label);
        place(controls.cam_list, l.cam_list);
        place(controls.stream_label, l.stream_label);
        place(controls.stream_list, l.stream_list);
        place(controls.status, l.status);
        place(controls.next, l.next);
        place(controls.hint, l.hint);
        place(controls.back, l.back);
        place(controls.full, l.full);
        place(controls.confirm, l.confirm);
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

fn client_rect(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    // SAFETY: valid window handle.
    let _ = unsafe { GetClientRect(hwnd, &mut rc) };
    rc
}

/// Paints the camera image with the selection on the area page.
fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint and EndPaint in pairs on our window.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let client = client_rect(hwnd);
    with_app(|a| {
        if a.page != Page::Area {
            return;
        }
        let l = layout_setup(client.right, client.bottom, a.dpi);
        let box_rc = RECT { left: 0, top: 0, right: l.image.w, bottom: l.image.h };
        let buffer = draw::BackBuffer::new(hdc, l.image.w, l.image.h);
        draw::fill(buffer.dc, &box_rc, draw::rgb(0, 0, 0));
        match &a.frame {
            Some(frame) => {
                let img = draw::letterbox(&box_rc, frame.width as f32 / frame.height.max(1) as f32);
                draw::paint_frame(buffer.dc, &img, frame, crop_src(crate::geometry::NormRect::FULL, frame.width, frame.height));
                let (iw, ih) = draw::rect_wh(&img);
                let s = a.area;
                let sel = RECT {
                    left: img.left + (s.x * iw as f32).round() as i32,
                    top: img.top + (s.y * ih as f32).round() as i32,
                    right: img.left + ((s.x + s.w) * iw as f32).round() as i32,
                    bottom: img.top + ((s.y + s.h) * ih as f32).round() as i32,
                };
                draw::dim_outside(buffer.dc, &img, &sel);
                draw::outline(buffer.dc, &sel, draw::rgb(255, 255, 255));
            }
            None => {
                let text = if a.status_text.is_empty() { "Starting the camera…" } else { a.status_text.as_str() };
                draw::center_text(buffer.dc, &box_rc, text, a.font);
            }
        }
        buffer.blit(hdc, l.image.x, l.image.y);
    });
    // SAFETY: pairs with BeginPaint.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    watchdog::enter_message(msg);
    // SAFETY: the window procedure contract; handle() only makes Win32 calls on our windows.
    let result = unsafe { handle(hwnd, msg, wp, lp) };
    watchdog::leave_message();
    result
}

unsafe fn handle(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ops: Vec<Op> = match msg {
        WM_ENTERSIZEMOVE | WM_ENTERMENULOOP => {
            watchdog::set_modal(true);
            Vec::new()
        }
        WM_EXITSIZEMOVE | WM_EXITMENULOOP => {
            watchdog::set_modal(false);
            Vec::new()
        }
        WM_SIZE => {
            relayout(hwnd);
            Vec::new()
        }
        WM_TIMER if wp.0 == TIMER_ID => with_app(|a| {
            let mut ops = a.poll_jobs();
            ops.extend(a.check_capture());
            ops
        })
        .unwrap_or_default(),
        WM_COMMAND => {
            let id = loword(wp.0);
            let code = hiword(wp.0);
            let selected = || {
                // SAFETY: lp is the handle of the list box that sent the notification.
                let i = unsafe { SendMessageW(HWND(lp.0 as *mut _), LB_GETCURSEL, None, None) }.0;
                (i >= 0).then_some(i as usize)
            };
            match (id, code) {
                (ID_CAM_LIST, LBN_SELCHANGE) => match selected() {
                    Some(i) => with_app(|a| a.select_camera(i)).unwrap_or_default(),
                    None => Vec::new(),
                },
                (ID_STREAM_LIST, LBN_SELCHANGE) => match selected() {
                    Some(i) => with_app(|a| a.select_stream(i)).unwrap_or_default(),
                    None => Vec::new(),
                },
                (ID_NEXT, BN_CLICKED) => with_app(|a| a.next()).unwrap_or_default(),
                (ID_BACK, BN_CLICKED) => with_app(|a| a.back()).unwrap_or_default(),
                (ID_FULL, BN_CLICKED) => with_app(|a| a.full_image()).unwrap_or_default(),
                (ID_CONFIRM, BN_CLICKED) => with_app(|a| a.confirm()).unwrap_or_default(),
                _ => Vec::new(),
            }
        }
        WM_APP_FRAME => with_app(|a| a.on_frame()).unwrap_or_default(),
        WM_APP_STATUS => with_app(|a| a.on_status()).unwrap_or_default(),
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lp);
            let client = client_rect(hwnd);
            with_app(|a| if a.page == Page::Area { a.mouse_down(&client, x, y) } else { Vec::new() })
                .unwrap_or_default()
        }
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lp);
            let client = client_rect(hwnd);
            with_app(|a| a.mouse_move(&client, x, y)).unwrap_or_default()
        }
        WM_LBUTTONUP => with_app(|a| a.mouse_up()).unwrap_or_default(),
        WM_PAINT => {
            paint(hwnd);
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            let dpi = loword(wp.0) as u32;
            let old_font = with_app(|a| {
                a.dpi = dpi;
                std::mem::replace(&mut a.font, message_font(dpi))
            });
            if let Some(f) = old_font {
                let _ = unsafe { DeleteObject(f.into()) };
            }
            // SAFETY: lp points to the suggested window rectangle.
            let r = unsafe { &*(lp.0 as *const RECT) };
            let _ = unsafe {
                SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE)
            };
            relayout(hwnd);
            return LRESULT(0);
        }
        WM_CLOSE => {
            log!("setup window closed: exit");
            let _ = unsafe { DestroyWindow(hwnd) };
            return LRESULT(0);
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    };
    apply(ops);
    if matches!(msg, WM_TIMER | WM_COMMAND | WM_APP_FRAME | WM_APP_STATUS) {
        LRESULT(0)
    } else {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
}

/// Makes the setup window (hidden) and its controls.
pub fn create(hinst: HINSTANCE) -> Option<HWND> {
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    // SAFETY: GetDpiForSystem has no preconditions.
    let dpi = unsafe { GetDpiForSystem() };
    let scale = |v: i32| v * dpi as i32 / 96;
    // SAFETY: the class is registered; the window belongs to this thread.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("webcam-view-pip"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(800),
            scale(600),
            None,
            None,
            Some(hinst),
            None,
        )
    }
    .ok()?;
    let controls = create_controls(hwnd, hinst);
    // SAFETY: valid window handle.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    with_app(|a| {
        a.setup = hwnd;
        a.controls = controls;
        a.dpi = dpi;
        a.font = message_font(dpi);
    });
    relayout(hwnd);
    // SAFETY: valid window handle.
    unsafe {
        SetTimer(Some(hwnd), TIMER_ID, 100, None);
    }
    Some(hwnd)
}
