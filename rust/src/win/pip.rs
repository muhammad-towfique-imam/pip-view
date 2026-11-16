//! The PiP window: the selected area of the camera image in a borderless
//! window that stays on top. Drag moves it (Windows does the move), the
//! bottom-right grip resizes it (the app does the resize), and right-click
//! or press-and-hold opens the menu.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::app::{apply, wide, with_app, App, Op, Page};
use super::draw;
use crate::border::{Border, BORDER_PX};
use crate::geometry::{crop_src, first_pip_rect, hit_test, keep_aspect, Hit};
use crate::log::log;
use crate::watchdog;

pub const CLASS: PCWSTR = w!("WebcamViewPipWindow");
/// The grip size and the minimum window size at 100 % scale.
const GRIP: i32 = 16;
const MIN_W: i32 = 64;
const MIN_H: i32 = 48;

const CMD_AREA: u32 = 1;
const CMD_CAMERA: u32 = 2;
const CMD_EXIT: u32 = 3;
const CMD_BORDER: u32 = 100;

/// A PiP window action that runs after the state borrow ends.
pub enum PipOp {
    /// Show the window at (x, y) with the size (w, h), in physical pixels.
    Show(i32, i32, i32, i32),
    /// Open the menu at the screen point.
    Menu(i32, i32),
    /// Close both windows and end the app.
    Exit,
}

pub fn register(hinst: HINSTANCE) {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinst,
        // SAFETY: a system cursor.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    // SAFETY: a valid class description.
    unsafe {
        RegisterClassExW(&wc);
    }
}

/// Makes the PiP window (hidden).
pub fn create(hinst: HINSTANCE) -> Option<HWND> {
    // SAFETY: the class is registered; the window belongs to this thread.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            CLASS,
            w!("webcam-view-pip"),
            WS_POPUP,
            0,
            0,
            320,
            180,
            None,
            None,
            Some(hinst),
            None,
        )
    }
    .ok()?;
    with_app(|a| a.pip = hwnd);
    Some(hwnd)
}

/// The virtual screen (all monitors) in physical pixels.
fn desktop() -> (f32, f32, f32, f32) {
    // SAFETY: GetSystemMetrics has no preconditions.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN) as f32,
            GetSystemMetrics(SM_YVIRTUALSCREEN) as f32,
            GetSystemMetrics(SM_CXVIRTUALSCREEN) as f32,
            GetSystemMetrics(SM_CYVIRTUALSCREEN) as f32,
        )
    }
}

/// The work area and the scale of the monitor at `pos` (or the primary monitor).
fn monitor_at(pos: Option<(f32, f32)>) -> ((f32, f32, f32, f32), f32) {
    let pt = pos.map_or(POINT { x: 0, y: 0 }, |(x, y)| POINT { x: x as i32, y: y as i32 });
    // SAFETY: valid structures; these calls do not send messages.
    unsafe {
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let r = info.rcWork;
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        (
            (r.left as f32, r.top as f32, (r.right - r.left) as f32, (r.bottom - r.top) as f32),
            dx as f32 / 96.0,
        )
    }
}

/// The aspect of the selected area in frame pixels, if a frame exists.
fn area_aspect(a: &App) -> Option<f32> {
    a.frame.as_ref().map(|f| a.area.aspect(f.width, f.height))
}

/// Shows the PiP page. Without a saved size and without a frame, the window
/// appears with the first frame (`pip_pending`).
pub fn show(a: &mut App) -> Vec<Op> {
    let aspect = area_aspect(a);
    let size = match (a.window_size, aspect) {
        (Some((w, _)), Some(asp)) => {
            let (w, h) = keep_aspect(w as i32, asp, 1, 1);
            Some((w as f32, h as f32))
        }
        (size, _) => size,
    };
    let (work, scale) = monitor_at(a.window_pos);
    log!("PiP show: saved pos {:?}, size {size:?}, aspect {aspect:?}, work {work:?}, scale {scale}", a.window_pos);
    match first_pip_rect(a.window_pos, size, aspect, desktop(), work, scale) {
        Some((x, y, w, h)) => {
            a.pip_pending = false;
            vec![Op::Pip(PipOp::Show(x, y, w, h)), Op::ShowPage(Page::Pip)]
        }
        None => {
            a.pip_pending = true;
            vec![Op::ShowPage(Page::Pip)]
        }
    }
}

