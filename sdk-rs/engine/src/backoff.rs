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

    /// `delay` shortened by up to a quarter, seeded from host time, so clients
    /// dropped together do not reconnect together. Never longer than `delay`.
    pub fn jittered(&self, attempt: u32, now_ms: f64) -> f64 {
        let seed = now_ms.to_bits() ^ u64::from(attempt).rotate_left(32);
        jitter(self.delay(attempt), split_mix(seed))
    }
}

/// Scales `delay` into `[0.75, 1.0] * delay` from 53 random `bits`.
pub fn jitter(delay: f64, bits: u64) -> f64 {
    let unit = (bits >> 11) as f64 / ((1u64 << 53) as f64);
    delay * (0.75 + unit * 0.25)
}

fn split_mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
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
    fn jitter_stays_within_a_quarter_below_the_delay_and_varies_with_time() {
        let schedule = Backoff::new(1_000.0, 4, 30_000.0);
        let delays: Vec<f64> = (0..64)
            .map(|tick| schedule.jittered(2, 1_700_000_000_000.0 + f64::from(tick)))
            .collect();
        assert!(
            delays
                .iter()
                .all(|delay| (3_000.0..=4_000.0).contains(delay))
        );
        let spread = delays.iter().cloned().fold(f64::MIN, f64::max)
            - delays.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread > 500.0, "spread {spread}");
        assert_eq!(schedule.jittered(2, 5.0), schedule.jittered(2, 5.0));
    }

    #[test]
    fn unbounded_steps_stop_at_the_cap() {
        let schedule = Backoff::new(250.0, u32::MAX, 30_000.0);
        assert_eq!(schedule.delay(0), 250.0);
        assert_eq!(schedule.delay(7), 30_000.0);
        assert_eq!(schedule.delay(u32::MAX), 30_000.0);
    }
}
