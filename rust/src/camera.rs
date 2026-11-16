//! Camera list and background frame capture.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _};
use eframe::egui;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::pixel_format::FormatDecoder as _;
use nokhwa::utils::{ApiBackend, CameraFormat, CameraInfo, FrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

use crate::log::log;
use crate::watchdog::CAPTURE_FRAMES;

/// 1 decoded camera frame, ready for the texture. The capture thread makes
/// the image, so that the UI thread does not convert pixels.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub image: Arc<egui::ColorImage>,
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
        let thread = std::thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                let _done_tx = done_tx; // Dropped when the thread ends.
                match body(&thread_shared, &ctx) {
                    Ok(()) => log!("capture thread ended"),
                    Err(e) => {
                        log!("camera error: {e:#}");
                        *thread_shared.error.lock().unwrap() = Some(format!("{e:#}"));
                        ctx.request_repaint();
                    }
                }
            })
            .expect("start the capture thread");
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
        } else {
            log!("capture thread did not end in {} ms; detached", STOP_TIMEOUT.as_millis());
        }
    }
}

/// The largest frame that the app asks for. The PiP shows a small window,
/// and a larger frame costs much CPU time on each frame.
const MAX_PIXELS: u32 = 1920 * 1080;
/// A higher frame rate does not make the PiP better, but costs CPU time.
const MAX_USEFUL_FPS: u32 = 30;

/// The camera format to use: a format that the app can decode, up to 1080p,
/// with the frame rate up to 30 fps first, then the largest resolution.
/// If all formats are larger than 1080p, the smallest one.
fn choose_format(formats: &[CameraFormat], decodable: &[FrameFormat]) -> Option<CameraFormat> {
    let pixels = |f: &CameraFormat| f.width() * f.height();
    let usable = formats.iter().filter(|f| decodable.contains(&f.format()));
    let small = usable.clone().filter(|f| pixels(f) <= MAX_PIXELS);
    if let Some(best) = small.max_by_key(|f| (f.frame_rate().min(MAX_USEFUL_FPS), pixels(f), f.frame_rate())) {
        return Some(*best);
    }
    usable.min_by_key(|f| (pixels(f), std::cmp::Reverse(f.frame_rate().min(MAX_USEFUL_FPS)))).copied()
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
    let t = Instant::now();
    let infos = query()?;
    let info = infos.get(index).ok_or_else(|| anyhow!("camera {index} is not connected"))?;
    log!("camera query: {} cameras in {} ms", infos.len(), t.elapsed().as_millis());
    // Open with any format that the app can decode, then select a better one.
    let any = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    let t = Instant::now();
    let mut camera = Camera::new(info.index().clone(), any)
        .with_context(|| format!("open camera \"{}\"", info.human_name()))?;
    log!("camera open: \"{}\" in {} ms", info.human_name(), t.elapsed().as_millis());
    match camera.compatible_camera_formats() {
        Ok(formats) => {
            let list: Vec<String> = formats.iter().map(|f| f.to_string()).collect();
            log!("camera formats: {}", list.join("; "));
            if let Some(best) = choose_format(&formats, RgbFormat::FORMATS) {
                let exact = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(best));
                if let Err(e) = camera.set_camera_requset(exact) {
                    log!("cannot set format {best}: {e}");
                }
            }
        }
        Err(e) => log!("cannot list camera formats: {e}"),
    }
    log!("camera format: {}", camera.camera_format());
    let t = Instant::now();
    camera.open_stream().context("start camera stream")?;
    log!("camera stream started in {} ms", t.elapsed().as_millis());

    let result = capture_loop(&mut camera, shared, ctx);
    let _ = camera.stop_stream();
    result
}

/// Time totals for the capture log, written every 5 seconds.
#[derive(Default)]
struct CaptureStats {
    frames: u32,
    read: Duration,
    decode: Duration,
    convert: Duration,
    max_read: Duration,
}

const STATS_EVERY: Duration = Duration::from_secs(5);

