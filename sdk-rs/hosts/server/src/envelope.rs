//! Today's `wss /account` JSON envelope (NX `services/exchange-server/src/server.ts`).
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Account settings and an optional encrypted handover; no plaintext API secret.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitPayload {
    pub account_name: String,
    pub exchange_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing_mode: Option<String>,
    #[serde(
        default,
        deserialize_with = "coinbase_algorithm",
        skip_serializing_if = "Option::is_none"
    )]
    pub coinbase_jwt_algorithm: Option<safe_cex_remote_signer::CoinbaseJwtAlgorithm>,
    /// Consumed on this connection; never included in serialized settings.
    #[serde(default, skip_serializing)]
    pub sealed: Option<crate::delegated::Sealed>,
    /// Reply provenance only; never an engine option or account fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, deserialize_with = "key")]
    pub key: String,
    #[serde(default, deserialize_with = "text")]
    pub passphrase: Option<String>,
    pub testnet: Option<bool>,
    #[serde(default, deserialize_with = "text")]
    pub application_id: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub api_key_index: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub paradex_address: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub subkey_address: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub subaccount: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub orderly_exchange_id: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub grvt_trading_account_id: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub grvt_signer_address: Option<String>,
    pub tealstreet_proxy: Option<bool>,
    pub tealstreet_tunnel: Option<bool>,
    pub tealstreet_ws_route: Option<bool>,
    #[serde(default, deserialize_with = "text")]
    pub proxy_region: Option<String>,
    pub public_only: Option<bool>,
    pub hyperliquid_builder: Option<HyperliquidBuilder>,
    /// The Hyperliquid vault the account trades; without it orders go to the main wallet.
    #[serde(default, deserialize_with = "text")]
    pub vault: Option<String>,
    /// Bybit's regional site (`BRA_BTL` selects the Brazil host and header).
    #[serde(default, deserialize_with = "text")]
    pub bybit_site_id: Option<String>,
    pub enable_spot_markets: Option<bool>,
    pub enable_inverse_markets: Option<bool>,
    pub enable_option_markets: Option<bool>,
    /// Diagnostics only; excluded from the init fingerprint.
    #[serde(skip_serializing)]
    pub verbose: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidBuilder {
    pub address: String,
    pub fee_tenths_bps: f64,
}

/// Browser account fields are JS values: a stored index may be a number and an
/// unset field may be null, and neither may fail the whole init.
fn text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => Some(text),
        Value::Number(number) => Some(number.to_string()),
        Value::Null => None,
        other => {
            return Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Other(&other.to_string()),
                &"a string",
            ));
        }
    })
}

fn key<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(text(deserializer)?.unwrap_or_default())
}

fn coinbase_algorithm<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<safe_cex_remote_signer::CoinbaseJwtAlgorithm>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|value| {
            safe_cex_remote_signer::CoinbaseJwtAlgorithm::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
            .map_err(|_| serde::de::Error::custom("Invalid Coinbase JWT algorithm"))
        })
        .transpose()
}

