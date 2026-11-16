//! The app window: camera select, area select, and PiP modes.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, vec2, Color32, PointerButton, Pos2, Rect, ResizeDirection, Sense,
    Stroke, StrokeKind, TextureHandle, TextureOptions, ViewportCommand, WindowLevel,
};

use crate::border::{Border, BORDER_PX};
use crate::camera::{self, Capture};
use crate::config::{self, Config};
use crate::geometry::{fit_aspect, pip_size, position_visible, NormRect, PIP_WIDTH};
use crate::log::log;
use crate::watchdog::{self, Stage};

/// Runs `f` as UI step `stage` for the watchdog, then returns to `back`.
fn staged<T>(stage: Stage, back: Stage, f: impl FnOnce() -> T) -> T {
    watchdog::enter(stage);
    let r = f();
    watchdog::enter(back);
    r
}

/// Time totals of the UI thread for the log, written every 5 seconds.
struct UiStats {
    frames: u32,
    update: Duration,
    upload: Duration,
    uploads: u32,
    max_update: Duration,
    since: Instant,
}

impl Default for UiStats {
    fn default() -> Self {
        UiStats {
            frames: 0,
            update: Duration::ZERO,
            upload: Duration::ZERO,
            uploads: 0,
            max_update: Duration::ZERO,
            since: Instant::now(),
        }
    }
}

const UI_STATS_EVERY: Duration = Duration::from_secs(5);

/// Smallest PiP window size in points, so that it can still be grabbed.
const MIN_PIP: [f32; 2] = [64.0, 48.0];
/// Largest initial PiP size, as a fraction of the monitor size.
const MAX_PIP_FRACTION: f32 = 0.8;
/// Side of the resize grip square in the bottom-right corner of the PiP.
const GRIP: f32 = 16.0;
/// Window size in the camera select and area select modes.
const SETUP_SIZE: [f32; 2] = [800.0, 600.0];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    SelectCamera,
    SelectArea,
    Pip,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::SelectCamera => "camera select",
            Mode::SelectArea => "area select",
            Mode::Pip => "PiP",
        }
    }
}

/// Writes the GPU and the renderer to the log.
fn log_renderer(cc: &eframe::CreationContext) {
    if let Some(rs) = &cc.wgpu_render_state {
        let i = rs.adapter.get_info();
        log!(
            "GPU: {} ({:?}, {:?}), driver {} {}",
            i.name,
            i.backend,
            i.device_type,
            i.driver,
            i.driver_info
        );
    }
    if let Some(gl) = &cc.gl {
        use eframe::glow::HasContext as _;
        // SAFETY: glGetString with valid enums on the current context.
        let (renderer, version) =
            unsafe { (gl.get_parameter_string(eframe::glow::RENDERER), gl.get_parameter_string(eframe::glow::VERSION)) };
        log!("GPU: {renderer}, OpenGL {version}");
    }
}

pub struct PipApp {
    mode: Mode,
    cameras: Vec<String>,
    selected: Option<usize>,
    camera_name: String,
    capture: Option<Capture>,
    texture: Option<TextureHandle>,
    frame_size: Option<(u32, u32)>,
    last_seq: u64,
    area: NormRect,
    drag_start: Option<(f32, f32)>,
    error: Option<String>,
    config_path: Option<PathBuf>,
    /// Saved PiP position in physical pixels.
    window_pos: Option<(f32, f32)>,
    /// Saved PiP inner size in physical pixels.
    window_size: Option<(f32, f32)>,
    /// Enter PiP mode on the first frame, when the display scale is known.
    startup_pip: bool,
    /// The PiP window waits for the first frame to get its size.
    pip_size_pending: bool,
    border: Border,
    /// The renderer choice from config.json, kept when the app saves.
    renderer: Option<String>,
    stats: UiStats,
}

