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
    instance_id: Option<Arc<str>>,
    account_label: Option<Arc<str>>,
    pending_signs: Option<Arc<Mutex<PendingSigns>>>,
    order_receipts: Arc<Mutex<OrderReceipts>>,
}

#[derive(Default)]
struct OrderReceipts {
    sequence: u64,
    retired: bool,
}

enum PendingSigns {
    Waiting(Vec<String>),
    Registered(Out),
    Retired,
}

#[derive(Clone)]
struct Owner {
    current: Arc<Mutex<Weak<()>>>,
    captured: Arc<()>,
}

/// Retires one temporary producer without retiring an account's output.
pub(crate) struct OutputLifetime(Owner);

impl Drop for OutputLifetime {
    fn drop(&mut self) {
        *self.0.current.lock().unwrap() = Weak::new();
    }
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
    #[cfg(test)]
    pub(crate) fn queued(&self) -> usize {
        self.rx.len()
    }

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
    pub(crate) fn isolated(&self) -> (Self, OutputLifetime) {
        let captured = Arc::new(());
        let owner = Owner {
            current: Arc::new(Mutex::new(Arc::downgrade(&captured))),
            captured,
        };
        (
            Self {
                owner: Some(owner.clone()),
                ..self.clone()
            },
            OutputLifetime(owner),
        )
    }

