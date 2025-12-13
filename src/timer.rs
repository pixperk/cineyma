use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Handle to a scheduled timer that can be cancelled
/// When dropped without calling cancel(), the timer continues running
#[derive(Clone)]
pub struct TimerHandle {
    cancelled: Arc<AtomicBool>,
}

impl TimerHandle {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cancel the timer - it will not fire
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Check if the timer has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
