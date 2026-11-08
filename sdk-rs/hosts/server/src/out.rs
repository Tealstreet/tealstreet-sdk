//! A socket's bounded outbound queue. A client that stops reading fills it;
//! the socket then closes with 1013 rather than buffering without limit.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{Notify, mpsc};

pub const OUTBOUND_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct Out {
    tx: mpsc::Sender<Queued>,
    overflow: Arc<Notify>,
    accounts: Arc<Mutex<HashMap<String, Owner>>>,
    owner: Option<Owner>,
}

#[derive(Clone)]
struct Owner {
    current: Arc<Mutex<Weak<()>>>,
    captured: Arc<()>,
}

impl Owner {
    fn is_current(&self) -> bool {
        Weak::ptr_eq(
            &Arc::downgrade(&self.captured),
            &self.current.lock().unwrap(),
        )
    }
}

struct Queued {
    text: String,
    owner: Option<Owner>,
}

impl Queued {
    fn into_current(self) -> Option<String> {
        self.owner
            .as_ref()
            .is_none_or(Owner::is_current)
            .then_some(self.text)
    }
}

/// Only current-owner messages leave the queue, including when draining before close.
pub struct Receiver {
    rx: mpsc::Receiver<Queued>,
}

impl Receiver {
    pub async fn recv(&mut self) -> Option<String> {
        loop {
            if let Some(text) = self.rx.recv().await?.into_current() {
                return Some(text);
            }
        }
    }

    pub fn try_recv(&mut self) -> Result<String, mpsc::error::TryRecvError> {
        loop {
            if let Some(text) = self.rx.try_recv()?.into_current() {
                return Ok(text);
            }
        }
    }
}

impl Out {
    pub fn channel(capacity: usize) -> (Self, Receiver, Arc<Notify>) {
        let (tx, rx) = mpsc::channel(capacity);
        let overflow = Arc::new(Notify::new());
        (
            Self {
                tx,
                overflow: overflow.clone(),
                accounts: Arc::default(),
                owner: None,
            },
            Receiver { rx },
            overflow,
        )
    }

    /// Capture once before an account's callback, task or pump can defer output.
    pub fn account(&self, name: &str) -> Self {
        let mut accounts = self.accounts.lock().unwrap();
        // A drained, unjoined account has no producer or queued output retaining its token.
        accounts.retain(|_, owner| Arc::strong_count(&owner.captured) > 1);
        let owner = accounts
            .entry(name.to_owned())
            .or_insert_with(|| {
                let captured = Arc::new(());
                Owner {
                    current: Arc::new(Mutex::new(Arc::downgrade(&captured))),
                    captured,
                }
            })
            .clone();
        Self {
            owner: Some(owner),
            ..self.clone()
        }
    }

    /// Retire even already-queued output; a subsequent init gets a fresh generation.
    pub fn retire_account(&self, name: &str) {
        if let Some(owner) = self.accounts.lock().unwrap().remove(name) {
            *owner.current.lock().unwrap() = Weak::new();
        }
    }

    /// Never blocks: a full queue signals overflow and drops this message,
    /// and the socket closes so a resume recovers with a fresh snapshot.
    pub fn send(&self, text: String) -> bool {
        if self.owner.as_ref().is_some_and(|owner| !owner.is_current()) {
            return false;
        }
        match self.tx.try_send(Queued {
            text,
            owner: self.owner.clone(),
        }) {
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

    #[test]
    fn drained_unjoined_names_do_not_accumulate_owner_records() {
        let (out, mut rx, _) = Out::channel(1);
        for index in 0..100 {
            assert!(
                out.account(&format!("unjoined-{index}"))
                    .send("refused".into())
            );
            assert_eq!(rx.try_recv().as_deref(), Ok("refused"));
        }
        assert_eq!(out.accounts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn dequeue_drops_retired_owner_and_preserves_other_accounts() {
        let (out, mut rx, _) = Out::channel(8);
        let a = out.account("acct");
        let other = out.account("other");
        assert!(a.send("A current".into()));
        assert_eq!(rx.recv().await.as_deref(), Some("A current"));
        assert!(a.send("A queued".into()));
        assert!(other.send("other queued".into()));
        out.retire_account("acct");
        let b = out.account("acct");
        assert!(b.send("B current".into()));
        assert!(out.send("session".into()));
        assert_eq!(rx.recv().await.as_deref(), Some("other queued"));
        assert_eq!(rx.recv().await.as_deref(), Some("B current"));
        assert_eq!(rx.recv().await.as_deref(), Some("session"));
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    }

    #[tokio::test]
    async fn delayed_callback_keeps_its_captured_owner_after_same_name_replacement() {
        let (out, mut rx, overflow) = Out::channel(1);
        let a = out.account("acct");
        let (release, wait) = tokio::sync::oneshot::channel();
        let callback = tokio::spawn(async move {
            wait.await.unwrap();
            a.send("late A reply".into())
        });
        out.retire_account("acct");
        let b = out.account("acct");
        assert!(b.send("B reply".into()));
        release.send(()).unwrap();
        assert!(
            !callback.await.unwrap(),
            "stale output is refused before enqueue"
        );
        assert_eq!(rx.try_recv().as_deref(), Ok("B reply"));
        assert_eq!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), overflow.notified())
                .await
                .is_err(),
            "dropping stale work does not overflow the full queue"
        );
    }

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
