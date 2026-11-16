//! The capture thread: opens 1 stream of a camera, tries other modes if a
//! mode fails, opens the camera again if it is lost, and gives the newest
//! frame to the window.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Media::MediaFoundation::IMFMediaSource;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use super::mf::{self, Device, Frame, ReadResult, Reader};
use crate::capture_logic::{CapAction, CapEvent, Fallback};
use crate::log::log;
use crate::modes::{try_order, VideoMode};
use crate::watchdog::CAPTURE_FRAMES;
use crate::worker::{alive, Job};

/// Posted to the window when a new frame is ready (at most 1 in the queue).
pub const WM_APP_FRAME: u32 = WM_APP + 1;
/// Posted to the window when the status changes.
pub const WM_APP_STATUS: u32 = WM_APP + 2;

const THREAD_NAME: &str = "capture";
/// The time limit for the first frame of a mode.
const FIRST_FRAME_LIMIT: Duration = Duration::from_secs(5);
/// After frames came: no frame for this time means the camera stopped
/// (for a driver that blocks ReadSample in place of an error).
const FRAME_GAP_LIMIT: Duration = Duration::from_secs(3);
/// The longest wait for an earlier capture thread to end before a start.
const WAIT_OLD_LIMIT: Duration = Duration::from_secs(10);
/// The time between 2 tries to open a lost camera.
const RETRY_EVERY: Duration = Duration::from_secs(3);
/// The longest time `Drop` waits for the thread.
const STOP_LIMIT: Duration = Duration::from_millis(500);
/// A start that takes longer than this is "not responding": the time
/// for the device to open plus the first-frame limit.
const START_LIMIT: Duration = Duration::from_secs(12);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    /// Frames come. The text is the mode, for example "1280×720 30 fps MJPG".
    Running(String),
    /// All tries failed. The text tells the user what to do.
    Failed(String),
    /// The camera stopped; the thread tries again every 3 seconds.
    Lost(String),
    /// The camera does not respond (a stuck driver call).
    NotResponding,
}

struct Shared {
    latest: Mutex<Option<Arc<Frame>>>,
    status: Mutex<(Status, Instant)>,
    /// True while a WM_APP_FRAME message is in the queue.
    posted: AtomicBool,
    stop: AtomicBool,
    /// The media source of the current try, so that `Drop` and the first-frame
    /// timer can shut it down. Media Foundation objects are free-threaded.
    source: Mutex<Option<IMFMediaSource>>,
    /// The window handle as a number (an HWND cannot go to a thread).
    hwnd: isize,
}

