//! Smoothed speed and ETA from a growing byte counter.

use std::time::{Duration, Instant};

/// Smoothing time constant, seconds.
const TAU: f64 = 2.0;
/// Updates closer together than this reuse the last sample.
const MIN_SAMPLE: Duration = Duration::from_millis(50);

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

pub struct Meter {
    start: Instant,
    last: Instant,
    last_done: u64,
    /// Moving average and its accumulated weight; `ema / weight` removes the bias of starting at 0.
    ema: f64,
    weight: f64,
    snap: Snapshot,
}

impl Meter {
    /// `start_done` = bytes already present (resume); they don't count toward speed.
    pub fn new(total: u64, start_done: u64) -> Meter {
        let now = Instant::now();
        Meter {
            start: now,
            last: now,
            last_done: start_done,
            ema: 0.0,
            weight: 0.0,
            snap: Snapshot { done: start_done, total, speed: 0.0, peak: 0.0, eta: None, elapsed: 0.0 },
        }
    }

    pub fn update(&mut self, done: u64) -> Snapshot {
        self.update_at(done, Instant::now())
    }

    /// Same as `update` with an explicit clock (tests).
    pub fn update_at(&mut self, done: u64, now: Instant) -> Snapshot {
        self.snap.done = done;
        let dt = now.saturating_duration_since(self.last);
        if dt < MIN_SAMPLE {
            return self.snap.clone();
        }
        let dt = dt.as_secs_f64();
        let alpha = 1.0 - (-dt / TAU).exp();
        self.ema += alpha * (done.saturating_sub(self.last_done) as f64 / dt - self.ema);
        self.weight += alpha * (1.0 - self.weight);
        self.last = now;
        self.last_done = done;

        let s = &mut self.snap;
        s.speed = self.ema / self.weight;
        s.elapsed = now.saturating_duration_since(self.start).as_secs_f64();
        let warm = s.elapsed >= TAU;
        if warm {
            s.peak = s.peak.max(s.speed);
        }
        s.eta = (warm && s.speed > 0.0).then(|| s.total.saturating_sub(done) as f64 / s.speed);
        s.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    /// Feeds `rate` bytes/s in 100 ms steps for `secs`, starting from `done`.
    fn run(m: &mut Meter, done: u64, rate: u64, secs: u64) -> Snapshot {
        let mut snap = None;
        for i in 1..=secs * 10 {
            snap = Some(m.update_at(done + rate / 10 * i, m.start + Duration::from_millis(100 * i)));
        }
        snap.unwrap()
    }

    #[test]
    fn converges_and_eta() {
        let mut m = Meter::new(2000 * MB, 0);
        let s = run(&mut m, 0, 100 * MB, 10);
        assert!((s.speed / (100 * MB) as f64 - 1.0).abs() < 0.02, "{s:?}");
        assert!((s.eta.unwrap() - 10.0).abs() < 0.3, "{s:?}");
        assert!(s.peak >= s.speed);
        assert_eq!(s.done, 1000 * MB);
    }

    #[test]
    fn resumed_bytes_not_speed() {
        let mut m = Meter::new(2000 * MB, 500 * MB);
        let s = run(&mut m, 500 * MB, 100 * MB, 5);
        assert!((s.speed / (100 * MB) as f64 - 1.0).abs() < 0.02, "{s:?}");
        assert!((s.eta.unwrap() - 10.0).abs() < 0.3, "{s:?}");
    }

    #[test]
    fn no_eta_while_warming_up() {
        let mut m = Meter::new(2000 * MB, 0);
        let s = m.update_at(190 * MB, m.start + Duration::from_millis(1900));
        assert_eq!((s.eta, s.peak), (None, 0.0));
        let s = m.update_at(200 * MB, m.start + Duration::from_millis(2000));
        assert!(s.eta.is_some());
    }

    #[test]
    fn fast_updates_reuse_sample() {
        let mut m = Meter::new(100, 0);
        let a = m.update_at(10, m.start + Duration::from_millis(100));
        let b = m.update_at(20, m.start + Duration::from_millis(120));
        assert_eq!(b, Snapshot { done: 20, ..a });
    }
}