fn capture_loop(camera: &mut Camera, shared: &Shared, ctx: &egui::Context) -> anyhow::Result<()> {
    let mut seq = 0u64;
    let started = Instant::now();
    let mut stats = CaptureStats::default();
    let mut stats_since = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        let t = Instant::now();
        let buffer = camera.frame().context("read camera frame")?;
        let read = t.elapsed();
        let t = Instant::now();
        let image = buffer.decode_image::<RgbFormat>().context("decode camera frame")?;
        let decode = t.elapsed();
        let t = Instant::now();
        let (width, height) = image.dimensions();
        let color = Arc::new(egui::ColorImage::from_rgb([width as usize, height as usize], image.as_raw()));
        let convert = t.elapsed();
        seq += 1;
        if seq == 1 {
            log!(
                "first frame {width}x{height} ({:?}) after {} ms",
                buffer.source_frame_format(),
                started.elapsed().as_millis()
            );
        }
        *shared.latest.lock().unwrap() = Some(Arc::new(Frame { width, height, image: color, seq }));
        CAPTURE_FRAMES.fetch_add(1, Ordering::Relaxed);
        ctx.request_repaint();

        stats.frames += 1;
        stats.read += read;
        stats.decode += decode;
        stats.convert += convert;
        stats.max_read = stats.max_read.max(read);
        if stats_since.elapsed() >= STATS_EVERY {
            let n = stats.frames.max(1);
            log!(
                "capture: {:.1} fps, read {:.1} ms (max {:.0}), decode {:.1} ms, convert {:.1} ms",
                stats.frames as f64 / stats_since.elapsed().as_secs_f64(),
                stats.read.as_secs_f64() * 1000.0 / n as f64,
                stats.max_read.as_secs_f64() * 1000.0,
                stats.decode.as_secs_f64() * 1000.0 / n as f64,
                stats.convert.as_secs_f64() * 1000.0 / n as f64,
            );
            stats = CaptureStats::default();
            stats_since = Instant::now();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use nokhwa::utils::{CameraFormat, FrameFormat, Resolution};

    fn f(w: u32, h: u32, fps: u32, ff: FrameFormat) -> CameraFormat {
        CameraFormat::new(Resolution::new(w, h), ff, fps)
    }

    const DECODABLE: &[FrameFormat] = &[FrameFormat::MJPEG, FrameFormat::YUYV, FrameFormat::NV12];

    #[test]
    fn choose_skips_4k_and_takes_1080p_30() {
        let formats = [
            f(3840, 2160, 30, FrameFormat::MJPEG),
            f(1920, 1080, 30, FrameFormat::MJPEG),
            f(1280, 720, 30, FrameFormat::MJPEG),
        ];
        assert_eq!(choose_format(&formats, DECODABLE), Some(f(1920, 1080, 30, FrameFormat::MJPEG)));
    }

    #[test]
    fn choose_prefers_30_fps_over_resolution() {
        let formats = [f(1920, 1080, 5, FrameFormat::YUYV), f(1280, 720, 30, FrameFormat::MJPEG)];
        assert_eq!(choose_format(&formats, DECODABLE), Some(f(1280, 720, 30, FrameFormat::MJPEG)));
    }

    #[test]
    fn choose_does_not_take_60_fps_over_resolution() {
        let formats = [f(640, 480, 60, FrameFormat::MJPEG), f(1920, 1080, 30, FrameFormat::MJPEG)];
        assert_eq!(choose_format(&formats, DECODABLE), Some(f(1920, 1080, 30, FrameFormat::MJPEG)));
    }

    #[test]
    fn choose_ignores_formats_it_cannot_decode() {
        // nokhwa's AbsoluteHighestFrameRate fails here: the top rate is in a format it cannot decode.
        let formats = [f(1280, 720, 60, FrameFormat::RAWBGR), f(1280, 720, 30, FrameFormat::MJPEG)];
        assert_eq!(choose_format(&[formats[0], formats[1]], &[FrameFormat::MJPEG]), Some(formats[1]));
    }

    #[test]
    fn choose_takes_smallest_if_all_are_above_1080p() {
        let formats = [f(3840, 2160, 30, FrameFormat::MJPEG), f(2560, 1440, 30, FrameFormat::MJPEG)];
        assert_eq!(choose_format(&formats, DECODABLE), Some(f(2560, 1440, 30, FrameFormat::MJPEG)));
    }

    #[test]
    fn choose_with_nothing_decodable_is_none() {
        assert_eq!(choose_format(&[f(640, 480, 30, FrameFormat::RAWBGR)], DECODABLE), None);
    }

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