// SAFETY: IMFMediaSource is a free-threaded Media Foundation object; all
// other fields are Send and Sync.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Shared {
    fn post(&self, msg: u32) {
        // SAFETY: PostMessageW is safe with any window handle; a stale handle fails.
        let _ = unsafe { PostMessageW(Some(HWND(self.hwnd as *mut _)), msg, WPARAM(0), LPARAM(0)) };
    }

    fn set_status(&self, status: Status) {
        log!("capture status: {status:?}");
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = (status, Instant::now());
        self.post(WM_APP_STATUS);
    }

    fn shutdown_source(&self) {
        if let Some(source) = self.source.lock().unwrap_or_else(|e| e.into_inner()).take() {
            // SAFETY: Shutdown on a valid source; it ends a blocked ReadSample.
            let _ = unsafe { source.Shutdown() };
        }
    }

    /// Gives a new start time to a `Starting` status (without a message), so
    /// that the start limit counts from the current try.
    fn touch_starting(&self) {
        let mut guard = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if guard.0 == Status::Starting {
            guard.1 = Instant::now();
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

pub struct Capture {
    shared: Arc<Shared>,
    job: Option<Job<()>>,
}

impl Capture {
    /// Starts the capture of `stream` of `device`. Frames and status changes
    /// are posted to `hwnd`. The thread first waits for an earlier capture
    /// thread to end (a camera call cannot be interrupted).
    pub fn start(device: Device, stream: u32, hwnd: HWND) -> Capture {
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            status: Mutex::new((Status::Starting, Instant::now())),
            posted: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            source: Mutex::new(None),
            hwnd: hwnd.0 as isize,
        });
        log!("capture: start \"{}\" stream {stream}", device.name);
        let thread_shared = shared.clone();
        let job = Job::spawn(THREAD_NAME, move || run(&device, stream, &thread_shared));
        Capture { shared, job: Some(job) }
    }

    /// The newest frame. Also lets the thread post the next WM_APP_FRAME.
    pub fn latest(&self) -> Option<Arc<Frame>> {
        self.shared.posted.store(false, Ordering::Relaxed);
        self.shared.latest.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn status(&self) -> Status {
        let (status, since) = self.shared.status.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if status == Status::Starting && since.elapsed() > START_LIMIT {
            return Status::NotResponding;
        }
        status
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.shutdown_source();
        if let Some(job) = self.job.take() {
            if job.wait(STOP_LIMIT).is_err() {
                log!("capture thread did not end in {} ms; detached", STOP_LIMIT.as_millis());
            }
        }
    }
}

/// The result of 1 try of a mode.
enum Outcome {
    Stopped,
    /// The try ended. `bool`: frames came in this try.
    Event(CapEvent, bool),
}

fn run(device: &Device, stream: u32, sh: &Shared) {
    mf::com_init();
    // `alive` counts this thread too.
    let waited = Instant::now();
    while alive(THREAD_NAME) > 1 {
        if sh.stopped() {
            return;
        }
        if waited.elapsed() > WAIT_OLD_LIMIT {
            log!("capture: an earlier capture thread did not end in {} s", WAIT_OLD_LIMIT.as_secs());
            sh.set_status(Status::NotResponding);
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if waited.elapsed() > Duration::from_millis(100) {
        log!("capture: waited {} ms for an earlier capture thread", waited.elapsed().as_millis());
    }
    sh.touch_starting();
    let modes = match stream_modes(device, stream) {
        Ok(m) if !m.is_empty() => m,
        Ok(_) => {
            sh.set_status(Status::Failed(format!("Stream {stream} has no video mode. Select a different stream.")));
            return;
        }
        Err(e) => {
            log!("capture: cannot read the streams: {e:#}");
            sh.set_status(Status::Failed("The camera does not open. Select a different camera or stream.".into()));
            return;
        }
    };
    let order = try_order(&modes);
    let tries: Vec<String> = order.iter().map(|&i| modes[i].label()).collect();
    log!("capture: modes to try: {}", tries.join("; "));
    let mut fallback = Fallback::new(order.len());
    let mut action = fallback.start();
    // True after the camera was lost: a failed open is then a failed retry.
    let mut lost = false;
    while !sh.stopped() {
        let mode = match action {
            CapAction::TryMode(i) => modes[order[i]],
            CapAction::RetryLater => {
                let until = Instant::now() + RETRY_EVERY;
                while Instant::now() < until && !sh.stopped() {
                    std::thread::sleep(Duration::from_millis(100));
                }
                modes[order[fallback.mode()]]
            }
            CapAction::GiveUp => {
                sh.set_status(Status::Failed(
                    "This stream gives no image. Select a different stream.".into(),
                ));
                return;
            }
            CapAction::Running => return,
        };
        if sh.stopped() {
            break;
        }
        match try_mode(device, stream, &mode, sh) {
            Outcome::Stopped => break,
            Outcome::Event(event, had_frames) => {
                if had_frames {
                    fallback.on(CapEvent::FirstFrame);
                }
                let event = match event {
                    CapEvent::OpenFailed | CapEvent::NoFirstFrame if lost => CapEvent::RetryFailed,
                    CapEvent::Lost => {
                        lost = true;
                        event
                    }
                    other => other,
                };
                log!("capture: mode {} ended with {event:?}", mode.label());
                action = fallback.on(event);
                if action == CapAction::RetryLater {
                    sh.set_status(Status::Lost("Camera not available. Trying again…".into()));
                }
            }
        }
    }
    sh.shutdown_source();
}

/// The modes of `stream`, read from the camera.
fn stream_modes(device: &Device, stream: u32) -> anyhow::Result<Vec<VideoMode>> {
    let streams = mf::probe_streams(&device.id)?;
    Ok(streams.into_iter().find(|s| s.index == stream).map(|s| s.modes).unwrap_or_default())
}

/// Opens `mode` and reads frames until the camera stops or the capture stops.
fn try_mode(device: &Device, stream: u32, mode: &VideoMode, sh: &Shared) -> Outcome {
    sh.touch_starting();
    let t = Instant::now();
    let source = match mf::open_source(&device.id) {
        Ok(s) => s,
        Err(e) => {
            log!("capture: open source failed: {e:#}");
            return Outcome::Event(CapEvent::OpenFailed, false);
        }
    };
    *sh.source.lock().unwrap_or_else(|e| e.into_inner()) = Some(source.clone());
    if sh.stopped() {
        sh.shutdown_source();
        return Outcome::Stopped;
    }
    let mut reader = match Reader::open(&source, stream, mode) {
        Ok(r) => r,
        Err(e) => {
            log!("capture: open reader failed: {e:#}");
            sh.shutdown_source();
            return Outcome::Event(CapEvent::OpenFailed, false);
        }
    };
    log!("capture: mode {} open in {} ms", mode.label(), t.elapsed().as_millis());

    // A frame watchdog: without a first frame in 5 s, or without a frame for
    // 3 s after that, shut the source down; this ends a blocked ReadSample.
    let last_frame = Arc::new(AtomicU64::new(NO_FRAME));
    let try_done = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    {
        let (last_frame, try_done) = (last_frame.clone(), try_done.clone());
        let source = SendSource(source.clone());
        let _ = std::thread::Builder::new().name("frame-watchdog".into()).spawn(move || {
            let source = source;
            while !try_done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                let now = started.elapsed().as_millis() as u64;
                let last = last_frame.load(Ordering::Relaxed);
                let (gap, limit) = if last == NO_FRAME {
                    (now, FIRST_FRAME_LIMIT)
                } else {
                    (now.saturating_sub(last), FRAME_GAP_LIMIT)
                };
                if gap > limit.as_millis() as u64 && !try_done.load(Ordering::Relaxed) {
                    log!("capture: no frame for {gap} ms (first frame: {})", last != NO_FRAME);
                    // SAFETY: Shutdown on a valid, free-threaded source.
                    let _ = unsafe { source.0.Shutdown() };
                    return;
                }
            }
        });
    }

    let outcome = read_loop(&mut reader, mode, sh, &last_frame, started);
    try_done.store(true, Ordering::Relaxed);
    sh.shutdown_source();
    outcome
}

/// `last_frame` before the first frame.
const NO_FRAME: u64 = u64::MAX;

/// An IMFMediaSource moved to the timer thread.
struct SendSource(IMFMediaSource);
// SAFETY: Media Foundation media sources are free-threaded.
unsafe impl Send for SendSource {}

/// Time totals for the capture log, written every 5 seconds.
#[derive(Default)]
struct Stats {
    frames: u32,
    read: Duration,
    max_read: Duration,
}

const STATS_EVERY: Duration = Duration::from_secs(5);

fn read_loop(reader: &mut Reader, mode: &VideoMode, sh: &Shared, last_frame: &AtomicU64, started: Instant) -> Outcome {
    let mut stats = Stats::default();
    let mut stats_since = Instant::now();
    loop {
        if sh.stopped() {
            return Outcome::Stopped;
        }
        let t = Instant::now();
        let result = reader.read();
        let read = t.elapsed();
        let had_first = last_frame.load(Ordering::Relaxed) != NO_FRAME;
        match result {
            Ok(ReadResult::Frame(frame)) => {
                last_frame.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
                if !had_first {
                    log!("capture: first frame {}x{} after {} ms", frame.width, frame.height, started.elapsed().as_millis());
                    sh.set_status(Status::Running(mode.label()));
                }
                *sh.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(frame));
                CAPTURE_FRAMES.fetch_add(1, Ordering::Relaxed);
                if !sh.posted.swap(true, Ordering::Relaxed) {
                    sh.post(WM_APP_FRAME);
                }
                stats.frames += 1;
                stats.read += read;
                stats.max_read = stats.max_read.max(read);
                if stats_since.elapsed() >= STATS_EVERY {
                    log!(
                        "capture: {:.1} fps, read+copy {:.1} ms (max {:.0})",
                        stats.frames as f64 / stats_since.elapsed().as_secs_f64(),
                        stats.read.as_secs_f64() * 1000.0 / stats.frames.max(1) as f64,
                        stats.max_read.as_secs_f64() * 1000.0
                    );
                    stats = Stats::default();
                    stats_since = Instant::now();
                }
            }
            Ok(ReadResult::Tick) => {}
            Ok(ReadResult::Lost) | Err(_) => {
                if let Err(e) = &result {
                    log!("capture: read failed: {e:#}");
                }
                if sh.stopped() {
                    return Outcome::Stopped;
                }
                let event = if had_first { CapEvent::Lost } else { CapEvent::NoFirstFrame };
                return Outcome::Event(event, had_first);
            }
        }
    }
}
