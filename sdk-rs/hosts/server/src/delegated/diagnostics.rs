//! Admission diagnostics contain only server-owned context and classified codes.
use super::{Handovers, probe::ProbeRefusal};
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct RefusalContext {
    attempt_id: Option<String>,
    venue: Option<String>,
}

impl Handovers {
    pub(crate) fn refusal_context(&self, account: &str, venue: &str) -> RefusalContext {
        let attempt = self.attempts.get(account);
        let venue = attempt.map_or(venue, |attempt| attempt.venue.as_str());
        RefusalContext {
            // Never take the id from a client-supplied sealed payload.
            attempt_id: attempt.map(|attempt| attempt.id.clone()),
            venue: safe_cex_venues::supported(venue).then(|| venue.to_owned()),
        }
    }
}

impl RefusalContext {
    pub(crate) fn fields(&self, refusal: ProbeRefusal) -> Value {
        json!({
            "event": "hosted.admission_refused",
            "attempt_id": self.attempt_id,
            "venue": self.venue,
            "kind": "unverified",
            "stage": refusal.stage,
            "transient": refusal.transient,
            "error_kind": refusal.error_kind.map(|kind| kind.as_str()),
            "venue_code": refusal.venue_code,
        })
    }

    pub(crate) fn refused(&self, refusal: ProbeRefusal) {
        crate::state::Log::warn(&self.fields(refusal).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delegated::probe::RefusalStage;

    #[test]
    fn refusal_diagnostic_uses_only_server_context_and_numeric_codes() {
        let mut handovers = Handovers::default();
        let offer = handovers
            .issue("synthetic-account-secret", "bybit", 1)
            .unwrap();
        let context =
            handovers.refusal_context("synthetic-account-secret", "synthetic-venue-secret");
        handovers.forget("synthetic-account-secret");
        let fields = context.fields(ProbeRefusal {
            transient: false,
            stage: RefusalStage::PrivateRead,
            error_kind: Some(safe_cex_engine::ErrorKind::AuthPermission),
            venue_code: Some(10010),
        });
        assert_eq!(
            fields,
            json!({
                "event":"hosted.admission_refused", "attempt_id":offer.attempt_id,
                "venue":"bybit", "kind":"unverified", "stage":"private_read", "transient":false,
                "error_kind":"authPermission", "venue_code":10010,
            })
        );
        assert!(!fields.to_string().contains("secret"));
        let unknown =
            handovers.refusal_context("synthetic-account-secret", "synthetic-venue-secret");
        let fields = unknown.fields(ProbeRefusal::INVALID.at(RefusalStage::HandoverOpen));
        assert!(fields["attempt_id"].is_null());
        assert!(fields["venue"].is_null());
        assert!(fields["venue_code"].is_null());
        assert!(!fields.to_string().contains("secret"));
    }
}
