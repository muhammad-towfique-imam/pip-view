//! The app state on the UI thread and the logic of the pages.
//!
//! Rule: the state is in a `RefCell`. A Win32 call can send a message to our
//! window procedure before it returns, so no Win32 call that changes windows
//! is made while the state is borrowed. The page logic returns `Op`s, and
//! `apply` runs them after the borrow ends.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::capture::{Capture, Status};
use super::mf::{self, Device, Frame};
use crate::border::Border;
use crate::config::{self, Config};
use crate::geometry::NormRect;
use crate::log::log;
use crate::modes::{default_stream, stream_label, StreamInfo};
use crate::worker::{alive, Job};

/// The time limit for device listing and stream listing.
const JOB_LIMIT: Duration = Duration::from_secs(5);
const DEVICE_JOB: &str = "list-devices";
const STREAM_JOB: &str = "list-streams";

pub const NOT_RESPONDING: &str = "The camera does not respond. Restart the app.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Camera,
    Area,
    Pip,
}

/// The child controls of the setup window.
#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub cam_label: HWND,
    pub cam_list: HWND,
    pub stream_label: HWND,
    pub stream_list: HWND,
    pub status: HWND,
    pub next: HWND,
    pub hint: HWND,
    pub back: HWND,
    pub full: HWND,
    pub confirm: HWND,
}

impl Controls {
    pub fn camera_page(&self) -> [HWND; 6] {
        [self.cam_label, self.cam_list, self.stream_label, self.stream_list, self.status, self.next]
    }

    pub fn area_page(&self) -> [HWND; 4] {
        [self.hint, self.back, self.full, self.confirm]
    }
}

/// A window operation that runs after the state borrow ends.
pub enum Op {
    Text(HWND, String),
    /// Replace the items of a list box and select 1.
    List(HWND, Vec<String>, Option<usize>),
    Enable(HWND, bool),
    Invalidate(HWND),
    /// Show the page: the windows and the controls of the page.
    ShowPage(Page),
    Capture(HWND),
    Release,
    /// Run a PiP window action (Task 4).
    Pip(super::pip::PipOp),
}

/// A job that lists something, with its start time.
struct Pending<T> {
    job: Job<anyhow::Result<T>>,
    started: Instant,
    reported: bool,
}

impl<T: Send + 'static> Pending<T> {
    fn new(name: &str, f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Pending<T> {
        Pending { job: Job::spawn(name, f), started: Instant::now(), reported: false }
    }
}

pub struct App {
    pub config_path: Option<PathBuf>,
    pub saved: Option<Config>,
    pub page: Page,
    pub setup: HWND,
    pub pip: HWND,
    pub controls: Controls,
    pub font: HFONT,
    pub dpi: u32,

    devices: Vec<Device>,
    device_job: Option<Pending<Vec<Device>>>,
    streams: Vec<StreamInfo>,
    /// The stream index of each list item (only color streams are listed).
    listed: Vec<u32>,
    stream_job: Option<(Pending<Vec<StreamInfo>>, String)>,
    pub device: Option<Device>,
    pub stream: Option<u32>,

    pub area: NormRect,
    drag: Option<(f32, f32)>,
    pub capture: Option<Capture>,
    pub frame: Option<Arc<Frame>>,
    pub status_text: String,

    pub border: Border,
    /// PiP position and size in physical pixels.
    pub window_pos: Option<(f32, f32)>,
    pub window_size: Option<(f32, f32)>,
    /// The PiP window waits for the first frame to get its size.
    pub pip_pending: bool,
    /// The app start opens the saved camera and stream directly in the PiP.
    auto_start: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn install(app: App) {
    APP.with(|a| *a.borrow_mut() = Some(app));
}

/// Runs `f` with the state. Returns `None` if the state is already borrowed
/// (a message that came during a Win32 call of an outer handler).
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| {
        let mut guard = a.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

/// A null-terminated UTF-16 string.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn set_text(hwnd: HWND, text: &str) {
    let w = wide(text);
    // SAFETY: valid window and a null-terminated string.
    let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(w.as_ptr())) };
}