impl PipApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let mut app = PipApp {
            mode: Mode::SelectCamera,
            cameras: Vec::new(),
            selected: None,
            camera_name: String::new(),
            capture: None,
            texture: None,
            frame_size: None,
            last_seq: 0,
            area: NormRect::FULL,
            drag_start: None,
            error: None,
            config_path: config::default_path(),
            window_pos: None,
            window_size: None,
            startup_pip: false,
            pip_size_pending: false,
            border: Border::None,
            renderer: None,
            stats: UiStats::default(),
        };
        log_renderer(cc);
        // Zoom would change the points-to-pixels scale of the saved geometry.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        staged(Stage::RefreshCameras, Stage::Outside, || app.refresh_cameras());

        let saved = app.config_path.as_deref().and_then(config::load);
        if let Some(cfg) = saved {
            app.window_pos = cfg.window_pos;
            app.window_size = cfg.window_size;
            app.area = cfg.area;
            app.border = cfg.border;
            app.renderer = cfg.renderer;
            log!("saved camera: \"{}\", area {:?}", cfg.camera_name, cfg.area);
            if let Some(index) = config::find_camera(&app.cameras, &cfg.camera_name) {
                app.selected = Some(index);
                app.start_capture(index, &cc.egui_ctx);
                app.startup_pip = true;
            } else {
                log!("saved camera is not connected");
            }
        }
        watchdog::enter(Stage::Outside);
        app
    }

    fn refresh_cameras(&mut self) {
        let t = Instant::now();
        let result = camera::list_cameras();
        log!("camera list in {} ms: {:?}", t.elapsed().as_millis(), result.as_ref().map_err(|e| format!("{e:#}")));
        match result {
            Ok(list) => self.cameras = list,
            Err(e) => {
                self.cameras.clear();
                self.error = Some(format!("{e:#}"));
            }
        }
        if self.selected.is_some_and(|i| i >= self.cameras.len()) {
            self.selected = None;
        }
    }

    fn start_capture(&mut self, index: usize, ctx: &egui::Context) {
        let back = Stage::Ui;
        staged(Stage::StopCapture, back, || self.capture = None); // Stop the old thread first.
        log!("start capture: camera {index} \"{}\"", self.cameras[index]);
        self.camera_name = self.cameras[index].clone();
        self.frame_size = None;
        self.last_seq = 0;
        self.error = None;
        self.capture = Some(staged(Stage::StartCapture, back, || Capture::start(index, ctx.clone())));
    }

    fn save_config(&self) {
        let Some(path) = &self.config_path else { return };
        let cfg = Config {
            camera_name: self.camera_name.clone(),
            area: self.area,
            window_pos: self.window_pos,
            window_size: self.window_size,
            border: self.border,
            renderer: self.renderer.clone(),
        };
        if let Err(e) = staged(Stage::SaveConfig, Stage::Ui, || config::save(path, &cfg)) {
            log!("cannot save config: {e:#}");
        }
    }

    /// Reads the current PiP window position and size, in physical pixels.
    /// Pixels do not change when the window opens on a monitor with a
    /// different display scale.
    fn read_window_geometry(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        if let Some(r) = outer {
            self.window_pos = Some((r.min.x * ppp, r.min.y * ppp));
        }
        if let Some(r) = inner {
            self.window_size = Some((r.width() * ppp, r.height() * ppp));
        }
    }

    /// The monitor size in points, if known.
    fn monitor_size(ctx: &egui::Context) -> Option<egui::Vec2> {
        ctx.input(|i| i.viewport().monitor_size)
    }

    /// The initial PiP size in points for the current area.
    fn pip_size_for_area(&self, ctx: &egui::Context) -> Option<egui::Vec2> {
        let (w, h) = self.frame_size?;
        let max = Self::monitor_size(ctx)
            .map(|m| (m.x * MAX_PIP_FRACTION, m.y * MAX_PIP_FRACTION))
            .unwrap_or((1536.0, 864.0));
        let (pw, ph) = pip_size(self.area.aspect(w, h), max);
        Some(vec2(pw, ph))
    }

    fn enter_pip(&mut self, ctx: &egui::Context) {
        log!(
            "enter PiP: saved pos {:?}, size {:?}, ppp {}, monitor {:?}",
            self.window_pos,
            self.window_size,
            ctx.pixels_per_point(),
            Self::monitor_size(ctx)
        );
        self.mode = Mode::Pip;
        ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(MIN_PIP.into()));
        let ppp = ctx.pixels_per_point();
        if let Some(pos) = self.window_pos {
            let size_px = self.window_size.unwrap_or((PIP_WIDTH * ppp, PIP_WIDTH * ppp));
            if position_visible(pos, size_px, desktop_rect_px()) {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(pos.0 / ppp, pos.1 / ppp)));
            } else {
                // The monitor was removed or the layout changed.
                self.window_pos = None;
            }
        }
        let saved_size = self.window_size.map(|(w, h)| {
            let size = vec2(w / ppp, h / ppp);
            Self::monitor_size(ctx).map_or(size, |m| size.min(m))
        });
        let size = saved_size.or_else(|| self.pip_size_for_area(ctx));
        match size {
            Some(size) => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
                self.pip_size_pending = false;
            }
            None => self.pip_size_pending = true,
        }
        self.save_config();
    }

    fn leave_pip(&mut self, ctx: &egui::Context, next: Mode) {
        log!("leave PiP: next mode {}", next.name());
        if self.mode == Mode::Pip {
            self.read_window_geometry(ctx);
            self.save_config();
            ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(SETUP_SIZE.into()));
        }
        self.mode = next;
    }

    fn go_to_camera_select(&mut self, ctx: &egui::Context) {
        self.leave_pip(ctx, Mode::SelectCamera);
        staged(Stage::StopCapture, Stage::Ui, || self.capture = None);
        self.texture = None;
        staged(Stage::RefreshCameras, Stage::Ui, || self.refresh_cameras());
    }

    /// Uploads a new camera frame and handles a camera failure.
    fn poll_capture(&mut self, ctx: &egui::Context) {
        let Some(capture) = &self.capture else { return };
        if let Some(err) = capture.error() {
            self.go_to_camera_select(ctx);
            self.error = Some(err);
            return;
        }
        let Some(frame) = capture.latest() else { return };
        if frame.seq == self.last_seq {
            return;
        }
        self.last_seq = frame.seq;
        self.frame_size = Some((frame.width, frame.height));
        let t = Instant::now();
        watchdog::enter(Stage::Upload);
        let image = frame.image.clone();
        match &mut self.texture {
            Some(t) => t.set(image, TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("camera", image, TextureOptions::LINEAR)),
        }
        watchdog::enter(Stage::PollCapture);
        self.stats.upload += t.elapsed();
        self.stats.uploads += 1;
        if self.mode == Mode::Pip && self.pip_size_pending {
            if let Some(size) = self.pip_size_for_area(ctx) {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
                self.pip_size_pending = false;
            }
        }
    }

    fn ui_select_camera(&mut self, ui: &mut egui::Ui) {
        ui.heading("Select a camera");
        ui.add_space(8.0);
        if let Some(err) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, err);
            ui.add_space(8.0);
        }
        if self.cameras.is_empty() {
            ui.label("No camera found");
        }
        for (i, name) in self.cameras.iter().enumerate() {
            ui.selectable_value(&mut self.selected, Some(i), name);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.error = None;
                self.refresh_cameras();
            }
            if ui.add_enabled(self.selected.is_some(), egui::Button::new("Next")).clicked() {
                if let Some(index) = self.selected {
                    if self.cameras[index] != self.camera_name {
                        self.area = NormRect::FULL;
                    }
                    self.start_capture(index, ui.ctx());
                    self.mode = Mode::SelectArea;
                }
            }
        });
    }

    fn ui_select_area(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Back").clicked() {
                self.go_to_camera_select(ui.ctx());
            }
            let confirm = ui.add_enabled(self.texture.is_some(), egui::Button::new("Confirm"));
            if confirm.clicked() {
                // A new area has a new aspect ratio, so compute a new size.
                self.window_size = None;
                self.enter_pip(ui.ctx());
            }
            ui.label("Drag on the image to select the area.");
        });
        if self.mode != Mode::SelectArea {
            return;
        }

        let (Some(texture), Some((fw, fh))) = (&self.texture, self.frame_size) else {
            ui.centered_and_justified(|ui| ui.label("Waiting for camera…"));
            return;
        };
        let avail = ui.available_rect_before_wrap();
        let image_rect = place(avail, fw as f32 / fh as f32);
        let response = ui.allocate_rect(image_rect, Sense::drag());
        let painter = ui.painter_at(avail);
        painter.image(texture.id(), image_rect, full_uv(), Color32::WHITE);

        let to_norm = |p: Pos2| {
            ((p.x - image_rect.min.x) / image_rect.width(), (p.y - image_rect.min.y) / image_rect.height())
        };
        if response.drag_started_by(PointerButton::Primary) {
            self.drag_start = response.interact_pointer_pos().map(to_norm);
        }
        if let (Some(start), Some(p)) = (self.drag_start, response.interact_pointer_pos()) {
            if response.dragged_by(PointerButton::Primary) {
                self.area = NormRect::from_drag(start, to_norm(p));
            }
        }
        if response.drag_stopped() {
            self.drag_start = None;
        }

        let a = self.area;
        let sel = Rect::from_min_size(
            pos2(image_rect.min.x + a.x * image_rect.width(), image_rect.min.y + a.y * image_rect.height()),
            vec2(a.w * image_rect.width(), a.h * image_rect.height()),
        );
        painter.rect_stroke(sel, 0.0, Stroke::new(2.0, Color32::YELLOW), StrokeKind::Middle);
    }

    fn ui_pip(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let response = ui.allocate_rect(full, Sense::click_and_drag());
        let ctx = ui.ctx().clone();

        if let (Some(texture), Some((fw, fh))) = (&self.texture, self.frame_size) {
            let dest = place(full, self.area.aspect(fw, fh));
            let a = self.area;
            let uv = Rect::from_min_size(pos2(a.x, a.y), vec2(a.w, a.h));
            ui.painter().image(texture.id(), dest, uv, Color32::WHITE);
        }

        // The border is at the window edge, also over letterbox bars.
        if let Some(color) = self.border.color() {
            let width = BORDER_PX / ui.ctx().pixels_per_point();
            ui.painter().rect_stroke(full, 0.0, Stroke::new(width, color), StrokeKind::Inside);
        }

        // Resize grip mark.
        let c = full.max;
        let grip_stroke = Stroke::new(1.5, Color32::from_gray(160));
        for d in [5.0, 10.0] {
            ui.painter().line_segment([pos2(c.x - d, c.y - 2.0), pos2(c.x - 2.0, c.y - d)], grip_stroke);
        }

        if response.drag_started_by(PointerButton::Primary) {
            // The drag starts only after the pointer moved a few points, so
            // use the press position, not the current position.
            let press = ui.input(|i| i.pointer.press_origin());
            let cmd = pip_drag_command(press, c);
            log!("PiP drag: {cmd:?} (touch: {})", ui.input(|i| i.any_touches()));
            ctx.send_viewport_cmd(cmd);
        }

        response.context_menu(|ui| {
            if ui.button("Select area").clicked() {
                self.leave_pip(&ctx, Mode::SelectArea);
            }
            if ui.button("Select camera").clicked() {
                self.go_to_camera_select(&ctx);
            }
            ui.menu_button("Border", |ui| {
                for b in Border::ALL {
                    if ui.radio(self.border == b, b.label()).clicked() {
                        self.border = b;
                        self.save_config();
                    }
                }
            });
            if ui.button("Exit").clicked() {
                self.read_window_geometry(&ctx);
                self.save_config();
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        });
    }
}

