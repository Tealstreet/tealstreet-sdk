//! Account-owned retention, independent of the socket resume grace.
use super::*;
use tokio::time::Instant;

pub(super) const DELEGATED_RETENTION: Duration = Duration::from_secs(48 * 60 * 60);

pub(super) struct Retention {
    generation: u64,
    pub deadline: Instant,
    task: tokio::task::AbortHandle,
}

impl Holders {
    pub(super) fn cancel_retention(&mut self) {
        if let Some(retention) = self.retention.take() {
            retention.task.abort();
        }
    }
}

impl Registry {
    /// Called after a real holder leaves. Only the current account can arm work.
    pub(super) fn retain_delegated(self: &Arc<Self>, account: &Arc<Account>) {
        let Some(resident) = &account.delegated else {
            return;
        };
        let accounts = self.accounts.lock().unwrap();
        if !accounts
            .get(&(account.user.clone(), account.name.clone()))
            .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            return;
        }
        let mut holders = account.holders.lock().unwrap();
        if holders.disposed
            || holders.retention.is_some()
            || holders.by_id.values().any(Option::is_some)
        {
            return;
        }
        let deadline = Instant::now() + DELEGATED_RETENTION;
        let generation = self.next_id();
        resident.retain_until(deadline);
        let registry = Arc::downgrade(self);
        let captured = Arc::downgrade(account);
        let task = tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            if let (Some(registry), Some(account)) = (registry.upgrade(), captured.upgrade()) {
                registry.expire_delegated(&account, generation);
            }
        });
        holders.retention = Some(Retention {
            generation,
            deadline,
            task: task.abort_handle(),
        });
    }

    fn expire_delegated(&self, account: &Arc<Account>, generation: u64) {
        // Map identity, account incarnation, timer generation and live admission
        // are checked under the same locks before any disposal can be attributed.
        let mut accounts = self.accounts.lock().unwrap();
        let key = (account.user.clone(), account.name.clone());
        if !accounts
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            return;
        }
        let mut holders = account.holders.lock().unwrap();
        if holders.disposed
            || holders.by_id.values().any(Option::is_some)
            || !holders.retention.as_ref().is_some_and(|retention| {
                retention.generation == generation && Instant::now() >= retention.deadline
            })
        {
            return;
        }
        holders.disposed = true;
        holders.retention = None;
        if let Some(resident) = &account.delegated {
            resident.revoke();
        }
        accounts.remove(&key);
        drop(holders);
        drop(accounts);
        let stopped = self.keep_notice(account, "delegated-retention");
        self.dispose_for(
            account,
            Disposal::NoClient48h,
            "no client for 48 h",
            Some(stopped),
        );
    }

    /// Explicit removal/disable also applies while this user's account is retained.
    pub(crate) fn dispose_delegated(&self, user: &str, name: &str) {
        let account = self
            .accounts
            .lock()
            .unwrap()
            .get(&(user.into(), name.into()))
            .cloned();
        if let Some(account) = account.filter(|account| account.delegated.is_some()) {
            self.dispose_for(&account, Disposal::PageDisposed, "account disposed", None);
        }
    }
}
