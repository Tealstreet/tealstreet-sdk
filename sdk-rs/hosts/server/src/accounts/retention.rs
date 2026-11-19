//! Account-owned retention, independent of the socket resume grace.
use super::*;
use tokio::time::Instant;

pub(super) const DELEGATED_RETENTION: Duration = Duration::from_secs(48 * 60 * 60);

pub(super) struct Retention {
    generation: u64,
    pub deadline: Instant,
    /// Wall-clock deadline, for host notifications only.
    deadline_ms: u64,
    proxy_expired: bool,
    pub(super) labels: std::collections::BTreeSet<String>,
    task: tokio::task::AbortHandle,
}

#[cfg(test)]
impl Retention {
    pub(super) fn abort_cleanup_for_test(&self) {
        self.task.abort();
    }
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
    pub(super) fn retain_delegated(self: &Arc<Self>, account: &Arc<Account>, labels: Vec<String>) {
        let Some(resident) = &account.delegated() else {
            return;
        };
        let accounts = self.accounts.lock().unwrap();
        if !accounts
            .get(&(account.user.clone(), account.registry_name().to_owned()))
            .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            return;
        }
        let mut holders = account.holders.lock().unwrap();
        // A nonfinal client still contributes its labels. Only clock creation
        // waits for the last live client; revocation lookup must not forget it.
        if !holders.disposed {
            holders.departed_labels.extend(labels.iter().cloned());
            // Parked holders will lose their label map when resume grace ends,
            // even when another admitted holder prevents the retention clock.
            let parked_labels: Vec<_> = holders
                .labels
                .iter()
                .filter(|(id, _)| holders.by_id.get(id).is_some_and(Option::is_none))
                .flat_map(|(_, labels)| labels.keys().cloned())
                .collect();
            holders.departed_labels.extend(parked_labels);
        }
        if holders.disposed
            || holders.retention.is_some()
            || holders.by_id.values().any(Option::is_some)
        {
            return;
        }
        let now = Instant::now();
        // #3424 / 8625fd869: unusable proxy authority stops and erases. AGT-37
        // (2026-11-20) made this the permanent contract; there is no renewal.
        let (deadline, proxy_expired) = if account.proxy_authority_required() {
            let maximum_ms = DELEGATED_RETENTION.as_secs() * 1000;
            let remaining_ms = account
                .token
                .0
                .read()
                .ok()
                .and_then(|current| {
                    current
                        .as_ref()
                        .map(|(_, expiry)| expiry.saturating_sub(now_ms()).min(maximum_ms))
                })
                .unwrap_or(0);
            (
                now + Duration::from_millis(remaining_ms),
                remaining_ms < maximum_ms,
            )
        } else {
            (now + DELEGATED_RETENTION, false)
        };
        let generation = self.next_id();
        resident.retain_until(deadline);
        let interval = deadline.saturating_duration_since(now);
        let deadline_ms = now_ms().saturating_add(interval.as_millis() as u64);
        // AGT-37: armed with, and aborted by, the retention task; never when off
        // or when the interval is shorter than the lead.
        let warn_at = (self.host_notify.is_some() && interval >= crate::host_notify::WARNING_LEAD)
            .then(|| deadline - crate::host_notify::WARNING_LEAD);
        let registry = Arc::downgrade(self);
        let captured = Arc::downgrade(account);
        let task = tokio::spawn(async move {
            if let Some(warn_at) = warn_at {
                tokio::time::sleep_until(warn_at).await;
                if let (Some(registry), Some(account)) = (registry.upgrade(), captured.upgrade()) {
                    registry.warn_delegated(&account, generation);
                }
            }
            tokio::time::sleep_until(deadline).await;
            if let (Some(registry), Some(account)) = (registry.upgrade(), captured.upgrade()) {
                registry.expire_delegated(&account, generation);
            }
        });
        let labels = holders
            .departed_labels
            .iter()
            .cloned()
            .chain(
                holders
                    .labels
                    .values()
                    .flat_map(|labels| labels.keys().cloned()),
            )
            .chain(std::iter::once(account.name.clone()))
            .collect();
        holders.retention = Some(Retention {
            generation,
            labels,
            deadline,
            deadline_ms,
            proxy_expired,
            task: task.abort_handle(),
        });
    }

    /// The warning re-checks the same identity and generation as expiry does.
    fn warn_delegated(&self, account: &Arc<Account>, generation: u64) {
        let accounts = self.accounts.lock().unwrap();
        let key = (account.user.clone(), account.registry_name().to_owned());
        if !accounts
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            return;
        }
        let holders = account.holders.lock().unwrap();
        let Some(retention) = holders.retention.as_ref().filter(|retention| {
            retention.generation == generation
                && !holders.disposed
                && !holders.by_id.values().any(Option::is_some)
        }) else {
            return;
        };
        let draft =
            crate::host_notify::shutdown_warning(retention.proxy_expired, retention.deadline_ms);
        drop(holders);
        drop(accounts);
        self.host_notify(account, draft);
    }

    fn expire_delegated(&self, account: &Arc<Account>, generation: u64) {
        // Map identity, account incarnation, timer generation and live admission
        // are checked under the same locks before any disposal can be attributed.
        let mut accounts = self.accounts.lock().unwrap();
        let key = (account.user.clone(), account.registry_name().to_owned());
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
        let shutdown = holders.retention.as_ref().map(|state| {
            crate::host_notify::retention_shutdown(state.proxy_expired, state.deadline_ms)
        });
        let (disposal, reason) = if holders
            .retention
            .as_ref()
            .is_some_and(|state| state.proxy_expired)
        {
            (Disposal::ProxyTokenExpired, "proxy-token-expired")
        } else {
            (Disposal::NoClient48h, "no client for 48 h")
        };
        holders.disposed = true;
        holders.retention = None;
        if let Some(resident) = &account.delegated() {
            resident.revoke();
        }
        accounts.remove(&key);
        drop(holders);
        drop(accounts);
        if let Some(shutdown) = shutdown {
            self.host_notify(account, shutdown);
        }
        let stopped = self.keep_notice(account, "delegated-retention");
        self.dispose_for(account, disposal, reason, Some(stopped));
    }

    /// Explicit removal/disable also applies while this user's account is retained.
    /// AGT-79: an unjoined socket never stops an account another live holder holds.
    pub(crate) fn dispose_delegated(&self, user: &str, name: &str) {
        let accounts = self.accounts.lock().unwrap();
        let mut matches = accounts.values().filter(|account| {
            if account.user != user || account.delegated().is_none() {
                return false;
            }
            account.has_label(name)
        });
        let account = matches.next().cloned();
        let unique = matches.next().is_none();
        drop(accounts);
        let Some(account) = account.filter(|_| unique) else {
            return;
        };
        let holders = account.holders.lock().unwrap();
        if !holders.disposed && holders.by_id.values().any(Option::is_some) {
            return;
        }
        drop(holders);
        self.dispose_for(&account, Disposal::PageDisposed, "account disposed", None);
    }
}
