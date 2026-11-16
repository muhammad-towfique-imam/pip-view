# Windows app rewrite with Win32 and Media Foundation — Design

Date: 2026-11-16

## Intent

The Windows app (`rust/`) has 2 problems on a smart teaching board
(Bacbon smart teaching Android board with a Windows PC and an "AI camera"):

1. The whole window hangs at start ("Not responding").
2. The Windows Camera app shows a small AI window over the main camera
   image, but our app does not. The user thinks that the AI overlay is in a
   different camera stream. The code confirms that our app can read only 1
   stream: nokhwa always reads `MF_SOURCE_READER_FIRST_VIDEO_STREAM`.

The user selected a full rewrite of the Windows app with Win32 and Media
Foundation (option 2 from the first design), for better compatibility.

The user has little access to the board. Each test on the board is
expensive. The new app must work on the first try as much as possible, must
recover from problems without help, and must write a log that explains a
failure from 1 test.

## Goals

- The same user flow as v0.3: select a camera, select an area, show the area
  in a PiP window that stays on top. The same saved settings, the same
  start behavior (open the PiP directly if the saved camera is connected).
- **New: stream selection.** The camera screen lists all video streams of
  the selected camera, with their type (Preview, Capture, Other) and their
  best mode. The user selects 1. The app saves it.
- **Default stream: Preview**, if the camera has a preview stream. This is
  the stream that the Windows Camera app shows. Else the first video stream.
- The same PiP controls: drag to move, drag the bottom-right corner to
  resize with the aspect kept, right-click (or touch press-and-hold) for the
  menu: Select area, Select camera, Border (None, White, Green, Red, Blue,
  Yellow), Exit.
- No hang of the window. Camera calls never run on the UI thread.
- A detailed log in `%APPDATA%\webcam-view-pip\log.txt`, like v0.3.1, with
  more detail about devices, streams, and media types.

## Non-goals

- Linux or macOS runtime. On Linux, the crate builds and the unit tests of
  the platform-free modules run; `main` prints "Windows only" and exits.
- Recording, filters, click-through, more than 1 PiP window.
- Support for the old `"renderer"` config field. It is ignored.
- Changes to the Android app.

## Anti-goals

- A new hang in a different place, for example a camera call on the UI
  thread or a wait without a timeout.
- A silent failure: each failure must show a message in the window and a
  line in the log.

## Constraints

- Rust stable, the `windows` crate (the app already uses it), no egui, no
  nokhwa.
- The developer machine is Linux. Type check locally with
  `cargo check --target x86_64-pc-windows-gnu`. Build, clippy, and tests run
  on `windows-latest` in GitHub Actions. There is no camera in CI.
- Windows 10 and 11, 64-bit.
- Config file: the same path and format as v0.3. New optional fields only.

## Approaches

1. **Full Win32 + Media Foundation (selected by the user).** Native window,
   GDI drawing, Media Foundation Source Reader. Most compatible, full control
   of streams and windows. Most code.
2. **Keep egui, replace only the camera code with Media Foundation.** Less
   code, but the egui/wgpu renderer stays, and it is a possible cause of the
   hang. The user rejected it in favor of option 1.
3. **Fork nokhwa to add stream selection.** The same camera work as option 1
   plus a fork to maintain, and the renderer question stays. Rejected.

## Design

### Parts that stay

- `geometry.rs` (normalized area, `fit_aspect`, `pip_size`,
  `position_visible`), `border.rs`, `config.rs`, `log.rs`, and the stall
  detector of `watchdog.rs`. They are platform-free and tested.
- The log format and the log path.

### Camera (Media Foundation)

- `MFStartup` 1 time at start. The capture thread uses
  `CoInitializeEx(COINIT_MULTITHREADED)`.
- **Devices:** `MFEnumDeviceSources` with the video capture source type.
  For each device: the friendly name and the symbolic link. The app saves
  both. It finds the saved camera by symbolic link first, then by name.
