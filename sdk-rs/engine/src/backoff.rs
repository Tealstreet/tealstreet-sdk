//! Exponential retry delays: `initial_ms * 2^min(attempt, max_steps)`, capped.
//! Venues keep their own attempt counters and constants; this owns the formula.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backoff {
    pub initial_ms: f64,
    pub max_steps: u32,
    pub cap_ms: f64,
}

impl Backoff {
    pub const fn new(initial_ms: f64, max_steps: u32, cap_ms: f64) -> Self {
        Self {
            initial_ms,
            max_steps,
            cap_ms,
        }
    }

    /// `attempt` 0 waits `initial_ms`; each further attempt doubles until `max_steps`.
    pub fn delay(&self, attempt: u32) -> f64 {
        let steps = attempt.min(self.max_steps).min(i32::MAX as u32) as i32;
        (self.initial_ms * 2f64.powi(steps)).min(self.cap_ms)
    }
}

/// Scales `delay` into `[0.75, 1.0] * delay` from 53 random `bits`.
pub fn jitter(delay: f64, bits: u64) -> f64 {
    let unit = (bits >> 11) as f64 / ((1u64 << 53) as f64);
    delay * (0.75 + unit * 0.25)
}

#[cfg(test)]
mod tests {
    use super::Backoff;

    #[test]
    fn doubles_from_initial_until_steps_then_caps() {
        let schedule = Backoff::new(500.0, 5, 30_000.0);
        let delays: Vec<f64> = (0..8).map(|attempt| schedule.delay(attempt)).collect();
        assert_eq!(
            delays,
            [
                500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0, 16_000.0, 16_000.0
            ]
        );
        assert_eq!(Backoff::new(1_000.0, 6, 30_000.0).delay(9), 30_000.0);
    }

    #[test]
    fn jitter_scales_into_the_last_quarter_below_the_delay() {
        assert_eq!(super::jitter(4_000.0, 0), 3_000.0);
        assert!(super::jitter(4_000.0, u64::MAX) <= 4_000.0);
        assert!(super::jitter(4_000.0, u64::MAX) > 3_999.0);
    }

    #[test]
    fn unbounded_steps_stop_at_the_cap() {
        let schedule = Backoff::new(250.0, u32::MAX, 30_000.0);
        assert_eq!(schedule.delay(0), 250.0);
        assert_eq!(schedule.delay(7), 30_000.0);
        assert_eq!(schedule.delay(u32::MAX), 30_000.0);
    }
}
