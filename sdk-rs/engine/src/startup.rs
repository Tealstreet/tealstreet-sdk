//! Public discovery retry policy. Venues rebuild requests to refresh signatures.
use crate::{Cx, LogLevel, TimerId};

struct Read<K> {
    key: K,
    attempts: u32,
    timer: Option<TimerId>,
}

pub struct Backoff<K> {
    reads: Vec<Read<K>>,
}
impl<K> Default for Backoff<K> {
    fn default() -> Self {
        Self { reads: Vec::new() }
    }
}
impl<K: PartialEq> Backoff<K> {
    pub fn waiting(&self, key: &K) -> bool {
        self.reads
            .iter()
            .any(|read| &read.key == key && read.timer.is_some())
    }

    /// At most one retry per independent read; no write request enters this API.
    pub fn failed<P>(&mut self, cx: &mut Cx<'_, P>, key: K) {
        let index = match self.reads.iter().position(|read| read.key == key) {
            Some(index) => index,
            None => {
                self.reads.push(Read {
                    key,
                    attempts: 0,
                    timer: None,
                });
                self.reads.len() - 1
            }
        };
        let read = &mut self.reads[index];
        if read.timer.is_some() {
            return;
        }
        read.attempts = read.attempts.saturating_add(1);
        let delay = (1_000u32 << read.attempts.saturating_sub(1).min(5)).min(30_000);
        read.timer = Some(cx.set_timer(f64::from(delay)));
        cx.log(
            LogLevel::Warning,
            "Public discovery read failed; retry scheduled",
        );
    }

    pub fn succeeded<P>(&mut self, cx: &mut Cx<'_, P>, key: &K) {
        if let Some(index) = self.reads.iter().position(|read| &read.key == key)
            && let Some(timer) = self.reads.swap_remove(index).timer
        {
            cx.clear_timer(timer);
        }
    }
}
impl<K: Clone> Backoff<K> {
    pub fn on_timer(&mut self, timer: TimerId) -> Option<K> {
        let read = self
            .reads
            .iter_mut()
            .find(|read| read.timer == Some(timer))?;
        read.timer = None;
        Some(read.key.clone())
    }
}

/// Do not expose raw catalogue rows, which can arrive through authenticated APIs.
pub fn skipped_row<P>(cx: &mut Cx<'_, P>) {
    cx.log(LogLevel::Warning, "Skipped malformed public discovery row");
}

/// TS numeric coercion for public fields that allow blank strings.
pub fn number_or_zero(text: &str) -> Option<f64> {
    if text.trim().is_empty() {
        Some(0.0)
    } else {
        text.parse::<f64>().ok().filter(|value| value.is_finite())
    }
}

/// Retains positional identities when one row in a nested public list is invalid.
pub struct Row<T>(pub Option<T>);
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Row<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: &'de serde_json::value::RawValue = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self(serde_json::from_str(raw.get()).ok()))
    }
}
