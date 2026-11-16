//! The mode fallback and retry rules of the capture thread.

/// The most modes that the capture thread tries before it gives up.
const MAX_TRIES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapEvent {
    /// The source, the reader, or the media type did not open.
    OpenFailed,
    FirstFrame,
    /// No frame came in the time limit.
    NoFirstFrame,
    /// The camera stopped after it gave frames.
    Lost,
    /// A retry after `Lost` did not open the camera.
    RetryFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapAction {
    /// Open the mode at this position of the ranked mode list.
    TryMode(usize),
    Running,
    /// All tries failed: the user must select a different stream.
    GiveUp,
    /// Wait, then open the same mode again.
    RetryLater,
}

/// Before the first frame, a failure tries the next ranked mode (at most
/// `MAX_TRIES`). After the first frame, the mode is kept, and a lost camera
/// is opened again until it returns.
pub struct Fallback {
    modes: usize,
    current: usize,
    running: bool,
}

impl Fallback {
    pub fn new(mode_count: usize) -> Fallback {
        Fallback { modes: mode_count, current: 0, running: false }
    }

    pub fn start(&mut self) -> CapAction {
        self.current = 0;
        self.running = false;
        CapAction::TryMode(0)
    }

    /// The position of the current mode in the ranked mode list.
    pub fn mode(&self) -> usize {
        self.current
    }

    pub fn on(&mut self, event: CapEvent) -> CapAction {
        match event {
            CapEvent::FirstFrame => {
                self.running = true;
                CapAction::Running
            }
            _ if self.running => CapAction::RetryLater,
            CapEvent::OpenFailed | CapEvent::NoFirstFrame | CapEvent::Lost | CapEvent::RetryFailed => {
                let next = self.current + 1;
                if next < self.modes.min(MAX_TRIES) {
                    self.current = next;
                    CapAction::TryMode(next)
                } else {
                    CapAction::GiveUp
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_tries_first() {
        assert_eq!(Fallback::new(4).start(), CapAction::TryMode(0));
    }

    #[test]
    fn failures_try_next_modes_then_give_up() {
        let mut f = Fallback::new(4);
        f.start();
        assert_eq!(f.on(CapEvent::OpenFailed), CapAction::TryMode(1));
        assert_eq!(f.on(CapEvent::OpenFailed), CapAction::TryMode(2));
        assert_eq!(f.on(CapEvent::OpenFailed), CapAction::GiveUp);
    }

    #[test]
    fn fewer_modes_give_up_earlier() {
        let mut f = Fallback::new(1);
        f.start();
        assert_eq!(f.on(CapEvent::OpenFailed), CapAction::GiveUp);
    }

    #[test]
    fn no_first_frame_counts_as_failure() {
        let mut f = Fallback::new(3);
        f.start();
        assert_eq!(f.on(CapEvent::NoFirstFrame), CapAction::TryMode(1));
    }

    #[test]
    fn first_frame_runs() {
        let mut f = Fallback::new(3);
        f.start();
        f.on(CapEvent::OpenFailed);
        assert_eq!(f.on(CapEvent::FirstFrame), CapAction::Running);
        assert_eq!(f.mode(), 1);
    }

    #[test]
    fn lost_after_running_retries_forever() {
        let mut f = Fallback::new(1);
        f.start();
        f.on(CapEvent::FirstFrame);
        assert_eq!(f.on(CapEvent::Lost), CapAction::RetryLater);
        for _ in 0..10 {
            assert_eq!(f.on(CapEvent::RetryFailed), CapAction::RetryLater);
        }
        assert_eq!(f.mode(), 0);
    }
}
