# webcam-view-pip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows Rust app that shows a selected area of a camera image in an always-on-top picture-in-picture (PiP) window, built and released by GitHub Actions.

**Architecture:** 1 eframe window with 3 modes (camera select, area select, PiP). A background thread reads camera frames with nokhwa and shares the newest RGBA frame. The UI draws 1 texture and crops it on the GPU with a UV rectangle.

**Tech Stack:** Rust stable, eframe/egui 0.37, nokhwa 0.10 (`input-native`), serde/serde_json, directories 6, anyhow.

**Spec:** `docs/superpowers/specs/2026-11-15-webcam-view-pip-design.md`

## Global Constraints

- Release target: Windows x86_64, 1 file `webcam-view-pip.exe`, no installer.
- `#![windows_subsystem = "windows"]` in `src/main.rs`.
- Config path: `%APPDATA%\webcam-view-pip\config.json` (via `directories::ProjectDirs::from("", "", "webcam-view-pip")`, `config_dir()`).
- Area is stored normalized (0.0–1.0 for x, y, width, height).
- Initial PiP width: 320 px; height from the area aspect ratio.
- PiP resize grip: bottom-right 16 px square.
- PiP context menu items, in order: "Select area", "Select camera", "Exit".
- The local machine is Linux. `cargo test` and `cargo clippy` run locally (nokhwa uses V4L2 on Linux). Windows builds run only in GitHub Actions.

## Review Focus

1. Reverse drag (bottom-right to top-left) in area select → the same rectangle as a forward drag. Test in Task 1.
2. Drag that leaves the image → the rectangle is clamped to the image. Test in Task 1.
3. A click with no drag, or a tiny drag → the rectangle gets a minimum size (0.02 normalized per side), never 0. Test in Task 1.
4. Corrupt or missing `config.json` → the app starts in camera select, no crash. Test in Task 2.
5. Camera order changed between runs (index differs, name same) → the app finds the camera by name. Test in Task 2.

---

### Task 1: Project scaffold and geometry

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `src/main.rs`, `src/geometry.rs`

**Interfaces:**
- Produces (in `src/geometry.rs`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)] pub struct NormRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }`
  - `pub const MIN_SIDE: f32 = 0.02;`
  - `impl NormRect { pub const FULL: NormRect; pub fn from_drag(a: (f32, f32), b: (f32, f32)) -> NormRect; pub fn aspect(&self, img_w: u32, img_h: u32) -> f32 }`
    - `from_drag` takes 2 normalized points (any order, any values), clamps them to 0..1, orders them, and enforces `MIN_SIDE` while it keeps the rectangle inside 0..1.
    - `aspect` returns pixel width / pixel height of the area in an image of `img_w`×`img_h`.
  - `pub fn fit_aspect(container: (f32, f32), aspect: f32) -> (f32, f32, f32, f32)` — returns `(x, y, w, h)` of the largest rectangle with `aspect` centered in `container` (letterbox).

- [ ] **Step 1: Create the scaffold**

`Cargo.toml`: package `webcam-view-pip`, edition 2021. Dependencies: `eframe = "0.37"`, `nokhwa = { version = "0.10", features = ["input-native"] }`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `directories = "6"`, `anyhow = "1"`. Dev-dependency: `tempfile = "3"`. Release profile: `lto = true`, `strip = true`.
`.gitignore`: `/target`.
`src/main.rs`: `#![windows_subsystem = "windows"]`, `mod geometry;`, and an empty `fn main() {}` for now.
Run `git init` and keep the existing `docs/`.

- [ ] **Step 2: Write the failing tests in `src/geometry.rs`**

```rust
#[test] fn forward_drag() {
    assert_eq!(NormRect::from_drag((0.1, 0.2), (0.5, 0.6)), NormRect { x: 0.1, y: 0.2, w: 0.4, h: 0.4 });
}
#[test] fn reverse_drag_equals_forward() {
    assert_eq!(NormRect::from_drag((0.5, 0.6), (0.1, 0.2)), NormRect::from_drag((0.1, 0.2), (0.5, 0.6)));
}
#[test] fn drag_outside_is_clamped() {
    let r = NormRect::from_drag((-0.3, 0.5), (1.4, 2.0));
    assert_eq!(r, NormRect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 });
}
#[test] fn click_gets_min_size() {
    let r = NormRect::from_drag((0.5, 0.5), (0.5, 0.5));
    assert!(r.w >= MIN_SIDE && r.h >= MIN_SIDE);
}
#[test] fn min_size_stays_inside_at_edge() {
    let r = NormRect::from_drag((1.0, 1.0), (1.0, 1.0));
    assert!(r.x + r.w <= 1.0 + 1e-6 && r.y + r.h <= 1.0 + 1e-6 && r.w >= MIN_SIDE);
}
#[test] fn aspect_uses_pixels() {
    let r = NormRect { x: 0.0, y: 0.0, w: 0.5, h: 0.5 };
    assert!((r.aspect(1920, 1080) - 16.0 / 9.0).abs() < 1e-4);
}
#[test] fn fit_wide_container_letterboxes_sides() {
    assert_eq!(fit_aspect((200.0, 100.0), 1.0), (50.0, 0.0, 100.0, 100.0));
}
#[test] fn fit_tall_container_letterboxes_top() {
    assert_eq!(fit_aspect((100.0, 200.0), 1.0), (0.0, 50.0, 100.0, 100.0));
}
```