/// Runs window operations. Call it only without a state borrow.
pub fn apply(ops: Vec<Op>) {
    for op in ops {
        // SAFETY: Win32 calls on windows of this thread with valid arguments.
        unsafe {
            match op {
                Op::Text(h, t) => set_text(h, &t),
                Op::List(h, items, sel) => {
                    SendMessageW(h, LB_RESETCONTENT, None, None);
                    for item in &items {
                        let w = wide(item);
                        SendMessageW(h, LB_ADDSTRING, None, Some(LPARAM(w.as_ptr() as isize)));
                    }
                    let sel = sel.map_or(usize::MAX, |s| s);
                    SendMessageW(h, LB_SETCURSEL, Some(WPARAM(sel)), None);
                }
                Op::Enable(h, on) => {
                    let _ = EnableWindow(h, on);
                }
                Op::Invalidate(h) => {
                    let _ = InvalidateRect(Some(h), None, false);
                }
                Op::ShowPage(page) => show_page(page),
                Op::Capture(h) => {
                    SetCapture(h);
                }
                Op::Release => {
                    let _ = ReleaseCapture();
                }
                Op::Pip(p) => super::pip::run_op(p),
            }
        }
    }
}

fn show_page(page: Page) {
    let Some((controls, setup, pip)) = with_app(|a| {
        a.page = page;
        (a.controls, a.setup, a.pip)
    }) else {
        return;
    };
    let show = |h: HWND, on: bool| {
        // SAFETY: valid window handles of this thread.
        unsafe {
            let _ = ShowWindow(h, if on { SW_SHOW } else { SW_HIDE });
        }
    };
    for h in controls.camera_page() {
        show(h, page == Page::Camera);
    }
    for h in controls.area_page() {
        show(h, page == Page::Area);
    }
    show(setup, page != Page::Pip);
    if page != Page::Pip && !pip.is_invalid() {
        show(pip, false);
    }
    // SAFETY: valid window handle.
    unsafe {
        let _ = InvalidateRect(Some(setup), None, true);
    }
}

impl App {
    pub fn new(config_path: Option<PathBuf>, saved: Option<Config>) -> App {
        let area = saved.as_ref().map_or(NormRect::FULL, |c| c.area);
        let border = saved.as_ref().map_or(Border::None, |c| c.border);
        let (window_pos, window_size) = saved.as_ref().map_or((None, None), |c| (c.window_pos, c.window_size));
        App {
            config_path,
            saved,
            page: Page::Camera,
            setup: HWND::default(),
            pip: HWND::default(),
            controls: Controls::default(),
            font: HFONT::default(),
            dpi: 96,
            devices: Vec::new(),
            device_job: None,
            streams: Vec::new(),
            listed: Vec::new(),
            stream_job: None,
            device: None,
            stream: None,
            area,
            drag: None,
            capture: None,
            frame: None,
            status_text: String::new(),
            border,
            window_pos,
            window_size,
            pip_pending: false,
            auto_start: false,
        }
    }

    /// At app start with a saved config: show the PiP at once with "Starting
    /// the camera…", and find the saved camera and stream in the background.
    /// Without a saved size, use the camera page (the PiP size needs a frame).
    pub fn begin(&mut self) -> Vec<Op> {
        let can_auto = self.saved.as_ref().is_some_and(|c| c.window_size.is_some());
        if !can_auto {
            return self.open_camera_page(None);
        }
        log!("start: open the saved camera in the PiP");
        self.auto_start = true;
        self.status_text = "Starting the camera…".into();
        let mut ops = self.start_device_job();
        ops.extend(super::pip::show(self));
        ops
    }

    /// The automatic start failed: show the camera page with the reason.
    fn auto_failed(&mut self, message: &str) -> Vec<Op> {
        log!("start: {message}");
        self.auto_start = false;
        let mut ops = vec![Op::ShowPage(Page::Camera)];
        ops.push(self.status(message));
        ops
    }

