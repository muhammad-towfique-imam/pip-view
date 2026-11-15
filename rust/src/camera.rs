//! Camera list and background frame capture.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Context as _};
use eframe::egui;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraInfo, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

/// 1 decoded camera frame in RGBA.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Increases by 1 for each new frame.
    pub seq: u64,
}

fn query() -> anyhow::Result<Vec<CameraInfo>> {
    nokhwa::query(ApiBackend::Auto).context("list cameras")
}

/// Returns the names of the connected cameras, in system order.
pub fn list_cameras() -> anyhow::Result<Vec<String>> {
    Ok(query()?.iter().map(|c| c.human_name()).collect())
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Option<Arc<Frame>>>,
    error: Mutex<Option<String>>,
    stop: AtomicBool,
}

/// The longest time `Drop` waits for the capture thread to end.
const STOP_TIMEOUT: Duration = Duration::from_millis(500);

/// A running capture. Dropping it stops the capture thread.
pub struct Capture {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Disconnects when the thread ends.
    done: Receiver<()>,
}

impl Capture {
    /// Starts capture from the camera at position `index` of `list_cameras()`.
    pub fn start(index: usize, ctx: egui::Context) -> Capture {
        Capture::spawn(ctx, move |shared, ctx| run(index, shared, ctx))
    }

    /// Runs `body` on a new thread. An error from `body` goes to `error()`.
    fn spawn<F>(ctx: egui::Context, body: F) -> Capture
    where
        F: FnOnce(&Shared, &egui::Context) -> anyhow::Result<()> + Send + 'static,
    {
        let shared = Arc::new(Shared::default());
        let thread_shared = shared.clone();
        let (done_tx, done) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let _done_tx = done_tx; // Dropped when the thread ends.
            if let Err(e) = body(&thread_shared, &ctx) {
                *thread_shared.error.lock().unwrap() = Some(format!("{e:#}"));
                ctx.request_repaint();
            }
        });
        Capture { shared, thread: Some(thread), done }
    }

    /// The newest frame, if a frame has arrived.
    pub fn latest(&self) -> Option<Arc<Frame>> {
        self.shared.latest.lock().unwrap().clone()
    }

    /// The error text, if the camera failed.
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().clone()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        // A camera call can block for a long time and never read the stop
        // flag. Do not freeze the UI: wait a short time, then detach the
        // thread. When the process ends, the OS releases the camera.
        let ended = !matches!(self.done.recv_timeout(STOP_TIMEOUT), Err(RecvTimeoutError::Timeout));
        if let (true, Some(t)) = (ended, self.thread.take()) {
            let _ = t.join();
        }
    }
}

/// nokhwa initializes COM 1 time, on the first thread that uses it. But the
/// `Drop` of its camera calls `CoUninitialize` on the capture thread.
/// Initialize COM on this thread, in the same apartment mode as nokhwa, so
/// that the calls are balanced.
#[cfg(windows)]
fn init_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    // SAFETY: CoInitializeEx has no preconditions; a failure is ignored and
    // nokhwa then reports its own error.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
}

fn run(index: usize, shared: &Shared, ctx: &egui::Context) -> anyhow::Result<()> {
    #[cfg(windows)]
    init_com();
    let infos = query()?;
    let info = infos.get(index).ok_or_else(|| anyhow!("camera {index} is not connected"))?;
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(info.index().clone(), format)
        .with_context(|| format!("open camera \"{}\"", info.human_name()))?;
    camera.open_stream().context("start camera stream")?;

    let result = capture_loop(&mut camera, shared, ctx);
    let _ = camera.stop_stream();
    result
}

fn capture_loop(camera: &mut Camera, shared: &Shared, ctx: &egui::Context) -> anyhow::Result<()> {
    let mut seq = 0u64;
    while !shared.stop.load(Ordering::Relaxed) {
        let buffer = camera.frame().context("read camera frame")?;
        let image = buffer.decode_image::<RgbFormat>().context("decode camera frame")?;
        let (width, height) = image.dimensions();
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for px in image.as_raw().chunks_exact(3) {
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
        seq += 1;
        *shared.latest.lock().unwrap() = Some(Arc::new(Frame { width, height, rgba, seq }));
        ctx.request_repaint();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn drop_does_not_wait_for_a_stuck_thread() {
        // A camera read that blocks and never checks the stop flag.
        let capture = Capture::spawn(egui::Context::default(), |_, _| {
            std::thread::sleep(Duration::from_secs(3));
            Ok(())
        });
        let t = Instant::now();
        drop(capture);
        assert!(t.elapsed() < Duration::from_secs(1), "drop took {:?}", t.elapsed());
    }

    #[test]
    fn drop_stops_a_thread_that_checks_the_flag() {
        let saw_stop = Arc::new(AtomicBool::new(false));
        let flag = saw_stop.clone();
        let capture = Capture::spawn(egui::Context::default(), move |shared, _| {
            while !shared.stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            flag.store(true, Ordering::Relaxed);
            Ok(())
        });
        drop(capture);
        assert!(saw_stop.load(Ordering::Relaxed));
    }

    #[test]
    fn body_error_is_reported() {
        let capture = Capture::spawn(egui::Context::default(), |_, _| Err(anyhow!("boom")));
        let t = Instant::now();
        while capture.error().is_none() && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(capture.error().as_deref(), Some("boom"));
    }
}