Float compare: use exact equality only where the test values are exact in f32 as written; if a case fails only by rounding, change that assert to an epsilon compare (1e-6).

- [ ] **Step 3: Run tests, expect FAIL** — `cargo test geometry` → compile errors (items not defined).
- [ ] **Step 4: Implement `NormRect`, `MIN_SIDE`, `from_drag`, `aspect`, `fit_aspect`** to the interface above.
- [ ] **Step 5: Run tests, expect PASS** — `cargo test geometry` → 8 passed.
- [ ] **Step 6: Commit** — `git add -A && git commit -m "feat: scaffold project and add area geometry"`

### Task 2: Config

**Files:**
- Create: `src/config.rs`; Modify: `src/main.rs` (add `mod config;`)

**Interfaces:**
- Consumes: `geometry::NormRect`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Config { pub camera_name: String, pub area: NormRect, pub window_pos: Option<(f32, f32)>, pub window_size: Option<(f32, f32)> }`
  - `pub fn default_path() -> Option<PathBuf>` — `ProjectDirs` config dir joined with `config.json`.
  - `pub fn load(path: &Path) -> Option<Config>` — `None` if the file is missing or invalid.
  - `pub fn save(path: &Path, cfg: &Config) -> anyhow::Result<()>` — creates the parent folder.
  - `pub fn find_camera(names: &[String], saved: &str) -> Option<usize>` — index of the first exact name match.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn save_then_load_round_trip() { /* tempdir; save a Config with window_pos Some((10.0, 20.0)); load == original */ }
#[test] fn save_creates_parent_dir() { /* path = tempdir/a/b/config.json; save ok; file exists */ }
#[test] fn missing_file_is_none() { assert!(load(Path::new("/nonexistent/x/config.json")).is_none()); }
#[test] fn corrupt_file_is_none() { /* write "{not json" to file; load is None */ }
#[test] fn find_camera_by_name_not_index() {
    let names = vec!["OBS Virtual".to_string(), "Logitech C920".to_string()];
    assert_eq!(find_camera(&names, "Logitech C920"), Some(1));
    assert_eq!(find_camera(&names, "Gone Cam"), None);
}
```

- [ ] **Step 2: Run tests, expect FAIL** — `cargo test config` → compile errors.
- [ ] **Step 3: Implement the interface above.**
- [ ] **Step 4: Run tests, expect PASS** — `cargo test config` → 5 passed.
- [ ] **Step 5: Commit** — `git commit -am "feat: add config load/save and camera lookup"` (add the new file first).

### Task 3: Camera capture

**Files:**
- Create: `src/camera.rs`; Modify: `src/main.rs` (add `mod camera;`)

**Interfaces:**
- Produces:
  - `pub struct Frame { pub width: u32, pub height: u32, pub rgba: Vec<u8>, pub seq: u64 }`
  - `pub fn list_cameras() -> anyhow::Result<Vec<String>>` — `nokhwa::query(ApiBackend::Auto)`, returns `human_name()` of each camera, in query order.
  - `pub struct Capture` with:
    - `pub fn start(index: usize, ctx: egui::Context) -> Capture` — spawns the thread.
    - `pub fn latest(&self) -> Option<Arc<Frame>>` — the newest frame (clone of the `Arc`).
    - `pub fn error(&self) -> Option<String>` — set if the camera failed to open or a frame read failed.
    - `impl Drop` — sets the stop flag and joins the thread.

- [ ] **Step 1: Implement `Frame`, `list_cameras`, `Capture`**

Thread body: `Camera::new(CameraIndex::Index(index as u32), RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate))`, `open_stream()`, then loop until the stop flag: `frame()` → `decode_image::<RgbFormat>()` → convert RGB to RGBA → store `Arc<Frame>` with incremented `seq` in `Arc<Mutex<Option<Arc<Frame>>>>` → `ctx.request_repaint()`. On any error: store the message in `Arc<Mutex<Option<String>>>`, call `request_repaint()`, and end the thread. Call `stop_stream()` before the thread ends.

- [ ] **Step 2: Verify** — `cargo clippy --all-targets -- -D warnings` → no errors; `cargo test` → 13 passed.
- [ ] **Step 3: Commit** — `git add -A && git commit -m "feat: add background camera capture"`

