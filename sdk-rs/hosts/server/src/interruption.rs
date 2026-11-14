//! One-shot, user-owned interruption notices. No socket or box owns the pending notice.
use crate::{envelope, out::Out};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_USERS: usize = 4096;
const MAX_ACKNOWLEDGEMENTS: usize = 4096;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    OwnerReleased,
    LeaseExpired,
    WaitingForKey,
}

/// Admission recovery only: key availability never acknowledges a restored job.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DelegatedKeyStatus {
    WaitingForKey,
    KeyAvailable,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Marker {
    pub reason: Reason,
    pub from_box: String,
    pub at: u64,
}
impl Marker {
    pub fn parse(header: &str, now: u64) -> Option<Self> {
        if header.len() > 1024 {
            return None;
        }
        let marker: Self = serde_json::from_str(header).ok()?;
        if marker.from_box.len() > 128
            || marker.from_box.trim().is_empty()
            || marker.from_box.chars().any(char::is_control)
            || marker.at == 0
            || marker.at > now.saturating_add(crate::router_lease::CLOCK_SKEW_MS)
        {
            return None;
        }
        Some(marker)
    }
    fn notice(&self, account: &str, display: &str) -> String {
        if self.reason == Reason::WaitingForKey {
            return envelope::algo_notice(
                account,
                &format!(
                    "The previous server ({display}) lost its memory-only delegated key (waiting for your key). Only client-retained TWAP schedules can be restored, paused. Check Open Orders before explicitly resuming TWAPs or starting new chases; resting orders may still fill."
                ),
                None,
            );
        }
        envelope::algo_notice(
            account,
            &format!(
                "Your algos on the previous server ({}) were stopped. Check your algos and Open Orders before explicitly resuming TWAPs or starting new chases; resting orders may still fill.",
                display
            ),
            None,
        )
    }
}
struct Pending {
    marker: Marker,
    sent: bool,
    until: Instant,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RestoreEpoch {
    pub marker: Option<u64>,
    pub lease: Option<u64>,
}
impl RestoreEpoch {
    fn active(self) -> bool {
        self.marker.is_some() || self.lease.is_some()
    }
}
struct Acknowledgement {
    epoch: RestoreEpoch,
    resumed: bool,
    until: Instant,
}
#[derive(Default)]
struct User {
    marker: Option<Pending>,
    jobs: HashMap<(String, String), Acknowledgement>,
}
fn prune(entries: &mut HashMap<String, User>) {
    let now = Instant::now();
    let wall = crate::accounts::now_ms();
    entries.retain(|_, user| {
        if user.marker.as_ref().is_some_and(|pending| {
            now >= pending.until
                || wall.saturating_sub(pending.marker.at) >= RETENTION.as_millis() as u64
        }) {
            user.marker = None;
        }
        user.jobs.retain(|_, ack| now < ack.until);
        user.marker.is_some() || !user.jobs.is_empty()
    });
}
#[derive(Default)]
pub struct Notices {
    users: Mutex<HashMap<String, User>>,
    local: Option<(String, String)>,
}
pub(crate) struct RestoreGuard {
    notices: Arc<Notices>,
    user: String,
    account: String,
}
impl RestoreGuard {
    pub fn epoch(&self, lease: Option<u64>) -> RestoreEpoch {
        let mut entries = self.notices.users.lock().unwrap();
        prune(&mut entries);
        RestoreEpoch {
            marker: entries
                .get(&self.user)
                .and_then(|user| user.marker.as_ref())
                .map(|pending| pending.marker.at),
            lease,
        }
    }
    pub fn must_pause(&self, job: &str, epoch: RestoreEpoch) -> bool {
        if !epoch.active() {
            return false;
        }
        let entries = self.notices.users.lock().unwrap();
        !entries
            .get(&self.user)
            .and_then(|user| user.jobs.get(&(self.account.clone(), job.to_owned())))
            .is_some_and(|ack| ack.epoch == epoch && ack.resumed && Instant::now() < ack.until)
    }
    pub fn reserve_resume(&self, job: &str, epoch: RestoreEpoch) -> Result<(), &'static str> {
        if !epoch.active() {
            return Ok(());
        }
        let mut entries = self.notices.users.lock().unwrap();
        prune(&mut entries);
        if !entries.contains_key(&self.user) && entries.len() >= MAX_USERS {
            return Err("Interruption acknowledgement capacity reached; job remains paused");
        }
        let user = entries.entry(self.user.clone()).or_default();
        let key = (self.account.clone(), job.to_owned());
        if !user.jobs.contains_key(&key) && user.jobs.len() >= MAX_ACKNOWLEDGEMENTS {
            return Err("Interruption acknowledgement capacity reached; job remains paused");
        }
        let acknowledgement = user.jobs.entry(key).or_insert(Acknowledgement {
            epoch,
            resumed: false,
            until: Instant::now() + RETENTION,
        });
        if acknowledgement.epoch != epoch {
            *acknowledgement = Acknowledgement {
                epoch,
                resumed: false,
                until: Instant::now() + RETENTION,
            };
        }
        Ok(())
    }
    pub fn acknowledge_resume(&self, job: &str, epoch: RestoreEpoch) {
        let mut entries = self.notices.users.lock().unwrap();
        if let Some(ack) = entries
            .get_mut(&self.user)
            .and_then(|user| user.jobs.get_mut(&(self.account.clone(), job.to_owned())))
            && ack.epoch == epoch
        {
            ack.resumed = true;
            ack.until = Instant::now() + RETENTION;
        }
    }
}
impl Notices {
    pub fn with_box(router: Option<String>, display: Option<String>) -> Self {
        Self {
            users: Mutex::default(),
            local: router.zip(display),
        }
    }
    pub(crate) fn account(self: &Arc<Self>, user: &str, account: &str) -> RestoreGuard {
        RestoreGuard {
            notices: self.clone(),
            user: user.into(),
            account: account.into(),
        }
    }
    pub fn record(&self, user: &str, marker: Marker) -> Result<(), &'static str> {
        let age = crate::accounts::now_ms().saturating_sub(marker.at);
        if age >= RETENTION.as_millis() as u64 {
            return Ok(());
        }
        let mut entries = self.users.lock().unwrap();
        prune(&mut entries);
        if let Some(previous) = entries.get(user).and_then(|user| user.marker.as_ref()) {
            if previous.marker.at >= marker.at {
                return Ok(());
            }
        } else if !entries.contains_key(user) && entries.len() >= MAX_USERS {
            return Err("interruption notice inventory full");
        }
        let user = entries.entry(user.to_owned()).or_default();
        user.jobs.clear();
        user.marker = Some(Pending {
            marker,
            sent: false,
            until: Instant::now() + RETENTION - Duration::from_millis(age),
        });
        Ok(())
    }
    /// Status for the requesting connection. Reuses the recorded interruption,
    /// without consuming its notice or weakening its per-job restore fence.
    pub(crate) fn delegated_status(
        &self,
        user: &str,
        account: &str,
        out: &Out,
        status: DelegatedKeyStatus,
    ) -> bool {
        let mut entries = self.users.lock().unwrap();
        prune(&mut entries);
        if matches!(status, DelegatedKeyStatus::WaitingForKey)
            && entries
                .get(user)
                .and_then(|user| user.marker.as_ref())
                .is_none()
        {
            return false;
        }
        let message = match status {
            DelegatedKeyStatus::WaitingForKey => "waiting for your key",
            DelegatedKeyStatus::KeyAvailable => "your key is available",
        };
        out.send(envelope::msg(
            account,
            "delegated:status",
            serde_json::json!({
                "status": status,
                "reason": Reason::WaitingForKey,
                "message": message,
            }),
        ))
    }

    pub fn deliver(&self, user: &str, account: &str, out: &Out) -> bool {
        let mut entries = self.users.lock().unwrap();
        prune(&mut entries);
        let Some(pending) = entries.get_mut(user).and_then(|user| user.marker.as_mut()) else {
            return false;
        };
        if pending.sent {
            return false;
        }
        let display = self
            .local
            .as_ref()
            .filter(|(id, _)| id == &pending.marker.from_box)
            .map_or(pending.marker.from_box.as_str(), |(_, name)| name.as_str());
        if !out.send(pending.marker.notice(account, display)) {
            return false;
        }
        pending.sent = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegated_restart_reason_reuses_the_bounded_interruption_marker() {
        let raw = serde_json::json!({
            "reason": "waiting-for-key", "fromBox": "01", "at": 1000
        })
        .to_string();
        let marker = Marker::parse(&raw, 1000).expect("typed restart reason");
        assert_eq!(
            serde_json::to_value(&marker).unwrap()["reason"],
            "waiting-for-key"
        );
        assert!(Marker::parse(&raw.replace("1000", "6001"), 1000).is_none());
        let notice: serde_json::Value = serde_json::from_str(&marker.notice("acct", "01")).unwrap();
        assert!(
            notice["d"]["data"][0]
                .as_str()
                .unwrap()
                .contains("waiting for your key")
        );
    }
    #[tokio::test(start_paused = true)]
    async fn acknowledgements_are_job_account_user_and_epoch_scoped_with_bounded_marker_lifetime() {
        let notices = Arc::new(Notices::default());
        let at = crate::accounts::now_ms();
        notices
            .record(
                "u",
                Marker {
                    reason: Reason::LeaseExpired,
                    from_box: "01".into(),
                    at,
                },
            )
            .unwrap();
        let a = notices.account("u", "account-a");
        let epoch = a.epoch(Some(1));
        a.reserve_resume("job-a", epoch).unwrap();
        assert!(
            a.must_pause("job-a", epoch),
            "an attempted resume alone cannot acknowledge"
        );
        a.acknowledge_resume("job-a", epoch);
        assert!(!a.must_pause("job-a", a.epoch(Some(1))));
        assert!(a.must_pause("job-b", epoch));
        assert!(notices.account("u", "account-b").must_pause("job-a", epoch));
        assert!(
            notices
                .account("other-user", "account-a")
                .must_pause("job-a", epoch)
        );
        assert!(
            a.must_pause("job-a", a.epoch(Some(2))),
            "new lease generation rearms"
        );
        notices
            .record(
                "u",
                Marker {
                    reason: Reason::OwnerReleased,
                    from_box: "01".into(),
                    at: at + 1,
                },
            )
            .unwrap();
        assert!(a.must_pause("job-a", a.epoch(Some(1))), "new marker rearms");
        let newer = a.epoch(Some(1));
        a.reserve_resume("job-a", newer).unwrap();
        a.acknowledge_resume("job-a", epoch);
        assert!(
            a.must_pause("job-a", newer),
            "old completion cannot acknowledge a newer marker"
        );
        a.acknowledge_resume("job-a", newer);
        assert!(!a.must_pause("job-a", newer));
        tokio::time::advance(RETENTION + Duration::from_millis(1)).await;
        assert!(!a.must_pause("job-a", a.epoch(None)));
        let (out, _messages, _) = Out::channel(1);
        assert!(!notices.deliver("u", "account-a", &out));
        assert!(notices.users.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn notice_is_once_per_user_across_accounts_and_repeated_upgrades() {
        let notices = Notices::default();
        let marker = Marker {
            reason: Reason::LeaseExpired,
            from_box: "exchange-01".into(),
            at: crate::accounts::now_ms(),
        };
        let (out, mut messages, _) = Out::channel(4);
        assert!(!notices.deliver("u", "acct", &out));
        notices.record("u", marker.clone()).unwrap();
        assert!(notices.deliver("u", "acct", &out));
        let message: serde_json::Value =
            serde_json::from_str(&messages.recv().await.unwrap()).unwrap();
        assert_eq!(message["d"]["op"], "error");
        assert_eq!(message["d"]["data"][1]["forceToast"], true);
        assert!(
            message["d"]["data"][0]
                .as_str()
                .unwrap()
                .contains("previous server")
        );
        notices.record("u", marker).unwrap();
        assert!(!notices.deliver("u", "other-account", &out));
        assert!(!notices.deliver("other-user", "acct", &out));
    }
    #[tokio::test]
    async fn failed_enqueue_keeps_the_notice_for_the_next_connection() {
        let notices = Notices::default();
        notices
            .record(
                "u",
                Marker {
                    reason: Reason::OwnerReleased,
                    from_box: "exchange-02".into(),
                    at: crate::accounts::now_ms(),
                },
            )
            .unwrap();
        let (out, _, _) = Out::channel(1);
        assert!(!notices.deliver("u", "acct", &out));
        let (out, _messages, _) = Out::channel(1);
        assert!(notices.deliver("u", "acct", &out));
    }
    #[tokio::test]
    async fn opaque_router_ids_are_displayed_without_display_name_validation() {
        let notices = Notices::with_box(Some("01".into()), Some("exchange-01".into()));
        let (out, mut messages, _) = Out::channel(4);
        for (user, id, display) in [("u", "01", "exchange-01"), ("other", "02", "02")] {
            let marker = Marker::parse(
                &serde_json::json!({"reason":"owner-released","fromBox":id,"at":crate::accounts::now_ms()}).to_string(),
                crate::accounts::now_ms(),
            )
            .unwrap();
            notices.record(user, marker).unwrap();
            assert!(notices.deliver(user, "acct", &out));
            assert!(
                messages
                    .recv()
                    .await
                    .unwrap()
                    .contains(&format!("previous server ({display})"))
            );
        }
    }

    #[test]
    fn marker_schema_rejects_arbitrary_reasons_boxes_and_future_time() {
        for raw in [
            r#"{"reason":"other","fromBox":"exchange-01","at":1}"#,
            r#"{"reason":"lease-expired","fromBox":"","at":1}"#,
            r#"{"reason":"lease-expired","fromBox":"01\n","at":1}"#,
            r#"{"reason":"lease-expired","fromBox":"exchange-01","at":6001}"#,
        ] {
            assert!(Marker::parse(raw, 1000).is_none(), "{raw}");
        }
    }
}
