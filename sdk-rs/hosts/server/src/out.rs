//! A socket's bounded outbound queue. A client that stops reading fills it;
//! the socket then closes with 1013 rather than buffering without limit.
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, mpsc};

pub const OUTBOUND_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct Out {
    tx: mpsc::Sender<String>,
    overflow: Arc<Notify>,
    instance_id: Option<Arc<str>>,
    pending_signs: Option<Arc<Mutex<PendingSigns>>>,
    order_sequences: Arc<Mutex<std::collections::HashMap<String, u64>>>,
}

enum PendingSigns {
    Waiting(Vec<String>),
    Registered(Out),
    Retired,
}

impl Out {
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<String>, Arc<Notify>) {
        let (tx, rx) = mpsc::channel(capacity);
        let overflow = Arc::new(Notify::new());
        (
            Self {
                tx,
                overflow: overflow.clone(),
                instance_id: None,
                pending_signs: None,
                order_sequences: Arc::default(),
            },
            rx,
            overflow,
        )
    }

    pub(crate) fn with_instance_id(&self, instance_id: Option<&str>) -> Self {
        Self {
            instance_id: instance_id.filter(|id| !id.is_empty()).map(Arc::from),
            pending_signs: None,
            ..self.clone()
        }
    }

    pub(crate) fn order_binding(&self, instance_id: Option<&str>) -> bool {
        instance_id.is_none_or(|id| !id.is_empty() && id.len() <= 128)
            && self.instance_id.as_deref() == instance_id
    }

    pub(crate) fn order_outcome(&self, name: &str, mut data: serde_json::Value) -> bool {
        let mut sequences = self.order_sequences.lock().unwrap();
        let current = sequences.entry(name.to_owned()).or_default();
        let Some(next) = current
            .checked_add(1)
            .filter(|next| *next <= 9_007_199_254_740_991)
        else {
            return false;
        };
        *current = next;
        data["outcomeSeq"] = next.into();
        self.send(crate::envelope::order_frame("order.outcome", name, data))
    }

    /// A resumed signer can reconnect before init supplies the new registration.
    pub(crate) fn awaiting_instance_id(&self) -> Self {
        Self {
            instance_id: None,
            pending_signs: Some(Arc::new(Mutex::new(PendingSigns::Waiting(Vec::new())))),
            ..self.clone()
        }
    }

    pub(crate) fn finish_registration(&self, out: &Out) {
        let Some(pending) = &self.pending_signs else {
            return;
        };
        let frames = {
            let mut state = pending.lock().unwrap();
            if !matches!(*state, PendingSigns::Waiting(_)) {
                return;
            }
            match std::mem::replace(&mut *state, PendingSigns::Registered(out.clone())) {
                PendingSigns::Waiting(frames) => frames,
                _ => unreachable!(),
            }
        };
        for frame in frames {
            out.send(frame);
        }
    }

    pub(crate) fn retire_registration(&self) {
        if let Some(pending) = &self.pending_signs {
            *pending.lock().unwrap() = PendingSigns::Retired;
        }
    }

    pub(crate) fn outbox(&self) -> safe_cex_remote_signer::Outbox {
        let out = self.clone();
        Arc::new(move |text| out.send(text))
    }

    /// Never blocks: a full queue signals overflow and drops this message,
    /// and the socket closes so a resume recovers with a fresh snapshot.
    pub fn send(&self, text: String) -> bool {
        if let Some(pending) = &self.pending_signs {
            let mut state = pending.lock().unwrap();
            let signing = text.starts_with(r#"{"t":"sign-req","d":"#);
            match &mut *state {
                PendingSigns::Registered(out) => return out.send(text),
                PendingSigns::Retired if signing => return false,
                PendingSigns::Waiting(frames) if signing => {
                    if frames.len() >= self.tx.max_capacity() {
                        self.overflow.notify_one();
                        return false;
                    }
                    frames.push(text);
                    return true;
                }
                _ => {}
            }
        }
        let text = match &self.instance_id {
            Some(instance_id) => crate::envelope::with_instance_id(text, instance_id),
            None => text,
        };
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

    #[tokio::test]
    async fn resumed_sign_requests_are_bounded_and_stamp_only_after_registration() {
        let (out, mut sent, overflow) = Out::channel(2);
        let pending = out.awaiting_instance_id();
        let request = r#"{"t":"sign-req","d":{"accountName":"acct","id":"held"}}"#;
        assert!(pending.send(request.into()));
        assert!(pending.send(request.into()));
        assert!(sent.try_recv().is_err());
        assert!(!pending.send(request.into()));
        overflow.notified().await;
        pending.finish_registration(&out.with_instance_id(Some("resumed")));
        for _ in 0..2 {
            let frame: serde_json::Value =
                serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            assert_eq!(frame["d"]["instanceId"], "resumed");
        }
    }

    #[tokio::test]
    async fn a_resumed_registration_cannot_be_rebound_or_revived_after_replacement() {
        let (out, mut sent, _) = Out::channel(4);
        let pending = out.awaiting_instance_id();
        let first = out.with_instance_id(Some("A"));
        pending.finish_registration(&first);
        pending.finish_registration(&out.with_instance_id(Some("B")));
        assert!(pending.send(crate::envelope::msg("acct", "store", serde_json::json!({}))));
        let frame: serde_json::Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        assert_eq!(frame["d"]["instanceId"], "A");

        let retired = out.awaiting_instance_id();
        let request = r#"{"t":"sign-req","d":{"accountName":"acct","id":"retired"}}"#;
        assert!(retired.send(request.into()));
        retired.retire_registration();
        retired.finish_registration(&out.with_instance_id(Some("B")));
        assert!(!retired.send(request.into()));
        assert!(sent.try_recv().is_err());
    }
}
