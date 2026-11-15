# webcam-view-pip — Design

Date: 2026-11-15

## Intent

A Windows desktop app, written in Rust, that shows a picture-in-picture (PiP)
window with a part of a camera image. The user selects the camera and the
area of the image one time. The app then shows only that area in a small
window that stays on top of other apps.

## Goals

- The user selects a camera from a list of the connected cameras.
- The user draws a rectangle on the live camera image to select the area.
- The app shows the selected area live in a PiP window.
- The PiP window stays on top, has no border, can move, and can resize.
- A right-click menu gives: "Select area", "Select camera", "Exit".
- The app saves the camera and the area. At the next start, the app opens
  the PiP window directly.
- A GitHub Actions workflow builds the Windows `.exe` and publishes it in a
  GitHub release.

## Non-goals

- Linux and macOS releases.
- Recording, screenshots, filters, or zoom beyond the selected area.
- More than 1 PiP window at the same time.
- Click-through or semi-transparent windows.
- An installer. The release is 1 `.exe` file.

## Constraints

- Language: Rust (stable).
- The developer machine is Linux. All Windows builds run in GitHub Actions.
  The developer cannot run the camera code locally on Windows.

## Approaches

1. **eframe/egui + nokhwa — selected.** egui gives the camera list and the
   rectangle selection with little code. nokhwa reads the camera through
   Windows Media Foundation. Pure Rust, least code. Click-through and
   transparency are harder (they are non-goals).
2. **Direct Win32 + Media Foundation (`windows` crate).** Smallest `.exe` and
   full window control, but 3 to 4 times more code and a hand-drawn UI.
   Rejected: too much code for the needed features.
3. **Tauri + web view.** Camera and crop in JavaScript. Rejected: the
   important parts are not Rust.

The user selected approach 1. The user accepted the default PiP behavior.

## Design

### Crates

- `eframe` / `egui` — window and UI (wgpu or glow renderer, default).
- `nokhwa` with the `input-native` feature — camera access (Media Foundation
  on Windows; V4L2 on Linux, so `cargo test` also works on the Linux machine).
- `serde`, `serde_json` — config file.
- `directories` — config path (`%APPDATA%\webcam-view-pip\config.json`).
- `anyhow` — errors.

### Window modes

The app uses 1 OS window. The window changes between 3 modes:

1. **Camera select.** A normal window with a border. It shows the list of
   cameras and an "Next" button.
2. **Area select.** A normal window with a border. It shows the live,
   full camera image, scaled to fit. The user drags with the left mouse
   button to draw the rectangle. "Confirm" goes to PiP mode. "Back" goes to
   camera select.
3. **PiP.** The app sends `ViewportCommand`s: no decorations, always on top,
   window size set to the aspect ratio of the area. It shows only the
   selected area.

### PiP interaction

- Left-drag in the window: move the window (`ViewportCommand::StartDrag`).
- Left-drag in the bottom-right corner (16 px area): resize
  (`ViewportCommand::BeginResize`). The image keeps its aspect ratio and the
  app adds black bars if the window has a different aspect ratio.
- Right-click: a context menu with "Select area", "Select camera", "Exit".

### Camera capture

- A background thread opens the camera, reads frames, converts each frame to
  RGBA, and puts the newest frame in an `Arc<Mutex<Option<Frame>>>`. Then it
  calls `ctx.request_repaint()`.
- The UI thread uploads the newest frame to 1 egui texture.
- The crop is done on the GPU: the UI draws the texture with a UV rectangle
  equal to the selected area. There is no CPU crop.
- To change the camera, the UI stops the thread (a stop flag) and starts a
  new one.

### Area storage

The area is stored as a normalized rectangle (0.0 to 1.0 for x, y, width,
height). Thus the area stays correct if the camera resolution changes.

### Config

`config.json` holds the camera index, the camera name, the normalized area,
and the last PiP window position and size. At start:

- If the config exists and a camera with the same name exists, open PiP mode.
- If not, open camera select mode.

### Errors

- No cameras: camera select mode shows "No camera found" and a "Refresh"
  button.
- The camera fails to open or stops: the app shows the error text in the
  window and goes back to camera select mode.

### Build and release

`.github/workflows/release.yml`:

- Triggers: push of a tag `v*`, pull requests, and manual run
  (`workflow_dispatch`).
- Runner: `windows-latest`.
- Steps: checkout, install stable Rust, cache, `cargo test`,
  `cargo build --release`.
- On a tag: upload `webcam-view-pip.exe` to a GitHub release for that tag.
- On other triggers: upload the `.exe` as a workflow artifact.

`main.rs` has `#![windows_subsystem = "windows"]`, so no console window opens.

### Tests

- Unit tests for the pure logic: rectangle math (drag points to normalized
  rectangle, clamp to the image, minimum size), aspect-ratio fit, and config
  save and load.
- The camera and UI code have no automatic tests. The user tests them on
  Windows with the release `.exe`.

## Left to the builder

- Exact crate versions (use current stable versions).
- The initial PiP window size (proposal: 320 px wide, height from the area
  aspect ratio).
- Small UI layout details.