    pub fn save_config(&self) {
        let (Some(path), Some(device)) = (&self.config_path, &self.device) else { return };
        let cfg = Config {
            camera_name: device.name.clone(),
            camera_id: Some(device.id.clone()),
            stream: self.stream,
            area: self.area,
            window_pos: self.window_pos,
            window_size: self.window_size,
            border: self.border,
        };
        if let Err(e) = config::save(path, &cfg) {
            log!("cannot save config: {e:#}");
        }
    }

    fn status(&mut self, text: &str) -> Op {
        self.status_text = text.to_string();
        Op::Text(self.controls.status, text.to_string())
    }

    fn next_enabled(&self) -> Op {
        Op::Enable(self.controls.next, self.device.is_some() && self.stream.is_some())
    }

    // ----- Camera page -----

    /// Shows the camera page and reads the camera list again.
    pub fn open_camera_page(&mut self, message: Option<String>) -> Vec<Op> {
        self.capture = None;
        self.frame = None;
        let mut ops = vec![Op::ShowPage(Page::Camera)];
        ops.extend(self.start_device_job());
        if let Some(m) = message {
            ops.push(self.status(&m));
        }
        ops
    }

    pub fn start_device_job(&mut self) -> Vec<Op> {
        if alive(DEVICE_JOB) > 0 {
            return vec![self.status(NOT_RESPONDING)];
        }
        self.device_job = Some(Pending::new(DEVICE_JOB, mf::list_devices));
        vec![self.status("Reading cameras…"), Op::List(self.controls.cam_list, Vec::new(), None)]
    }

    fn start_stream_job(&mut self, device: &Device) -> Vec<Op> {
        self.streams.clear();
        self.listed.clear();
        self.stream = None;
        let mut ops = vec![Op::List(self.controls.stream_list, Vec::new(), None), self.next_enabled()];
        if alive(STREAM_JOB) > 0 {
            ops.push(self.status(NOT_RESPONDING));
            return ops;
        }
        let id = device.id.clone();
        let job_id = id.clone();
        self.stream_job = Some((Pending::new(STREAM_JOB, move || mf::probe_streams(&job_id)), id));
        ops.push(self.status("Reading streams…"));
        ops
    }

    /// Called by the 100 ms timer: takes the results of the listing jobs.
    pub fn poll_jobs(&mut self) -> Vec<Op> {
        let mut ops = Vec::new();
        if let Some(p) = &mut self.device_job {
            if let Some(result) = p.job.poll() {
                log!("camera list in {} ms", p.started.elapsed().as_millis());
                self.device_job = None;
                ops.extend(self.devices_ready(result));
            } else if !p.reported && p.started.elapsed() > JOB_LIMIT {
                p.reported = true;
                log!("camera list: no result in {} s", JOB_LIMIT.as_secs());
                ops.push(self.status("The camera does not respond."));
            }
        }
        if let Some((p, id)) = &mut self.stream_job {
            if let Some(result) = p.job.poll() {
                log!("stream list in {} ms", p.started.elapsed().as_millis());
                let id = id.clone();
                self.stream_job = None;
                ops.extend(self.streams_ready(result, &id));
            } else if !p.reported && p.started.elapsed() > JOB_LIMIT {
                p.reported = true;
                log!("stream list: no result in {} s", JOB_LIMIT.as_secs());
                ops.push(self.status("The camera does not respond."));
            }
        }
        ops
    }

