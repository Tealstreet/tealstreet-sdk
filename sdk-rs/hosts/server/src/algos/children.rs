//! Account-owned evidence for TWAP amendment. Disappearing from the active
//! chaser list does not prove that a child's last order was canceled.
use super::*;
use safe_cex_algo_twap::{ChaserProgress, ChaserRequest, factory::TwapChasers};
use std::collections::HashSet;

pub(super) struct TwapHistory {
    delegate: Rc<dyn TwapChasers>,
    children: RefCell<HashMap<String, HashSet<String>>>,
    restored: RefCell<HashSet<String>>,
}
impl TwapHistory {
    pub fn new(delegate: Rc<dyn TwapChasers>) -> Self {
        Self {
            delegate,
            children: RefCell::default(),
            restored: RefCell::default(),
        }
    }
    pub fn observe_restore(&self, before: &[Value], after: &[Value]) {
        // Observe accepted resident rows, not unbounded client-supplied data.
        for row in after {
            if let Some(id) = row["id"].as_str()
                && !before.iter().any(|old| old["id"] == id)
            {
                self.restored.borrow_mut().insert(id.into());
            }
        }
    }
    // Factory capacity bounds the live parent set. A completed/retired id is
    // no longer amendable and cannot be revived by native state restoration.
    pub fn prune(&self, active: &[Value]) {
        let live: HashSet<_> = active.iter().filter_map(|row| row["id"].as_str()).collect();
        self.children
            .borrow_mut()
            .retain(|id, _| live.contains(id.as_str()));
        self.restored
            .borrow_mut()
            .retain(|id| live.contains(id.as_str()));
    }
    pub fn amendable(&self, id: &str) -> bool {
        // Restored state does not carry a complete child-order history. It can
        // use the existing explicit resume/stop paths, never guessed replacement.
        if self.restored.borrow().contains(id) {
            return false;
        }
        let children: Vec<_> = self
            .children
            .borrow()
            .get(id)
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default();
        children.into_iter().all(|child| {
            self.progress(&child)
                .is_some_and(|p| p.ended && !p.order_may_be_open)
        })
    }
}
impl TwapChasers for TwapHistory {
    fn start_with_id(&self, cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String> {
        self.delegate.start_with_id(cx, request)?;
        self.children
            .borrow_mut()
            .entry(request.twap_id.clone())
            .or_default()
            .insert(request.id.clone());
        Ok(())
    }
    fn progress(&self, id: &str) -> Option<ChaserProgress> {
        let progress = self.delegate.progress(id);
        if progress.is_some_and(|p| p.ended && !p.order_may_be_open) {
            for children in self.children.borrow_mut().values_mut() {
                children.remove(id);
            }
        }
        progress
    }
    fn halt(&self, cx: &AlgoCx, id: &str) {
        self.delegate.halt(cx, id);
    }
    fn remove(&self, cx: &AlgoCx, id: &str) {
        self.delegate.remove(cx, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Delegate(Option<ChaserProgress>);
    impl TwapChasers for Delegate {
        fn start_with_id(&self, _: &AlgoCx, _: &ChaserRequest) -> Result<(), String> {
            Ok(())
        }
        fn progress(&self, _: &str) -> Option<ChaserProgress> {
            self.0
        }
        fn halt(&self, _: &AlgoCx, _: &str) {}
        fn remove(&self, _: &AlgoCx, _: &str) {}
    }
    #[test]
    fn j6b_missing_or_uncertain_ended_child_is_not_proof_for_replacement() {
        for progress in [
            None,
            Some(ChaserProgress {
                remaining_size: 0.0,
                stopped: true,
                ended: true,
                order_may_be_open: true,
            }),
            Some(ChaserProgress {
                remaining_size: 0.0,
                stopped: true,
                ended: false,
                order_may_be_open: false,
            }),
        ] {
            let history = TwapHistory::new(Rc::new(Delegate(progress)));
            history
                .children
                .borrow_mut()
                .insert("parent".into(), HashSet::from(["child".into()]));
            assert!(!history.amendable("parent"));
        }
        let history = TwapHistory::new(Rc::new(Delegate(Some(ChaserProgress {
            remaining_size: 0.0,
            stopped: true,
            ended: true,
            order_may_be_open: false,
        }))));
        history
            .children
            .borrow_mut()
            .insert("parent".into(), HashSet::from(["child".into()]));
        assert!(history.amendable("parent"));
        assert!(history.children.borrow()["parent"].is_empty());
    }
    #[test]
    fn j6b_restoration_is_not_child_history_and_cannot_poison_an_existing_live_job() {
        let history = TwapHistory::new(Rc::new(Delegate(None)));
        history.observe_restore(
            &[serde_json::json!({"id":"live"})],
            &[
                serde_json::json!({"id":"restored"}),
                serde_json::json!({"id":"live"}),
            ],
        );
        assert!(!history.amendable("restored"));
        assert!(history.amendable("live"));
        history.prune(&[]);
        assert!(history.restored.borrow().is_empty());
    }
}