/// The command for a PiP drag that started with a press at `press`: resize
/// if the press was in the grip at the bottom-right `corner`, else move.
fn pip_drag_command(press: Option<Pos2>, corner: Pos2) -> ViewportCommand {
    let in_grip = press.is_some_and(|p| p.x >= corner.x - GRIP && p.y >= corner.y - GRIP);
    if in_grip {
        ViewportCommand::BeginResize(ResizeDirection::SouthEast)
    } else {
        ViewportCommand::StartDrag
    }
}

/// The bounding rectangle (x, y, w, h) of all monitors in physical pixels.
#[cfg(windows)]
fn desktop_rect_px() -> (f32, f32, f32, f32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };
    // SAFETY: GetSystemMetrics has no preconditions.
    let m = |i| unsafe { GetSystemMetrics(i) } as f32;
    (m(SM_XVIRTUALSCREEN), m(SM_YVIRTUALSCREEN), m(SM_CXVIRTUALSCREEN), m(SM_CYVIRTUALSCREEN))
}

/// Other platforms: accept any position in a large range.
#[cfg(not(windows))]
fn desktop_rect_px() -> (f32, f32, f32, f32) {
    (-16000.0, -16000.0, 32000.0, 32000.0)
}

/// The UV rectangle of the full texture.
fn full_uv() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