### Task 4: App UI (3 modes)

**Files:**
- Create: `src/app.rs`; Modify: `src/main.rs`

**Interfaces:**
- Consumes: Tasks 1–3 as declared above.
- Produces: `pub struct PipApp` with `pub fn new(cc: &eframe::CreationContext) -> Self` and `impl eframe::App`.

Internal state: `enum Mode { SelectCamera, SelectArea, Pip }`, `cameras: Vec<String>`, `selected: Option<usize>`, `capture: Option<Capture>`, `texture: Option<egui::TextureHandle>`, `last_seq: u64`, `area: NormRect`, `drag_start: Option<(f32, f32)>`, `error: Option<String>`, `config_path: Option<PathBuf>`.

- [ ] **Step 1: Startup and mode changes**

`new`: list cameras; load config; if `find_camera` finds the saved name, start the capture, set `area`, and enter PiP. Else enter SelectCamera.
Entering PiP: send `ViewportCommand::Decorations(false)`, `WindowLevel(WindowLevel::AlwaysOnTop)`, `InnerSize` = saved size or `(320, 320 / aspect)`, and `OuterPosition` if a position is saved. Then save the config.
Leaving PiP: send `Decorations(true)`, `WindowLevel(Normal)`, `InnerSize((800, 600))`.

- [ ] **Step 2: Each frame**

If `capture.latest()` has a new `seq`, upload it with `ColorImage::from_rgba_unmultiplied` into the 1 texture (`set` if it exists). If `capture.error()` is `Some`, drop the capture, store the error, and enter SelectCamera.

- [ ] **Step 3: SelectCamera UI** — the camera list as selectable labels, "Refresh" button, "Next" button (enabled when a camera is selected; starts the capture; enters SelectArea). Show "No camera found" if the list is empty and the error text if set.

- [ ] **Step 4: SelectArea UI** — draw the full texture fitted with `fit_aspect`; on drag, convert pointer positions to normalized image coordinates and call `NormRect::from_drag`; draw the rectangle as a stroke; buttons "Back" and "Confirm" (Confirm enters PiP). Show "Waiting for camera…" until the first frame.

- [ ] **Step 5: PiP UI** — black background; draw the texture fitted with `fit_aspect(window, area.aspect(w, h))` and `uv = Rect::from_min_size(pos2(area.x, area.y), vec2(area.w, area.h))`. 1 `Sense::click_and_drag` response over the window: drag start inside the bottom-right 16 px → `BeginResize(ResizeDirection::SouthEast)`; other drag start → `StartDrag`. `context_menu` with "Select area" (→ SelectArea), "Select camera" (→ drop capture, SelectCamera), "Exit" (→ `ViewportCommand::Close`). Draw a small grip mark in the corner.

- [ ] **Step 6: Save window geometry** — in PiP mode, in `on_exit` and when leaving PiP, read `ctx.input(|i| i.viewport().outer_rect / inner_rect)` and save `window_pos` and `window_size`.

- [ ] **Step 7: `main.rs`** — `eframe::run_native("webcam-view-pip", NativeOptions { viewport: ViewportBuilder::default().with_inner_size([800.0, 600.0]).with_title("webcam-view-pip"), ..Default::default() }, Box::new(|cc| Ok(Box::new(app::PipApp::new(cc)))))`.

If eframe 0.37 changed the `App` trait or a `ViewportCommand` name, follow the 0.37 API and keep the behavior.

- [ ] **Step 8: Verify** — `cargo clippy --all-targets -- -D warnings` clean; `cargo test` → 13 passed; `cargo build` succeeds.
- [ ] **Step 9: Commit** — `git add -A && git commit -m "feat: add camera select, area select and PiP modes"`

### Task 5: GitHub Actions build and release

**Files:**
- Create: `.github/workflows/release.yml`, `README.md`

- [ ] **Step 1: Write the workflow**

Triggers: `push: tags: ['v*']`, `pull_request`, `workflow_dispatch`. `permissions: contents: write`. 1 job on `windows-latest`: `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `cargo test --locked`, `cargo build --release --locked`, `actions/upload-artifact@v4` (`target/release/webcam-view-pip.exe`), and on `startsWith(github.ref, 'refs/tags/v')`: `softprops/action-gh-release@v2` with `files: target/release/webcam-view-pip.exe`. Commit `Cargo.lock` so `--locked` works.

- [ ] **Step 2: README** — what the app does, how to use it (setup, drag, resize, right-click menu), where the config is, how to release (`git tag v0.1.0 && git push origin v0.1.0`).
- [ ] **Step 3: Verify** — the YAML parses (`python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`), and `cargo build --locked` succeeds locally.
- [ ] **Step 4: Commit** — `git add -A && git commit -m "ci: build and release Windows exe"`
