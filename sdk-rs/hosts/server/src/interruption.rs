//! One-shot, user-owned interruption notices. No socket or box owns the pending notice.
use crate::{envelope, out::Out};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    OwnerReleased,
    LeaseExpired,
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
            || !marker
                .from_box
                .strip_prefix("exchange-")?
                .bytes()
                .all(|c| c.is_ascii_digit())
            || marker.from_box.len() <= "exchange-".len()
            || marker.at == 0
            || marker.at > now.saturating_add(crate::router_lease::CLOCK_SKEW_MS)
        {
            return None;
        }
        Some(marker)
    }
    fn notice(&self, account: &str) -> String {
        envelope::algo_notice(
            account,
            &format!(
                "Your algos on the previous server ({}) were stopped. Check your algos and Open Orders before explicitly resuming TWAPs or starting new chases; resting orders may still fill.",
                self.from_box
            ),
            None,
        )
    }
}
struct Pending {
    marker: Marker,
    sent: bool,
}
#[derive(Default)]
pub struct Notices(Mutex<HashMap<String, Pending>>);
impl Notices {
    pub fn record(&self, user: &str, marker: Marker) -> Result<(), &'static str> {
        let mut entries = self.0.lock().unwrap();
        if let Some(previous) = entries.get(user) {
            if previous.marker == marker || previous.marker.at >= marker.at {
                return Ok(());
            }
        } else if entries.len() >= 4096 {
            return Err("interruption notice inventory full");
        }
        entries.insert(
            user.to_owned(),
            Pending {
                marker,
                sent: false,
            },
        );
        Ok(())
    }
    pub fn deliver(&self, user: &str, account: &str, out: &Out) -> bool {
        let mut entries = self.0.lock().unwrap();
        let Some(pending) = entries.get_mut(user) else {
            return false;
        };
        if pending.sent {
            return false;
        }
        if !out.send(pending.marker.notice(account)) {
            return false;
        }
        pending.sent = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn notice_is_once_per_user_across_accounts_and_repeated_upgrades() {
        let notices = Notices::default();
        let marker = Marker {
            reason: Reason::LeaseExpired,
            from_box: "exchange-01".into(),
            at: 1,
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
                    at: 1,
                },
            )
            .unwrap();
        let (out, _, _) = Out::channel(1);
        assert!(!notices.deliver("u", "acct", &out));
        let (out, _messages, _) = Out::channel(1);
        assert!(notices.deliver("u", "acct", &out));
    }
    #[test]
    fn marker_schema_rejects_arbitrary_reasons_boxes_and_future_time() {
        for raw in [
            r#"{"reason":"other","fromBox":"exchange-01","at":1}"#,
            r#"{"reason":"lease-expired","fromBox":"untrusted-host","at":1}"#,
            r#"{"reason":"lease-expired","fromBox":"exchange-","at":1}"#,
            r#"{"reason":"lease-expired","fromBox":"exchange-01","at":6001}"#,
        ] {
            assert!(Marker::parse(raw, 1000).is_none(), "{raw}");
        }
    }
}
