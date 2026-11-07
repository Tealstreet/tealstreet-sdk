//! A socket's bounded outbound queue. A client that stops reading fills it;
//! the socket then closes with 1013 rather than buffering without limit.
use std::sync::Arc;
use tokio::sync::{Notify, mpsc};

pub const OUTBOUND_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct Out {
    tx: mpsc::Sender<String>,
    overflow: Arc<Notify>,
}

impl Out {
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<String>, Arc<Notify>) {
        let (tx, rx) = mpsc::channel(capacity);
        let overflow = Arc::new(Notify::new());
        (
            Self {
                tx,
                overflow: overflow.clone(),
            },
            rx,
            overflow,
        )
    }

    /// Never blocks: a full queue signals overflow and drops this message,
    /// and the socket closes so a resume recovers with a fresh snapshot.
    pub fn send(&self, text: String) -> bool {
        match self.tx.try_send(text) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow.notify_one();
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_full_queue_signals_overflow_instead_of_growing() {
        let (out, _rx, overflow) = Out::channel(2);
        assert!(out.send("a".into()) && out.send("b".into()));
        assert!(!out.send("c".into()));
        tokio::time::timeout(std::time::Duration::from_millis(50), overflow.notified())
            .await
            .expect("overflow is signalled");
    }
}