/// The largest rectangle with `aspect` centered in `container`.
fn place(container: Rect, aspect: f32) -> Rect {
    let (x, y, w, h) = fit_aspect((container.width(), container.height()), aspect);
    Rect::from_min_size(container.min + vec2(x, y), vec2(w, h))
}

impl eframe::App for PipApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        watchdog::enter(Stage::PollCapture);
        self.ui_frame(ui);
        watchdog::enter(Stage::Outside);
        self.record_frame(started.elapsed());
    }
}

impl PipApp {
    fn record_frame(&mut self, took: Duration) {
        let s = &mut self.stats;
        s.frames += 1;
        s.update += took;
        s.max_update = s.max_update.max(took);
        let elapsed = s.since.elapsed();
        if elapsed >= UI_STATS_EVERY {
            log!(
                "ui: {:.1} frames/s, update {:.1} ms (max {:.0}), upload {:.1} ms x {}, frame {:?}, mode {}",
                s.frames as f64 / elapsed.as_secs_f64(),
                s.update.as_secs_f64() * 1000.0 / s.frames.max(1) as f64,
                s.max_update.as_secs_f64() * 1000.0,
                s.upload.as_secs_f64() * 1000.0 / s.uploads.max(1) as f64,
                s.uploads,
                self.frame_size,
                self.mode.name()
            );
            self.stats = UiStats::default();
        }
    }