    fn devices_ready(&mut self, result: anyhow::Result<Vec<Device>>) -> Vec<Op> {
        match result {
            Err(e) => {
                log!("camera list failed: {e:#}");
                if self.auto_start {
                    return self.auto_failed("Cannot read the cameras. Restart the app.");
                }
                vec![self.status("Cannot read the cameras. Restart the app.")]
            }
            Ok(devices) if devices.is_empty() => {
                self.devices.clear();
                if self.auto_start {
                    return self.auto_failed("No camera found. Connect a camera and restart the app.");
                }
                vec![self.status("No camera found. Connect a camera and restart the app.")]
            }
            Ok(devices) => {
                let pairs: Vec<(String, String)> = devices.iter().map(|d| (d.name.clone(), d.id.clone())).collect();
                let saved = self.saved.as_ref();
                let found = saved.and_then(|c| config::find_camera(&pairs, c.camera_id.as_deref(), &c.camera_name));
                let mut early = Vec::new();
                if self.auto_start && found.is_none() {
                    early = self.auto_failed("The saved camera is not connected. Select a camera.");
                }
                let index = found.unwrap_or(0);
                let names = devices.iter().map(|d| d.name.clone()).collect();
                self.devices = devices;
                let mut ops = early;
                ops.push(Op::List(self.controls.cam_list, names, Some(index)));
                ops.extend(self.select_camera(index));
                ops
            }
        }
    }

    /// The user (or the start) selects the camera at `index` of the list.
    pub fn select_camera(&mut self, index: usize) -> Vec<Op> {
        let Some(device) = self.devices.get(index).cloned() else { return Vec::new() };
        log!("camera selected: \"{}\"", device.name);
        self.device = Some(device.clone());
        self.start_stream_job(&device)
    }

    fn streams_ready(&mut self, result: anyhow::Result<Vec<StreamInfo>>, id: &str) -> Vec<Op> {
        if self.device.as_ref().map(|d| d.id.as_str()) != Some(id) {
            return Vec::new(); // An old result for a camera that is no longer selected.
        }
        let streams = match result {
            Ok(s) => s,
            Err(e) => {
                log!("stream list failed: {e:#}");
                let message = "The camera does not open. It can be in use by a different app.";
                if self.auto_start {
                    return self.auto_failed(message);
                }
                return vec![self.status(message)];
            }
        };
        let color: Vec<&StreamInfo> = streams.iter().filter(|s| s.color).collect();
        if color.is_empty() {
            self.streams = streams;
            if self.auto_start {
                return self.auto_failed("This camera has no color stream.");
            }
            return vec![self.status("This camera has no color stream."), self.next_enabled()];
        }
        let saved_stream = self
            .saved
            .as_ref()
            .filter(|c| c.camera_id.as_deref() == Some(id))
            .and_then(|c| c.stream);
        let chosen = default_stream(&streams, saved_stream);
        self.listed = color.iter().map(|s| s.index).collect();
        let labels = color.iter().map(|s| stream_label(s)).collect();
        let sel = chosen.and_then(|c| self.listed.iter().position(|i| *i == c));
        self.stream = chosen;
        self.streams = streams;
        log!("default stream: {chosen:?}");
        let ops = vec![Op::List(self.controls.stream_list, labels, sel), self.status(""), self.next_enabled()];
        if self.auto_start {
            self.auto_start = false;
            if let (Some(device), Some(stream)) = (self.device.clone(), chosen) {
                log!("start: capture \"{}\" stream {stream}", device.name);
                self.status_text = "Starting the camera…".into();
                self.start_capture(device, stream);
            }
        }
        ops
    }

    pub fn select_stream(&mut self, list_index: usize) -> Vec<Op> {
        self.stream = self.listed.get(list_index).copied();
        log!("stream selected: {:?}", self.stream);
        vec![self.next_enabled()]
    }

    /// Next: start the capture and show the area page.
    pub fn next(&mut self) -> Vec<Op> {
        let (Some(device), Some(stream)) = (self.device.clone(), self.stream) else { return Vec::new() };
        let same = self
            .saved
            .as_ref()
            .is_some_and(|c| c.camera_id.as_deref() == Some(device.id.as_str()) && c.stream == Some(stream));
        if !same {
            self.area = NormRect::FULL;
        }
        self.start_capture(device, stream);
        vec![Op::ShowPage(Page::Area), Op::Text(self.controls.hint, "Starting the camera…".into())]
    }

    pub fn start_capture(&mut self, device: Device, stream: u32) {
        self.capture = None; // Stop the old capture first.
        self.frame = None;
        self.capture = Some(Capture::start(device, stream, self.setup));
    }

    // ----- Area page -----

