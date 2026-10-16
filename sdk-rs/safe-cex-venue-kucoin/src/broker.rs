//! Broker attribution for the core's normalized string-header boundary.
use indexmap::IndexMap;
use safe_cex_crypto::{Encoding, SigningOptions, sign_payload};
use safe_cex_types::{ExchangeName, ExchangeOptions, ExchangeOptionsExtraKucoinBroker};
use serde_json::Value;

// ts: trimString
pub fn trim_string(value: Option<&Value>) -> Option<String> {
    let value = safe_cex_num::js_trim(value?.as_str()?);
    (!value.is_empty()).then(|| value.to_owned())
}
// ts: getHeader
pub fn get_header(headers: &IndexMap<String, String>, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}
// ts: getKucoinRequestTimestamp
pub fn get_kucoin_request_timestamp(headers: Option<&IndexMap<String, String>>) -> Option<String> {
    get_header(headers?, "KC-API-TIMESTAMP")
}
// ts: getKucoinBrokerConfig
pub fn get_kucoin_broker_config(
    options: &ExchangeOptions,
) -> Option<ExchangeOptionsExtraKucoinBroker> {
    let config = options.extra.as_ref()?.kucoin_broker.as_ref()?;
    Some(ExchangeOptionsExtraKucoinBroker {
        partner: trim_string(Some(&Value::String(config.partner.clone())))?,
        broker_key: trim_string(Some(&Value::String(config.broker_key.clone())))?,
        broker_name: trim_string(Some(&Value::String(config.broker_name.clone())))?,
        verify: Some(config.verify.unwrap_or(true)),
    })
}
// ts: addKucoinBrokerHeaders
pub async fn add_kucoin_broker_headers(
    headers: &mut IndexMap<String, String>,
    options: &ExchangeOptions,
    timestamp: &str,
) -> Result<(), String> {
    let Some(config) = get_kucoin_broker_config(options).filter(|_| !options.key.is_empty()) else {
        return Ok(());
    };
    let preimage = format!("{timestamp}{}{}", config.partner, options.key);
    // Local broker signing has no external await: cancellation cannot leave a partial header set.
    let signed = sign_payload(
        &SigningOptions {
            key: &options.key,
            secret: &config.broker_key,
            payload_signer: None,
        },
        ExchangeName::Kucoin,
        &preimage,
        Encoding::Base64,
    )
    .await?;
    for (name, value) in [
        ("KC-API-TIMESTAMP", timestamp.to_owned()),
        ("KC-API-PARTNER", config.partner),
        ("KC-API-PARTNER-SIGN", signed.signature),
        ("KC-BROKER-NAME", config.broker_name),
        (
            "KC-API-PARTNER-VERIFY",
            if config.verify == Some(true) {
                "true"
            } else {
                "false"
            }
            .to_owned(),
        ),
    ] {
        if let Some((_, existing)) = headers
            .iter_mut()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            *existing = value;
        } else {
            headers.insert(name.to_owned(), value);
        }
    }
    Ok(())
}