- **No unbounded wait (builder's call):** device listing, device activation,
  and stream reading run on a worker thread. The UI waits at most 5 seconds
  for each, then shows "The camera does not respond." and logs the step.
  While a stuck worker exists, the app does not start a second one for the
  same step; the retry loop waits until the old worker ends.
- **Streams:** from the presentation descriptor of the media source. For each
  video stream: the index, the category from
  `MF_DEVICESTREAM_STREAM_CATEGORY` (`PINNAME_VIDEO_PREVIEW`,
  `PINNAME_VIDEO_CAPTURE`, `PINNAME_VIDEO_STILL`, other), and all native media
  types (subtype, width, height, frame rate). Still streams are not listed.
  Streams that have only infrared or depth types (subtypes L8, L16, D16, or a
  frame source type of infrared or depth) are not listed and are not a
  default (builder's call: on an "AI camera", such a stream can be first).
- **Why a plain Source Reader is enough (builder's call):** on Windows 10 and
  11, the camera device source goes through the Frame Server and loads the
  driver's device MFT for each app, as for the Windows Camera app. The Camera
  app shows 1 stream; it does not join 2 streams. So an overlay that the
  Camera app shows is in 1 stream, or it is drawn by other software on top
  of the window. WinRT `MediaCapture` is not used.
- **Mode choice** in the selected stream: the same rule as v0.3.1 — up to
  1080p, frame rate up to 30 fps first, then the largest resolution; if all
  modes are larger than 1080p, the smallest. All subtypes are allowed (MJPEG,
  YUY2, NV12, H.264, and others), because Media Foundation decodes them.
- **Reading:** a Source Reader on the media source, with
  `MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING` (software conversion; it works
  on all PCs). Select only the chosen stream, set the chosen native type,
  then set the output type to RGB32. Synchronous `ReadSample` on the capture
  thread.
- **Mode fallback:** if open, set type, or the first frame fails (no frame in
  5 seconds), the app tries the next 2 modes of the same stream in rank order.
  It does not change the stream that the user selected. Each try goes to the
  log.
- **Stop:** set a stop flag, call `Shutdown` on the media source (this ends a
  blocked `ReadSample`), and wait at most 500 ms for the thread. Then detach
  it, as in v0.3.
- **All modes fail** (open or first frame, also at start): the app opens the
  camera screen with the error, so that the user can select a different
  stream (builder's call).
- **Camera lost** (read error or end of stream): show "Camera not available.
  Trying again…" and try to open the same camera and stream every 3 seconds.
- **Frames:** the capture thread keeps the newest frame (BGRA bytes, width,
  height) in a shared slot. It posts 1 `WM_APP` message to the window only if
  the window has read the previous frame (an atomic flag), so the message
  queue cannot fill up.

### Windows and drawing

- 2 top-level windows: a **setup window** (normal border, about 800×600 at
  100% scale) for the camera and area screens, and a **PiP window**. Only 1
  is visible at a time.
- Per-monitor DPI awareness v2 and Common Controls 6, through an embedded
  manifest.
- **Drawing with GDI:** `StretchDIBits` with `HALFTONE` from the frame, with
  the source rectangle set to the selected area. So the crop and the scale
  need no extra copy. No background erase, to prevent flicker. Letterbox bars
  are black.
- **Setup window, camera screen:** a list of cameras, a list of the streams
  of the selected camera (for example "Preview — 1920×1080 30 fps MJPG"), an
  error text line, and the **Next** button. The app reads the streams on the
  capture thread, not on the UI thread, and shows "Reading streams…" until
  they arrive.
- **Setup window, area screen:** the live image (letterboxed), drag to draw
  the rectangle (the outside is dimmed), and the buttons **Back**,
  **Full image**, **Confirm**, with the hint "Drag on the image to select the
  area."
- **PiP window:** `WS_POPUP`, topmost, with a taskbar button.
  `WM_NCHITTEST` returns the bottom-right grip area (16 px at 100% scale) as
  a resize corner and the rest as a caption, so Windows moves and resizes the
  window, also with touch. `WM_SIZING` keeps the aspect of the area. Minimum
  size 64×48 at 100% scale. The border (3 physical pixels) and the grip mark
  are drawn with GDI.
- **Menu:** `WM_CONTEXTMENU` (right-click, and press-and-hold on a touch
  screen) opens a popup menu: Select area, Select camera, Border ▸ (radio
  items), Exit.
- Position and size are saved in physical pixels, as in v0.3. An off-screen
  saved position is not used.

### Config

`config.json` gets 2 optional fields: `camera_id` (the symbolic link) and
`stream` (the stream index). A config without them still loads. If the saved
stream is not in the camera, the app uses the default stream.

### Log and watchdog

- The log lines of v0.3.1, plus: each device (name, symbolic link), each
  stream (index, category, all native types), each open try and its result
  (with the HRESULT), the output type, and the frame rate of the stream.
- The watchdog marks the time when the window procedure starts and ends each
  message. If 1 message takes 2 seconds or more, the log gets a `UI STALL`
  line with the message name.

## Board tests (builder's call)

The user has little access to the board, so there is no separate diagnostic
build. The full app ships at 1 time. The camera screen writes all devices,
streams, and media types to the log when it opens, so the first test gives
the stream list also if the app fails later.

## Left to the builder

- The module split and the internal types.
- The exact layout, font, and spacing of the setup window (use the system
  message font).
- How to embed the manifest (for example the `embed-manifest` build crate).
- The text of error messages, in Simplified Technical English.
