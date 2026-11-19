//! Canonical engine settings and closed conflict diagnostics.
use super::*;

pub(super) const VERIFIED_SETTINGS_DIFFER: &str = "Account settings differ; account not attached";

// Use the owning projection policy; do not duplicate venue defaults here.
pub(super) fn canonicalize_tuning(fields: &mut Map<String, Value>, init: &InitPayload) {
    let Some(Value::Object(tuning)) = fields.get_mut("tuning") else {
        return;
    };
    let configured = init
        .tuning
        .as_ref()
        .is_some_and(|value| value.calculate_upnl_from_realtime_last_price);
    let default =
        safe_cex_store_frames::Inputs::new("", &init.exchange_name, false).calculate_upnl_from_last;
    let effective = safe_cex_store_frames::Inputs::new("", &init.exchange_name, configured)
        .calculate_upnl_from_last;
    if default == effective {
        tuning.remove("calculateUpnlFromRealtimeLastPrice");
    }
    if tuning.is_empty() {
        fields.remove("tuning");
    }
}

const FIELDS: &[&str] = &[
    "applicationId",
    "apiKeyIndex",
    "asterdex",
    "brokerAttribution",
    "bybitSiteId",
    "coinbaseJwtAlgorithm",
    "enableInverseMarkets",
    "enableOptionMarkets",
    "enableSpotMarkets",
    "endpoints",
    "grvtSignerAddress",
    "grvtTradingAccountId",
    "hyperliquidBuilder",
    "hyperliquidSignerAddress",
    "key",
    "lighterWsRoute",
    "orderlyExchangeId",
    "paradexAddress",
    "publicDataOnly",
    "routing",
    "subaccount",
    "subkeyAddress",
    "testnet",
    "tuning",
    "vault",
    "venue",
    "other",
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct SettingsConflict(u32);
impl Default for SettingsConflict {
    fn default() -> Self {
        Self(1 << (FIELDS.len() - 1))
    }
}
impl SettingsConflict {
    pub(super) fn between(old: &str, new: &str) -> Self {
        let mut mask = 0;
        for field in differing_fields(old, new) {
            if matches!(field.as_str(), "hostedSigningMode" | "passphrase") {
                continue;
            }
            let index = FIELDS
                .iter()
                .position(|name| *name == field)
                .unwrap_or(FIELDS.len() - 1);
            mask |= 1 << index;
        }
        if mask == 0 {
            Self::default()
        } else {
            Self(mask)
        }
    }
    pub(crate) fn names(self) -> Vec<&'static str> {
        FIELDS
            .iter()
            .enumerate()
            .filter_map(|(index, name)| (self.0 & (1 << index) != 0).then_some(*name))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_conflict_names_never_copy_values_or_unknown_keys() {
        let old = r#"{"routing":{"secret":"VALUE_CANARY"},"tuning":false,"UNKNOWN_KEY_CANARY":"VALUE_CANARY","passphrase":"hidden"}"#;
        let new = r#"{"routing":{},"tuning":true,"hostedSigningMode":"delegated"}"#;
        let names = SettingsConflict::between(old, new).names();
        assert_eq!(names, ["routing", "tuning", "other"]);
        assert!(!serde_json::to_string(&names).unwrap().contains("CANARY"));
    }
}