pub fn run_op(op: PipOp) {
    match op {
        PipOp::Show(x, y, w, h) => {
            let Some(pip) = with_app(|a| a.pip) else { return };
            // SAFETY: valid window handle of this thread.
            unsafe {
                let _ = SetWindowPos(pip, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
            }
        }
        PipOp::Menu(x, y) => menu(x, y),
        PipOp::Exit => {
            let Some((pip, setup)) = with_app(|a| (a.pip, a.setup)) else { return };
            log!("exit from the menu");
            // SAFETY: valid window handles; destroying the setup window ends the loop.
            unsafe {
                let _ = DestroyWindow(pip);
                let _ = DestroyWindow(setup);
            }
        }
    }
}

fn menu(x: i32, y: i32) {
    let Some((pip, current)) = with_app(|a| (a.pip, a.border)) else { return };
    let item = |m: HMENU, id: u32, text: &str| {
        let t = wide(text);
        // SAFETY: valid menu and a null-terminated string.
        unsafe {
            let _ = AppendMenuW(m, MF_STRING, id as usize, PCWSTR(t.as_ptr()));
        }
    };
    // SAFETY: menus are made, shown, and destroyed here.
    let cmd = unsafe {
        let Ok(root) = CreatePopupMenu() else { return };
        let Ok(borders) = CreatePopupMenu() else { return };
        item(root, CMD_AREA, "Select area");
        item(root, CMD_CAMERA, "Select camera");
        for (i, b) in Border::ALL.iter().enumerate() {
            item(borders, CMD_BORDER + i as u32, b.label());
        }
        let selected = Border::ALL.iter().position(|b| *b == current).unwrap_or(0) as u32;
        let last = CMD_BORDER + Border::ALL.len() as u32 - 1;
        let _ = CheckMenuRadioItem(borders, CMD_BORDER, last, CMD_BORDER + selected, MF_BYCOMMAND.0);
        let t = wide("Border");
        let _ = AppendMenuW(root, MF_POPUP, borders.0 as usize, PCWSTR(t.as_ptr()));
        item(root, CMD_EXIT, "Exit");
        // The menu closes correctly only if the window is in the foreground.
        let _ = SetForegroundWindow(pip);
        let cmd = TrackPopupMenu(root, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, Some(0), pip, None);
        // The documented way to make the next click outside close the menu.
        let _ = PostMessageW(Some(pip), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(root);
        cmd.0 as u32
    };
    if cmd == 0 {
        return;
    }
    let ops = with_app(|a| command(a, cmd)).unwrap_or_default();
    apply(ops);
}

/// Reads the PiP position and size (physical pixels) into the state.
fn read_geometry(a: &mut App) {
    let mut r = RECT::default();
    // SAFETY: valid window handle; GetWindowRect sends no message.
    if unsafe { GetWindowRect(a.pip, &mut r) }.is_ok() && r.right > r.left {
        a.window_pos = Some((r.left as f32, r.top as f32));
        a.window_size = Some(((r.right - r.left) as f32, (r.bottom - r.top) as f32));
    }
}

/// Saves the PiP position and size.
fn save_geometry(a: &mut App) {
    if a.page == Page::Pip {
        read_geometry(a);
        a.save_config();
    }
}

fn command(a: &mut App, cmd: u32) -> Vec<Op> {
    log!("PiP menu command {cmd}");
    match cmd {
        CMD_AREA => {
            save_geometry(a);
            vec![
                Op::ShowPage(Page::Area),
                Op::Text(a.controls.hint, "Drag on the image to select the area.".into()),
            ]
        }
        CMD_CAMERA => {
            save_geometry(a);
            a.open_camera_page(None)
        }
        CMD_EXIT => {
            save_geometry(a);
            vec![Op::Pip(PipOp::Exit)]
        }
        c if (CMD_BORDER..CMD_BORDER + Border::ALL.len() as u32).contains(&c) => {
            a.border = Border::ALL[(c - CMD_BORDER) as usize];
            save_geometry(a);
            vec![Op::Invalidate(a.pip)]
        }
        _ => Vec::new(),
    }
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: BeginPaint and EndPaint in pairs on our window.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let mut client = RECT::default();
    // SAFETY: valid window handle.
    let (scale, _) = unsafe {
        let _ = GetClientRect(hwnd, &mut client);
        (GetDpiForWindow(hwnd) as f32 / 96.0, ())
    };
    with_app(|a| {
        let (w, h) = draw::rect_wh(&client);
        let buffer = draw::BackBuffer::new(hdc, w, h);
        draw::fill(buffer.dc, &client, draw::rgb(0, 0, 0));
        if let Some(frame) = &a.frame {
            let dest = draw::letterbox(&client, a.area.aspect(frame.width, frame.height));
            draw::paint_frame(buffer.dc, &dest, frame, crop_src(a.area, frame.width, frame.height));
        }
        let show_text = a.frame.is_none() || !a.status_text.is_empty();
        if show_text {
            let text = if a.status_text.is_empty() { "Starting the camera…" } else { a.status_text.as_str() };
            draw::center_text(buffer.dc, &client, text, a.font);
        }
        if let Some((r, g, b)) = a.border.rgb() {
            draw::border(buffer.dc, &client, draw::rgb(r, g, b), BORDER_PX as i32);
        }
        draw::grip_mark(buffer.dc, &client, scale);
        buffer.blit(hdc, 0, 0);
    });
    // SAFETY: pairs with BeginPaint.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

/// The x and y of a mouse message (signed, for multiple monitors).
fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

/// The state of a resize from the grip: the window rectangle at the start and
/// the pointer start point in screen coordinates.
#[derive(Clone, Copy)]
struct Resize {
    left: i32,
    top: i32,
    width: i32,
    start_x: i32,
}

thread_local! {
    static RESIZE: std::cell::Cell<Option<Resize>> = const { std::cell::Cell::new(None) };
}

fn cursor_screen() -> POINT {
    let mut p = POINT::default();
    // SAFETY: valid out pointer.
    let _ = unsafe { GetCursorPos(&mut p) };
    p
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    watchdog::enter_message(msg);
    // SAFETY: the window procedure contract.
    let result = unsafe { handle(hwnd, msg, wp, lp) };
    watchdog::leave_message();
    result
}

unsafe fn handle(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let dpi_scale = |v: i32| unsafe { v * GetDpiForWindow(hwnd) as i32 / 96 };
    match msg {
        WM_ENTERSIZEMOVE | WM_ENTERMENULOOP => watchdog::set_modal(true),
        WM_EXITMENULOOP => watchdog::set_modal(false),
        WM_EXITSIZEMOVE => {
            watchdog::set_modal(false);
            with_app(save_geometry);
        }
        WM_NCHITTEST => {
            let (sx, sy) = xy(lp);
            let mut p = POINT { x: sx, y: sy };
            let mut client = RECT::default();
            unsafe {
                let _ = ScreenToClient(hwnd, &mut p);
                let _ = GetClientRect(hwnd, &mut client);
            }
            return match hit_test(p.x, p.y, client.right, client.bottom, dpi_scale(GRIP)) {
                // The app resizes from the grip (see WM_LBUTTONDOWN).
                Hit::Resize => LRESULT(HTCLIENT as isize),
                Hit::Move => LRESULT(HTCAPTION as isize),
            };
        }
        WM_SETCURSOR => {
            let p = cursor_screen();
            let mut q = p;
            let mut client = RECT::default();
            unsafe {
                let _ = ScreenToClient(hwnd, &mut q);
                let _ = GetClientRect(hwnd, &mut client);
            }
            if hit_test(q.x, q.y, client.right, client.bottom, dpi_scale(GRIP)) == Hit::Resize {
                unsafe {
                    SetCursor(LoadCursorW(None, IDC_SIZENWSE).ok());
                }
                return LRESULT(1);
            }
        }
        WM_LBUTTONDOWN => {
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(hwnd, &mut r);
                SetCapture(hwnd);
            }
            RESIZE.set(Some(Resize { left: r.left, top: r.top, width: r.right - r.left, start_x: cursor_screen().x }));
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if let Some(rs) = RESIZE.get() {
                let aspect = with_app(|a| area_aspect(a)).flatten().unwrap_or(16.0 / 9.0);
                let want = rs.width + cursor_screen().x - rs.start_x;
                let (w, h) = keep_aspect(want, aspect, dpi_scale(MIN_W), dpi_scale(MIN_H));
                unsafe {
                    let _ = SetWindowPos(hwnd, None, rs.left, rs.top, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                return LRESULT(0);
            }
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED => {
            if RESIZE.take().is_some() {
                if msg == WM_LBUTTONUP {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                }
                with_app(save_geometry);
                return LRESULT(0);
            }
        }
        // A right-click on the caption area (most of the window) is the
        // system-menu gesture for DefWindowProc, which then sends no
        // WM_CONTEXTMENU. Handle the right button here: the press does
        // nothing, the release opens the menu (also for press-and-hold).
        WM_NCRBUTTONDOWN => return LRESULT(0),
        WM_NCRBUTTONUP => {
            let (x, y) = xy(lp);
            log!("PiP menu: right button on the caption at {x},{y}");
            apply(vec![Op::Pip(PipOp::Menu(x, y))]);
            return LRESULT(0);
        }
        // The grip corner is client area.
        WM_RBUTTONUP => {
            let (cx, cy) = xy(lp);
            let mut p = POINT { x: cx, y: cy };
            unsafe {
                let _ = ClientToScreen(hwnd, &mut p);
            }
            log!("PiP menu: right button in the grip at {},{}", p.x, p.y);
            apply(vec![Op::Pip(PipOp::Menu(p.x, p.y))]);
            return LRESULT(0);
        }
        WM_CONTEXTMENU => {
            log!("PiP menu: WM_CONTEXTMENU");
            let (mut x, mut y) = xy(lp);
            if x == -1 && y == -1 {
                // From the keyboard: open the menu in the middle of the window.
                let mut r = RECT::default();
                unsafe {
                    let _ = GetWindowRect(hwnd, &mut r);
                }
                (x, y) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            }
            apply(vec![Op::Pip(PipOp::Menu(x, y))]);
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            return LRESULT(0);
        }
        // Keep the physical size when the window moves to a monitor with a
        // different scale: the saved geometry is in physical pixels.
        WM_DPICHANGED => {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            return LRESULT(0);
        }
        WM_CLOSE => {
            let ops = with_app(|a| {
                save_geometry(a);
                vec![Op::Pip(PipOp::Exit)]
            })
            .unwrap_or_default();
            apply(ops);
            return LRESULT(0);
        }
        WM_ENDSESSION => {
            with_app(save_geometry);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}