    pub fn channel(capacity: usize) -> (Self, Receiver, Arc<Notify>) {
        let (tx, rx) = mpsc::channel(capacity);
        let overflow = Arc::new(Notify::new());
        (
            Self {
                tx,
                overflow: overflow.clone(),
                accounts: Arc::default(),
                owner: None,
                instance_id: None,
                account_label: None,
                pending_signs: None,
                order_receipts: Arc::default(),
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

    pub(crate) fn with_instance_id(&self, instance_id: Option<&str>) -> Self {
        Self {
            instance_id: instance_id.filter(|id| !id.is_empty()).map(Arc::from),
            pending_signs: None,
            order_receipts: Arc::default(),
            ..self.clone()
        }
    }

    pub(crate) fn with_account_label(&self, label: &str) -> Self {
        Self {
            account_label: Some(Arc::from(label)),
            ..self.clone()
        }
    }

    pub(crate) fn account_label<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.account_label.as_deref().unwrap_or(fallback)
    }

    pub(crate) fn order_binding(&self, instance_id: Option<&str>) -> bool {
        self.pending_signs.is_none()
            && !self.order_receipts.lock().unwrap().retired
            && instance_id.is_none_or(|id| !id.is_empty() && id.len() <= 128)
            && self.instance_id.as_deref() == instance_id
    }

    pub(crate) fn order_receipt_binding_bounded(&self) -> bool {
        self.instance_id.as_deref().is_none_or(|id| id.len() <= 128)
    }

    pub(crate) fn order_outcome(&self, name: &str, data: serde_json::Value) -> bool {
        self.sequenced_outcome("order.outcome", name, data)
    }

    pub(crate) fn action_active(&self) -> bool {
        self.owner.as_ref().is_none_or(|owner| owner.is_current())
            && self.order_binding(self.instance_id.as_deref())
    }
    pub(crate) fn action_outcome(&self, name: &str, data: serde_json::Value) -> bool {
        self.sequenced_outcome("order.actionOutcome", name, data)
    }
    fn sequenced_outcome(&self, kind: &str, name: &str, mut data: serde_json::Value) -> bool {
        let mut receipts = self.order_receipts.lock().unwrap();
        if receipts.retired {
            return false;
        }
        let Some(next) = receipts
            .sequence
            .checked_add(1)
            .filter(|next| *next <= 9_007_199_254_740_991)
        else {
            self.overflow.notify_one();
            self.send(crate::envelope::error(
                Some(name),
                "Order receipt sequence exhausted; reconnect",
            ));
            return false;
        };
        receipts.sequence = next;
        data["outcomeSeq"] = next.into();
        let text = crate::envelope::order_frame(kind, name, data.clone());
        // The registration ID is inserted by send; reserve its full bounded
        // UTF-8/JSON expansion before exposing a result to the router.
        let size = self.instance_id.as_deref().map_or(text.len(), |id| {
            crate::envelope::with_instance_id(text.clone(), id).len()
        });
        if kind == "order.actionOutcome" && size > crate::order_contract::OUTCOME_MAX {
            data["outcome"] = "unknown".into();
            data["result"] = serde_json::Value::Null;
            data["error"] = serde_json::json!({"message":"Action result exceeds receipt limit","outcome":"unknown"});
            return self.send(crate::envelope::order_frame(kind, name, data));
        }
        self.send(text)
    }

    pub(crate) fn retire_order_receipts(&self) {
        self.order_receipts.lock().unwrap().retired = true;
    }

    pub(crate) fn with_fresh_order_receipts(&self) -> Self {
        Self {
            order_receipts: Arc::default(),
            ..self.clone()
        }
    }

    pub(crate) fn shares_order_receipts(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.order_receipts, &other.order_receipts)
    }

    /// A resumed signer can reconnect before init supplies the new registration.
    pub(crate) fn awaiting_instance_id(&self) -> Self {
        Self {
            instance_id: None,
            pending_signs: Some(Arc::new(Mutex::new(PendingSigns::Waiting(Vec::new())))),
            order_receipts: Arc::default(),
            ..self.clone()
        }
    }

    pub(crate) fn finish_registration(&self, out: &Out) {
        if self.owner.as_ref().is_some_and(|owner| !owner.is_current()) {
            self.retire_registration();
            return;
        }
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
        self.retire_order_receipts();
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
        if self.owner.as_ref().is_some_and(|owner| !owner.is_current()) {
            return false;
        }
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
        let text = match &self.account_label {
            Some(label) => crate::envelope::with_account_label(text, label),
            None => text,
        };
        let text = match &self.instance_id {
            Some(instance_id) => crate::envelope::with_instance_id(text, instance_id),
            None => text,
        };
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
    async fn deferred_sign_registration_checks_generation_before_stamping() {
        for replace in [false, true] {
            let (out, mut sent, _) = Out::channel(8);
            let pending = out.account("acct").awaiting_instance_id();
            let request = r#"{"t":"sign-req","d":{"accountName":"acct","id":"deferred-A"}}"#;
            assert!(pending.send(request.into()));
            let (release, wait) = tokio::sync::oneshot::channel();
            let held = pending.clone();
            let callback = tokio::spawn(async move {
                let current = wait.await.unwrap();
                held.finish_registration(&current);
            });
            if replace {
                out.retire_account("acct");
            }
            let current = out.account("acct").with_instance_id(Some("registration-B"));
            assert!(release.send(current.clone()).is_ok());
            callback.await.unwrap();
            assert!(current.send(crate::envelope::msg("acct", "store", serde_json::json!({}))));
            let mut requests = 0;
            while let Ok(text) = sent.try_recv() {
                let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(frame["d"]["instanceId"], "registration-B");
                requests += usize::from(frame["t"] == "sign-req");
            }
            assert_eq!(requests, usize::from(!replace));
            assert_eq!(pending.send(request.into()), !replace);
        }
    }

    #[tokio::test]
    async fn order_receipt_bookkeeping_is_fixed_and_exhaustion_does_not_wrap() {
        assert!(std::mem::size_of::<OrderReceipts>() <= 16);
        let (out, mut sent, overflow) = Out::channel(1);
        for i in 0..=OUTBOUND_CAPACITY {
            assert!(out.order_outcome(
                &format!("account-{i}"),
                serde_json::json!({"outcome":"refused"})
            ));
            let frame: serde_json::Value =
                serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            assert_eq!(frame["d"]["outcomeSeq"], i + 1);
        }
        assert_eq!(Arc::strong_count(&out.order_receipts), 1);
        out.order_receipts.lock().unwrap().sequence = 9_007_199_254_740_991;
        assert!(!out.order_outcome("acct", serde_json::json!({"outcome":"unknown"})));
        overflow.notified().await;
        let frame: serde_json::Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        assert_eq!(frame["d"]["op"], "error");
        assert_eq!(
            out.order_receipts.lock().unwrap().sequence,
            9_007_199_254_740_991
        );
    }

    #[tokio::test]
    async fn order_receipts_follow_registration_lifetimes() {
        let (socket, mut sent, _) = Out::channel(8);
        let first = socket.with_instance_id(Some("first"));
        let second = socket.with_instance_id(Some("second"));
        for (out, expected) in [(&first, 1), (&first, 2), (&second, 1)] {
            assert!(out.order_outcome("acct", serde_json::json!({"outcome":"unknown"})));
            let frame: serde_json::Value =
                serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            assert_eq!(frame["d"]["outcomeSeq"], expected);
        }
        first.retire_registration();
        assert!(!first.order_outcome("acct", serde_json::json!({"outcome":"unknown"})));
        assert!(sent.try_recv().is_err());
        assert!(second.order_outcome("acct", serde_json::json!({"outcome":"unknown"})));
        let frame: serde_json::Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        assert_eq!(frame["d"]["outcomeSeq"], 2);
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
