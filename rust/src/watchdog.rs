//! Records which step the UI thread is in, so that a log can show where the
//! UI stops when the window does not respond.

/// A step of the UI thread. `Outside` is the time between 2 calls of the app
/// `ui` function: eframe renders and Windows delivers events in this time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Outside = 0,
    PollCapture = 1,
    Upload = 2,
    Ui = 3,
    RefreshCameras = 4,
    StartCapture = 5,
    StopCapture = 6,
    SaveConfig = 7,
}

impl Stage {
    pub fn from_u8(v: u8) -> Stage {
        match v {
            1 => Stage::PollCapture,
            2 => Stage::Upload,
            3 => Stage::Ui,
            4 => Stage::RefreshCameras,
            5 => Stage::StartCapture,
            6 => Stage::StopCapture,
            7 => Stage::SaveConfig,
            _ => Stage::Outside,
        }
    }
}

/// A UI step that takes longer than this is a stall.
pub const STALL_MS: u64 = 2000;

/// What the detector found in 1 check.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Stall { stage: Stage, ms: u64 },
    Recovered { stage: Stage, ms: u64 },
}

/// Finds UI stalls from samples of the current stage.
#[derive(Default)]
pub struct StallDetector {
    /// The stall that was reported and has not ended: (stage, start time).
    open: Option<(Stage, u64)>,
}

impl StallDetector {
    /// `stage` started at `since_ms`. `capture_active` is true if the camera
    /// gave new frames during the stage. Without new frames, a long `Outside`
    /// time is normal: eframe does not repaint an idle window.
    pub fn check(&mut self, now_ms: u64, stage: Stage, since_ms: u64, capture_active: bool) -> Option<Event> {
        if let Some((open_stage, start)) = self.open {
            if since_ms != start || stage != open_stage {
                self.open = None;
                return Some(Event::Recovered { stage: open_stage, ms: since_ms.saturating_sub(start) });
            }
            return None;
        }
        let ms = now_ms.saturating_sub(since_ms);
        let counts = stage != Stage::Outside || capture_active;
        if ms >= STALL_MS && counts {
            self.open = Some((stage, since_ms));
            return Some(Event::Stall { stage, ms });
        }
        None
    }
}

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::Duration;

use crate::log::{log, now_ms};

static STAGE: AtomicU8 = AtomicU8::new(0);
static SINCE: AtomicU64 = AtomicU64::new(0);
/// Increases by 1 for each camera frame. The capture thread sets it.
pub static CAPTURE_FRAMES: AtomicU64 = AtomicU64::new(0);

/// The UI thread enters `stage` now.
pub fn enter(stage: Stage) {
    STAGE.store(stage as u8, Ordering::Relaxed);
    SINCE.store(now_ms(), Ordering::Relaxed);
}

/// Starts the thread that writes UI stalls to the log.
pub fn start() {
    let _ = std::thread::Builder::new().name("watchdog".into()).spawn(|| {
        let mut detector = StallDetector::default();
        let mut last_since = u64::MAX;
        let mut frames_at_since = 0;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let since = SINCE.load(Ordering::Relaxed);
            let stage = Stage::from_u8(STAGE.load(Ordering::Relaxed));
            let frames = CAPTURE_FRAMES.load(Ordering::Relaxed);
            if since != last_since {
                last_since = since;
                frames_at_since = frames;
            }
            match detector.check(now_ms(), stage, since, frames > frames_at_since) {
                Some(Event::Stall { stage, ms }) => {
                    log!("UI STALL: the UI thread is in {stage:?} for {ms} ms (camera frames: {frames})")
                }
                Some(Event::Recovered { stage, ms }) => log!("UI back after {ms} ms in {stage:?}"),
                None => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_step_is_not_a_stall() {
        let mut d = StallDetector::default();
        assert_eq!(d.check(1500, Stage::Upload, 0, true), None);
    }

    #[test]
    fn long_step_is_a_stall_reported_1_time() {
        let mut d = StallDetector::default();
        assert_eq!(d.check(2500, Stage::Upload, 0, false), Some(Event::Stall { stage: Stage::Upload, ms: 2500 }));
        assert_eq!(d.check(3000, Stage::Upload, 0, false), None);
    }

    #[test]
    fn idle_window_without_frames_is_not_a_stall() {
        let mut d = StallDetector::default();
        assert_eq!(d.check(10_000, Stage::Outside, 0, false), None);
    }

    #[test]
    fn outside_with_new_frames_is_a_stall() {
        let mut d = StallDetector::default();
        assert_eq!(d.check(3000, Stage::Outside, 0, true), Some(Event::Stall { stage: Stage::Outside, ms: 3000 }));
    }

    #[test]
    fn next_step_ends_the_stall() {
        let mut d = StallDetector::default();
        d.check(2500, Stage::Ui, 0, false);
        assert_eq!(d.check(7000, Stage::Outside, 6000, false), Some(Event::Recovered { stage: Stage::Ui, ms: 6000 }));
        assert_eq!(d.check(7100, Stage::Outside, 6000, false), None);
    }

    #[test]
    fn stage_values_round_trip() {
        for s in [Stage::Outside, Stage::PollCapture, Stage::Upload, Stage::Ui, Stage::RefreshCameras,
                  Stage::StartCapture, Stage::StopCapture, Stage::SaveConfig] {
            assert_eq!(Stage::from_u8(s as u8), s);
        }
    }
}
