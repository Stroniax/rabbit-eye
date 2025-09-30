#[derive(Clone, Copy, Debug)]
pub struct ScheduleOptions {
    interval: std::time::Duration,
    overlap_behavior: ScheduleOverlap,
}

impl ScheduleOptions {
    pub fn new(interval: std::time::Duration, overlap_behavior: ScheduleOverlap) -> Self {
        Self {
            interval,
            overlap_behavior,
        }
    }

    pub fn interval(&self) -> std::time::Duration {
        self.interval
    }
}

impl Default for ScheduleOptions {
    fn default() -> Self {
        let interval = std::time::Duration::from_secs(5);
        let overlap_behavior = ScheduleOverlap::default();
        Self::new(interval, overlap_behavior)
    }
}

/// Describes how to handle a schedule when work is still ongoing from a previous interval.
#[derive(Clone, Copy, Debug)]
pub enum ScheduleOverlap {
    /// Abort the previously running async task.
    AbortPrevious,

    /// Do not run the new work. Instaed, allow the previous work to keep running, and
    /// ignore the current prompt for scheduled work.
    SkipNew {
        /// The maximum number of scheduled intervals to wait before aborting the running task.
        max: usize,
    },

    /// Continue the previous task and start the new task.
    Overlap {
        /// Abort if this many tasks are running concurrently.
        max: usize,
    },
}

impl Default for ScheduleOverlap {
    fn default() -> Self {
        ScheduleOverlap::AbortPrevious
    }
}