impl InitPayload {
    /// Identifies what an account's engine is built from; an init with the same
    /// one shares the account (see `accounts::build_fingerprint`).
    pub fn fingerprint(&self) -> String {
        crate::accounts::build_fingerprint(self)
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountMsg {
    pub op: String,
    #[serde(default)]
    pub data: Value,
    #[serde(rename = "accountName")]
    pub account_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplySettings {
    pub account_name: String,
    /// Missing bindings deserialize so the handler can visibly refuse them.
    pub instance_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t", content = "d", rename_all = "kebab-case")]
pub enum Inbound {
    #[serde(rename = "order.submit")]
    OrderSubmit(crate::order_contract::OrderSubmit),
    #[serde(rename = "order.action")]
    OrderAction(crate::order_actions::WireRequest),
    #[serde(rename = "order.actionCancelQueued")]
    OrderActionCancelQueued(crate::order_actions::CancelQueued),
    #[serde(rename = "order.cancelQueued")]
    OrderCancelQueued(crate::order_contract::CancelQueued),
    Init(Box<InitPayload>),
    ApplySettings(ApplySettings),
    Msg(AccountMsg),
    SignRes(Value),
    Dispose {
        #[serde(rename = "accountName")]
        account_name: String,
    },
    EchoRes(Value),
    Ping(Option<PingData>),
    Perf(Value),
    Verbose(Value),
}

#[derive(Debug, Default, Deserialize)]
pub struct PingData {
    #[serde(default)]
    pub accounts: Vec<String>,
}

/// `t` first: the client routes `session`, `pong`, `echo` and `sign-req` by
/// their exact `{"t":"<type>"` prefix, and this crate's `json!` maps serialize
/// keys sorted, which put `d` ahead.
fn envelope(t: &str, d: Value) -> String {
    format!(r#"{{"t":"{t}","d":{d}}}"#)
}

pub(crate) fn account_name(text: &str) -> Option<String> {
    let frame: Value = serde_json::from_str(text).ok()?;
    frame["d"]["accountName"].as_str().map(str::to_owned)
}

pub(crate) fn with_instance_id(text: String, instance_id: &str) -> String {
    let Ok(mut frame) = serde_json::from_str::<Value>(&text) else {
        return text;
    };
    let kind = match frame["t"].as_str() {
        Some("msg") => "msg",
        Some("sign-req") => "sign-req",
        Some("order.ack") => "order.ack",
        Some("order.outcome") => "order.outcome",
        Some("order.actionAck") => "order.actionAck",
        Some("order.actionOutcome") => "order.actionOutcome",
        _ => return text,
    };
    let Some(data) = frame["d"].as_object_mut() else {
        return text;
    };
    data.insert("instanceId".into(), instance_id.into());
    envelope(kind, frame["d"].take())
}

pub(crate) fn with_account_label(text: String, label: &str) -> String {
    let Ok(mut frame) = serde_json::from_str::<Value>(&text) else {
        return text;
    };
    let kind = match frame["t"].as_str() {
        Some("msg") => "msg",
        Some("sign-req") => "sign-req",
        _ => return text,
    };
    let Some(data) = frame["d"].as_object_mut() else {
        return text;
    };
    if data.get("accountName").and_then(Value::as_str) == Some(label) {
        return text;
    }
    match data.get("op").and_then(Value::as_str) {
        Some("store") => {
            if let Some(rows) = data
                .get_mut("data")
                .and_then(|data| data.get_mut("activeTWAPs"))
                .and_then(Value::as_array_mut)
            {
                for row in rows {
                    if let Some(row) = row.as_object_mut() {
                        row.insert("accountName".into(), label.into());
                    }
                }
            }
        }
        Some("twap:stateUpdate") => {
            if let Some(rows) = data.get_mut("data").and_then(Value::as_object_mut) {
                for row in rows.values_mut() {
                    for field in ["options", "state"] {
                        if let Some(fields) = row.get_mut(field).and_then(Value::as_object_mut) {
                            fields.insert("accountName".into(), label.into());
                        }
                    }
                }
            }
        }
        _ => {}
    }
    data.insert("accountName".into(), label.into());
    envelope(kind, frame["d"].take())
}

pub fn msg(account_name: &str, op: &str, data: Value) -> String {
    envelope(
        "msg",
        json!({"op": op, "accountName": account_name, "data": data}),
    )
}

pub fn order_frame(kind: &str, account_name: &str, mut data: Value) -> String {
    data["accountName"] = account_name.into();
    envelope(kind, data)
}

pub fn delegated_key(account_name: &str, offer: &crate::delegated::KeyOffer) -> String {
    let mut data = serde_json::to_value(offer).expect("public offer serializes");
    data["op"] = "delegated-key".into();
    data["accountName"] = account_name.into();
    envelope("msg", data)
}

pub fn frame(account_name: &str, seq: u64, snapshot: bool, scxf: &[u8]) -> String {
    use base64::Engine as _;
    envelope(
        "msg",
        json!({
            "op": "frame",
            "accountName": account_name,
            "seq": seq,
            "snapshot": snapshot,
            "data": base64::engine::general_purpose::STANDARD.encode(scxf),
        }),
    )
}

pub fn error(account_name: Option<&str>, message: &str) -> String {
    envelope(
        "msg",
        json!({"op": "error", "accountName": account_name, "data": [message]}),
    )
}

/// Optional error metadata; old clients still receive the original first argument.
pub fn moving(account_name: &str, message: &str) -> String {
    envelope(
        "msg",
        json!({"op":"error", "accountName":account_name,
        "data":[message, {"reason":"moving"}]}),
    )
}

/// An algo error: shown even over a reconnect or to a hidden tab (`forceToast`), which
/// would otherwise only log it, since it can mean an order is left resting.
pub fn algo_error(account_name: &str, message: Value) -> String {
    envelope(
        "msg",
        json!({
            "op": "error",
            "accountName": account_name,
            "data": [message, {"forceToast": true}],
        }),
    )
}

/// An algo error, and for an algo's stop notice the algo it names
/// (`{"kind": "chase"|"twap", "id"}`), so the page can dedupe it exactly.
pub fn algo_notice(
    account_name: &str,
    message: &str,
    algo: Option<&crate::algos::AlgoRef>,
) -> String {
    let mut options = json!({"forceToast": true});
    if let Some(algo) = algo {
        options["algo"] = json!({"kind": algo.kind, "id": algo.id});
    }
    envelope(
        "msg",
        json!({
            "op": "error",
            "accountName": account_name,
            "data": [message, options],
        }),
    )
}

/// An init the server will not build: shown to the user at once (`forceToast`),
/// and marked `initRefused` so the client can stop waiting for that account to load.
pub fn init_refused(account_name: &str, message: &str) -> String {
    envelope(
        "msg",
        json!({
            "op": "error",
            "accountName": account_name,
            "data": [message, {"forceToast": true, "initRefused": true}],
        }),
    )
}

/// A pre-probe registry refusal says nothing about credential validity.
/// Keep the reason opaque on the socket; closed sub-reasons belong in diagnostics.
pub fn init_signing_mode_refused(account_name: &str) -> String {
    msg(
        account_name,
        "error",
        json!([
            "The requested account signing mode is unavailable; account not attached",
            {"forceToast":true, "initRefused":true, "kind":"signing-mode", "transient":false}
        ]),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountSigningMode {
    Cosigned,
    Delegated,
}

impl AccountSigningMode {
    pub fn requested(init: &InitPayload) -> Self {
        if init.signing_mode.as_deref() == Some("delegated") {
            Self::Delegated
        } else {
            Self::Cosigned
        }
    }
}

/// An attachment acknowledgement, never a requested-mode or pending-proof hint.
pub fn account_signing_mode(
    account_name: &str,
    requested: AccountSigningMode,
    actual: AccountSigningMode,
) -> String {
    msg(
        account_name,
        "account-signing-mode",
        json!({
            "version": 1,
            "requestedMode": requested,
            "actualMode": actual,
            "attached": true,
            "reason": if requested == actual { "requested" } else { "delegated-enabled-elsewhere" },
        }),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeTransitionRefusal {
    Busy,
    Unavailable,
}

impl ModeTransitionRefusal {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Busy => "mode_transition_busy",
            Self::Unavailable => "mode_transition_unavailable",
        }
    }

    pub fn transient(self) -> bool {
        self == Self::Busy
    }
}

/// Resident mode describes the account, not authority granted to this candidate.
pub fn init_mode_transition_refused(
    account_name: &str,
    refusal: ModeTransitionRefusal,
    requested: AccountSigningMode,
    resident: AccountSigningMode,
) -> String {
    let message = match refusal {
        ModeTransitionRefusal::Busy => {
            "Delegated handover is busy with existing signing work; account not attached. Existing orders and algos are unchanged."
        }
        ModeTransitionRefusal::Unavailable => {
            "Delegated handover cannot safely change this account's signer; account not attached. Existing orders and algos are unchanged."
        }
    };
    msg(
        account_name,
        "error",
        json!([
            message,
            {
                "forceToast": true,
                "initRefused": true,
                "kind": "admission",
                "reason": refusal.reason(),
                "transient": refusal.transient(),
                "requestedMode": requested,
                "residentMode": resident,
                "attached": false,
            }
        ]),
    )
}

/// Coarse v1 admission guidance; private error details never enter the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitRefusalReasonClass {
    KeyRejectedIpOrPermissions,
    KeyInvalid,
    VenueUnavailable,
    Unknown,
}

impl InitRefusalReasonClass {
    pub(crate) fn from_error(
        venue: &str,
        error_kind: Option<safe_cex_engine::ErrorKind>,
        venue_code: Option<i64>,
    ) -> Self {
        use safe_cex_engine::ErrorKind;
        match error_kind {
            Some(ErrorKind::AuthPermission) => Self::KeyRejectedIpOrPermissions,
            // HyroTrader can return 10003 for a disabled challenge with unchanged
            // valid credentials; its Bybit CLAUDE contract forbids that inference.
            Some(ErrorKind::AuthInvalid) if venue == "hyrotrader" && venue_code == Some(10003) => {
                Self::Unknown
            }
            Some(ErrorKind::AuthInvalid | ErrorKind::AuthExpired) => {
                // Venue CLAUDE contracts and native auth tables define these broad
                // key refusals; the numeric namespace belongs to the exact venue.
                let broad_key_refusal = matches!(
                    (venue, venue_code),
                    (
                        "binance"
                            | "binancepm"
                            | "binanceinverse"
                            | "binancespot"
                            | "binanceoptions",
                        Some(-2015 | -2014)
                    ) | (
                        "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo",
                        Some(10003 | 10010)
                    ) | (
                        "okx" | "okxeu" | "okxdemo" | "okxeudemo",
                        Some(50110 | 50111)
                    )
                );
                if broad_key_refusal {
                    Self::KeyRejectedIpOrPermissions
                } else {
                    Self::KeyInvalid
                }
            }
            Some(
                ErrorKind::Network
                | ErrorKind::Timeout
                | ErrorKind::RateLimited
                | ErrorKind::VenueUnavailable,
            ) => Self::VenueUnavailable,
            _ => Self::Unknown,
        }
    }
}

pub fn init_unverified(
    account_name: &str,
    transient: bool,
    reason_class: InitRefusalReasonClass,
) -> String {
    envelope(
        "msg",
        json!({"op":"error", "accountName":account_name,
            "data":["Could not verify your key; account not attached",
                {"forceToast":true, "initRefused":true, "kind":"unverified", "transient":transient, "reason_class":reason_class}]}),
    )
}

/// A supported venue without a possession proof never takes a legacy admission path.
pub fn init_proof_unavailable(account_name: &str) -> String {
    msg(
        account_name,
        "error",
        json!([
            "Key verification is not available for this venue; account not attached",
            {"forceToast": true, "initRefused": true, "kind": "proof-unavailable",
             "transient": false, "outcome": "notSent"}
        ]),
    )
}

/// Retired or unavailable venues refuse before routing, signing or engine work.
pub fn init_unsupported(account_name: &str, venue: &str) -> String {
    let error = safe_cex_venues::unsupported_venue(venue);
    msg(
        account_name,
        "error",
        json!([
            error.message,
            {
                "forceToast": true,
                "initRefused": true,
                "kind": error.kind,
                "outcome": error.outcome
            }
        ]),
    )
}

pub fn session(resume_id: &str, resumed: bool) -> String {
    envelope(
        "session",
        json!({"resumeId": resume_id, "resumed": resumed}),
    )
}

pub fn pong(accounts: &[String]) -> String {
    envelope("pong", json!({"accounts": accounts}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_reason_class_is_a_closed_wire_enum() {
        for (class, name) in [
            (
                InitRefusalReasonClass::KeyRejectedIpOrPermissions,
                "key_rejected_ip_or_permissions",
            ),
            (InitRefusalReasonClass::KeyInvalid, "key_invalid"),
            (
                InitRefusalReasonClass::VenueUnavailable,
                "venue_unavailable",
            ),
            (InitRefusalReasonClass::Unknown, "unknown"),
        ] {
            let value = serde_json::to_value(class).unwrap();
            assert_eq!(value, name);
            assert_eq!(
                serde_json::from_value::<InitRefusalReasonClass>(value).unwrap(),
                class
            );
            let frame: Value =
                serde_json::from_str(&init_unverified("acct", false, class)).unwrap();
            assert_eq!(frame["d"]["data"][1]["reason_class"], name);
        }
        assert!(
            serde_json::from_value::<InitRefusalReasonClass>(json!("raw venue detail")).is_err()
        );
    }

    #[test]
    fn refusal_reason_class_scopes_numeric_codes_to_the_venue_family() {
        use InitRefusalReasonClass::*;
        use safe_cex_engine::ErrorKind;
        for (venues, codes) in [
            (
                &[
                    "binance",
                    "binancepm",
                    "binanceinverse",
                    "binancespot",
                    "binanceoptions",
                ][..],
                &[-2015, -2014][..],
            ),
            (
                &["bybit", "bybitinverse", "bybitspot", "bybitdemo"][..],
                &[10003, 10010][..],
            ),
            (
                &["okx", "okxeu", "okxdemo", "okxeudemo"][..],
                &[50110, 50111][..],
            ),
        ] {
            for venue in venues {
                for code in codes {
                    assert_eq!(
                        InitRefusalReasonClass::from_error(
                            venue,
                            Some(ErrorKind::AuthInvalid),
                            Some(*code)
                        ),
                        KeyRejectedIpOrPermissions
                    );
                }
            }
        }
        for venue in [
            "bybit-compatible",
            "binance-other",
            "okx-other",
            "kucoin",
            "",
        ] {
            for code in [-2015, -2014, 10003, 10010, 50110, 50111] {
                assert_eq!(
                    InitRefusalReasonClass::from_error(
                        venue,
                        Some(ErrorKind::AuthInvalid),
                        Some(code)
                    ),
                    KeyInvalid
                );
            }
        }
        for (venue, code) in [("binance", 10003), ("bybit", 50111), ("okx", -2015)] {
            assert_eq!(
                InitRefusalReasonClass::from_error(venue, Some(ErrorKind::AuthInvalid), Some(code)),
                KeyInvalid
            );
        }
    }

    #[test]
    fn refusal_reason_class_reuses_other_venues_typed_permission_failures() {
        for (venue, code) in [
            ("bitget", 40018),
            ("blofin", 152406),
            ("kucoin", 400006),
            ("kucoinspot", 400006),
        ] {
            assert_eq!(
                InitRefusalReasonClass::from_error(
                    venue,
                    Some(safe_cex_engine::ErrorKind::AuthPermission),
                    Some(code)
                ),
                InitRefusalReasonClass::KeyRejectedIpOrPermissions,
            );
        }
    }

    #[test]
    fn refusal_reason_class_missing_or_unrelated_evidence_falls_back_safely() {
        use InitRefusalReasonClass::*;
        use safe_cex_engine::ErrorKind;
        for (kind, expected) in [
            (ErrorKind::AuthInvalid, KeyInvalid),
            (ErrorKind::AuthExpired, KeyInvalid),
            (ErrorKind::AuthPermission, KeyRejectedIpOrPermissions),
            (ErrorKind::Network, VenueUnavailable),
            (ErrorKind::Timeout, VenueUnavailable),
            (ErrorKind::RateLimited, VenueUnavailable),
            (ErrorKind::VenueUnavailable, VenueUnavailable),
            (ErrorKind::Rejected, Unknown),
            (ErrorKind::InvalidInput, Unknown),
            (ErrorKind::InsufficientBalance, Unknown),
            (ErrorKind::Unsupported, Unknown),
            (ErrorKind::Internal, Unknown),
        ] {
            assert_eq!(
                InitRefusalReasonClass::from_error("bybit", Some(kind), None),
                expected
            );
        }
        assert_eq!(
            InitRefusalReasonClass::from_error("bybit", None, Some(10003)),
            Unknown
        );
        assert_eq!(
            InitRefusalReasonClass::from_error("bybit", Some(ErrorKind::Internal), Some(10003)),
            Unknown
        );
        assert_eq!(
            InitRefusalReasonClass::from_error("bybit", Some(ErrorKind::Network), Some(10003)),
            VenueUnavailable
        );
    }

    #[test]
    fn refusal_reason_class_hyro_challenge_status_does_not_claim_an_invalid_key() {
        assert_eq!(
            InitRefusalReasonClass::from_error(
                "hyrotrader",
                Some(safe_cex_engine::ErrorKind::AuthPermission),
                Some(10010)
            ),
            InitRefusalReasonClass::KeyRejectedIpOrPermissions,
        );
        assert_eq!(
            InitRefusalReasonClass::from_error(
                "hyrotrader",
                Some(safe_cex_engine::ErrorKind::AuthInvalid),
                Some(10003)
            ),
            InitRefusalReasonClass::Unknown,
        );
    }

    #[test]
    fn refusal_reason_class_unknown_is_additive_and_contains_no_private_details() {
        let frame: Value = serde_json::from_str(&init_unverified(
            "acct",
            false,
            InitRefusalReasonClass::Unknown,
        ))
        .unwrap();
        assert_eq!(
            frame,
            json!({"t":"msg", "d":{
                "op":"error", "accountName":"acct",
                "data":["Could not verify your key; account not attached", {
                    "forceToast":true, "initRefused":true, "kind":"unverified",
                    "transient":false, "reason_class":"unknown"
                }]
            }})
        );
    }

    #[test]
    fn every_envelope_leads_with_its_type() {
        let session = session("r-1", true);
        assert_eq!(
            session,
            r#"{"t":"session","d":{"resumeId":"r-1","resumed":true}}"#
        );
        assert_eq!(
            pong(&["a".into()]),
            r#"{"t":"pong","d":{"accounts":["a"]}}"#
        );
        assert!(msg("a", "x", json!(1)).starts_with(r#"{"t":"msg","d":"#));
        assert!(frame("a", 1, true, b"x").starts_with(r#"{"t":"msg","d":"#));
        assert!(error(None, "e").starts_with(r#"{"t":"msg","d":"#));
        let parsed: Value = serde_json::from_str(&session).unwrap();
        assert_eq!(parsed["d"]["resumeId"], "r-1");
    }

    #[test]
    fn init_text_fields_accept_js_numbers_and_null() {
        let init: InitPayload = serde_json::from_value(json!({
            "accountName": "a",
            "exchangeName": "lighter",
            "key": null,
            "apiKeyIndex": 3,
            "applicationId": 281474976710654_u64,
            "subaccount": null,
        }))
        .unwrap();
        assert_eq!(init.key, "");
        assert_eq!(init.api_key_index.as_deref(), Some("3"));
        assert_eq!(init.application_id.as_deref(), Some("281474976710654"));
        assert_eq!(init.subaccount, None);
        let refused = serde_json::from_value::<InitPayload>(json!({
            "accountName": "a",
            "exchangeName": "lighter",
            "apiKeyIndex": {"index": 3},
        }));
        assert!(refused.is_err());
    }
}

#[cfg(test)]
mod mode_state_tests {
    use super::*;

    #[test]
    fn a_mode_confirmation_is_bound_to_the_attached_registration() {
        for requested in [AccountSigningMode::Cosigned, AccountSigningMode::Delegated] {
            let frame: Value = serde_json::from_str(&with_instance_id(
                account_signing_mode("alias", requested, AccountSigningMode::Delegated),
                "current-registration",
            ))
            .unwrap();
            assert_eq!(frame["t"], "msg");
            assert_eq!(frame["d"]["op"], "account-signing-mode");
            assert_eq!(frame["d"]["accountName"], "alias");
            assert_eq!(frame["d"]["instanceId"], "current-registration");
            assert_eq!(
                frame["d"]["data"],
                json!({"version":1,"requestedMode":requested,"actualMode":"delegated","attached":true,"reason":if requested == AccountSigningMode::Delegated {"requested"} else {"delegated-enabled-elsewhere"}})
            );
        }
    }

    #[test]
    fn handover_refusals_never_claim_an_actual_attached_mode() {
        for refusal in [
            ModeTransitionRefusal::Busy,
            ModeTransitionRefusal::Unavailable,
        ] {
            let frame: Value = serde_json::from_str(&init_mode_transition_refused(
                "candidate",
                refusal,
                AccountSigningMode::Delegated,
                AccountSigningMode::Cosigned,
            ))
            .unwrap();
            let fields = &frame["d"]["data"][1];
            assert_eq!(fields["kind"], "admission");
            assert_eq!(fields["reason"], refusal.reason());
            assert_eq!(fields["transient"], refusal == ModeTransitionRefusal::Busy);
            assert_eq!(fields["attached"], false);
            assert_eq!(fields["initRefused"], true);
            assert_eq!(fields["requestedMode"], "delegated");
            assert_eq!(fields["residentMode"], "cosigned");
            assert!(fields.get("actualMode").is_none());
        }
    }
}
