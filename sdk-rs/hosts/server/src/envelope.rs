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

pub fn init_unverified(account_name: &str, transient: bool) -> String {
    envelope(
        "msg",
        json!({"op":"error", "accountName":account_name,
            "data":["Could not verify your key; account not attached",
                {"forceToast":true, "initRefused":true, "kind":"unverified", "transient":transient}]}),
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
