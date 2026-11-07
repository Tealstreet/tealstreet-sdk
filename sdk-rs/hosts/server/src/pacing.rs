//! Account starts are paced per process (one egress IP), so a mass reconnect
//! queues inits instead of stampeding venue logins.
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

pub struct Pacer {
    spacing: Duration,
    jitter: Duration,
    next: Mutex<Option<Instant>>,
}

impl Pacer {
    pub fn new(per_second: u32, jitter: Duration) -> Self {
        Self {
            spacing: Duration::from_secs(1) / per_second.max(1),
            jitter,
            next: Mutex::new(None),
        }
    }

    /// Reserves the next start slot and returns when it arrives.
    pub async fn admit(&self) {
        let slot = {
            let mut next = self.next.lock().unwrap();
            let now = Instant::now();
            let slot = next.map_or(now, |next| next.max(now));
            *next = Some(slot + self.spacing + self.random_jitter());
            slot
        };
        tokio::time::sleep_until(slot).await;
    }

    fn random_jitter(&self) -> Duration {
        let mut bytes = [0u8; 2];
        let _ = getrandom::getrandom(&mut bytes);
        let fraction = f64::from(u16::from_le_bytes(bytes)) / f64::from(u16::MAX);
        self.jitter.mul_f64(fraction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn starts_are_spaced_and_the_first_is_immediate() {
        let pacer = Pacer::new(10, Duration::ZERO);
        let start = Instant::now();
        pacer.admit().await;
        assert_eq!(start.elapsed(), Duration::ZERO);
        for _ in 0..4 {
            pacer.admit().await;
        }
        assert_eq!(start.elapsed(), Duration::from_millis(400));
    }
}
