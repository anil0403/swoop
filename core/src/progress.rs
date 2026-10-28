//! Smoothed speed and ETA from a growing byte counter.

use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub done: u64,
    pub total: u64,
    /// Bytes/s, exponentially smoothed over ~2 s.
    pub speed: f64,
    /// Highest smoothed speed seen.
    pub peak: f64,
    /// Seconds remaining; None during the first ~2 s or while speed is 0.
    pub eta: Option<f64>,
    /// Seconds since the meter started.
    pub elapsed: f64,
}

pub struct Meter {}

impl Meter {
    /// `start_done` = bytes already present (resume); they don't count toward speed.
    pub fn new(total: u64, start_done: u64) -> Meter {
        todo!("{total} {start_done}")
    }

    pub fn update(&mut self, done: u64) -> Snapshot {
        self.update_at(done, Instant::now())
    }

    /// Same as `update` with an explicit clock (tests).
    pub fn update_at(&mut self, done: u64, now: Instant) -> Snapshot {
        todo!("{done} {now:?}")
    }
}