    /// The letterboxed camera image in setup window client coordinates.
    pub fn image_rect(&self, client: &RECT) -> Option<RECT> {
        let frame = self.frame.as_ref()?;
        let l = crate::layout::layout_setup(client.right, client.bottom, self.dpi);
        let area = RECT { left: l.image.x, top: l.image.y, right: l.image.x + l.image.w, bottom: l.image.y + l.image.h };
        Some(super::draw::letterbox(&area, frame.width as f32 / frame.height.max(1) as f32))
    }

    fn norm_point(img: &RECT, x: i32, y: i32) -> (f32, f32) {
        let (w, h) = super::draw::rect_wh(img);
        ((x - img.left) as f32 / w.max(1) as f32, (y - img.top) as f32 / h.max(1) as f32)
    }

    pub fn mouse_down(&mut self, client: &RECT, x: i32, y: i32) -> Vec<Op> {
        let Some(img) = self.image_rect(client) else { return Vec::new() };
        if x < img.left || x >= img.right || y < img.top || y >= img.bottom {
            return Vec::new();
        }
        self.drag = Some(Self::norm_point(&img, x, y));
        vec![Op::Capture(self.setup)]
    }

    pub fn mouse_move(&mut self, client: &RECT, x: i32, y: i32) -> Vec<Op> {
        let (Some(start), Some(img)) = (self.drag, self.image_rect(client)) else { return Vec::new() };
        self.area = NormRect::from_drag(start, Self::norm_point(&img, x, y));
        vec![Op::Invalidate(self.setup)]
    }

    pub fn mouse_up(&mut self) -> Vec<Op> {
        if self.drag.take().is_some() {
            vec![Op::Release]
        } else {
            Vec::new()
        }
    }

    pub fn full_image(&mut self) -> Vec<Op> {
        self.area = NormRect::FULL;
        vec![Op::Invalidate(self.setup)]
    }

    pub fn back(&mut self) -> Vec<Op> {
        self.capture = None;
        self.frame = None;
        vec![Op::ShowPage(Page::Camera)]
    }

    pub fn confirm(&mut self) -> Vec<Op> {
        if self.frame.is_none() {
            return Vec::new();
        }
        log!("confirm: area {:?}", self.area);
        self.save_config();
        super::pip::show(self)
    }

    // ----- Capture messages -----

    pub fn on_frame(&mut self) -> Vec<Op> {
        let Some(capture) = &self.capture else { return Vec::new() };
        if let Some(f) = capture.latest() {
            self.frame = Some(f);
        }
        match self.page {
            Page::Area => vec![Op::Invalidate(self.setup)],
            Page::Pip if self.pip_pending => super::pip::show(self),
            Page::Pip => vec![Op::Invalidate(self.pip)],
            Page::Camera => Vec::new(),
        }
    }

    pub fn on_status(&mut self) -> Vec<Op> {
        let Some(capture) = &self.capture else { return Vec::new() };
        let status = capture.status();
        let text = match &status {
            Status::Starting => "Starting the camera…".to_string(),
            Status::Running(_) => String::new(),
            Status::Lost(t) => t.clone(),
            Status::NotResponding => NOT_RESPONDING.to_string(),
            Status::Failed(t) => {
                let message = t.clone();
                return self.open_camera_page(Some(message));
            }
        };
        self.status_text = text.clone();
        match self.page {
            Page::Area => {
                let hint = if text.is_empty() { "Drag on the image to select the area.".to_string() } else { text };
                vec![Op::Text(self.controls.hint, hint), Op::Invalidate(self.setup)]
            }
            Page::Pip => vec![Op::Invalidate(self.pip)],
            Page::Camera => Vec::new(),
        }
    }

    /// The 100 ms timer also checks a start that takes too long.
    pub fn check_capture(&mut self) -> Vec<Op> {
        let not_responding = self.capture.as_ref().is_some_and(|c| c.status() == Status::NotResponding);
        if not_responding && self.status_text != NOT_RESPONDING {
            return self.on_status();
        }
        Vec::new()
    }
}
