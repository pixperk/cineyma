use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SupervisorStrategy {
    ///stop the actor on failure (default)
    #[default]
    Stop,
    ///restart the actor on failure
    Restart { max_restarts: u32, within: Duration },
    ///escalate to parent supervisor
    Escalate,
}

impl SupervisorStrategy {
    pub fn restart(max_restarts: u32, within: Duration) -> Self {
        Self::Restart {
            max_restarts,
            within,
        }
    }
}

///Track restart history for an actor
pub struct RestartTracker {
    ///Timestamps of recent restarts
    restart_times: Vec<std::time::Instant>,
    max_restarts: u32,
    within: Duration,
}

impl RestartTracker {
    pub fn new(max_restarts: u32, within: Duration) -> Self {
        Self {
            restart_times: Vec::new(),
            max_restarts,
            within,
        }
    }

    ///record a restart and return if next restart is allowed
    pub fn record_restart(&mut self) -> bool {
        let now = Instant::now();

        //remove old restarts
        self.restart_times
            .retain(|&time| now.duration_since(time) <= self.within);

        if self.restart_times.len() < self.max_restarts as usize {
            self.restart_times.push(now);
            true
        } else {
            false
        }
    }
}