    fn ui_frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.startup_pip {
            self.startup_pip = false;
            self.enter_pip(&ctx);
        }
        self.poll_capture(&ctx);
        watchdog::enter(Stage::Ui);

        if self.mode == Mode::Pip && ctx.input(|i| i.viewport().close_requested()) {
            self.read_window_geometry(&ctx);
            self.save_config();
        }

        match self.mode {
            Mode::SelectCamera => {
                egui::CentralPanel::default().show(ui, |ui| self.ui_select_camera(ui));
            }
            Mode::SelectArea => {
                egui::CentralPanel::default().show(ui, |ui| self.ui_select_area(ui));
            }
            Mode::Pip => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::BLACK))
                    .show(ui, |ui| self.ui_pip(ui));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORNER: Pos2 = pos2(320.0, 180.0);

    #[test]
    fn press_in_grip_resizes() {
        assert_eq!(
            pip_drag_command(Some(pos2(316.0, 176.0)), CORNER),
            ViewportCommand::BeginResize(ResizeDirection::SouthEast)
        );
    }

    #[test]
    fn press_outside_grip_moves() {
        assert_eq!(pip_drag_command(Some(pos2(100.0, 100.0)), CORNER), ViewportCommand::StartDrag);
    }

    #[test]
    fn unknown_press_moves() {
        assert_eq!(pip_drag_command(None, CORNER), ViewportCommand::StartDrag);
    }
}
