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
}
#[derive(Default)]
pub struct Notices {
    pending: Mutex<HashMap<String, Pending>>,
    local: Option<(String, String)>,
}
impl Notices {
    pub fn with_box(router: Option<String>, display: Option<String>) -> Self {
        Self {
            pending: Mutex::default(),
            local: router.zip(display),
        }
    }
    pub fn record(&self, user: &str, marker: Marker) -> Result<(), &'static str> {
        let mut entries = self.pending.lock().unwrap();
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
    pub fn has_interruption(&self, user: &str) -> bool {
        self.pending.lock().unwrap().contains_key(user)
    }
    pub fn deliver(&self, user: &str, account: &str, out: &Out) -> bool {
        let mut entries = self.pending.lock().unwrap();
        let Some(pending) = entries.get_mut(user) else {
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
    #[tokio::test]
    async fn opaque_router_ids_are_displayed_without_display_name_validation() {
        let notices = Notices::with_box(Some("01".into()), Some("exchange-01".into()));
        let (out, mut messages, _) = Out::channel(4);
        for (user, id, display) in [("u", "01", "exchange-01"), ("other", "02", "02")] {
            let marker = Marker::parse(
                &serde_json::json!({"reason":"owner-released","fromBox":id,"at":1}).to_string(),
                1,
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
