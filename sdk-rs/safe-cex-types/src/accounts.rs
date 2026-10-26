//! Canonical saved-account construction and import validation.
//! Private credential-policy data is generated from live TS metadata and defaultProxyRegion.
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ExchangeAccountCredentialField {
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "apiSecret")]
    ApiSecret,
    #[serde(rename = "passphrase")]
    Passphrase,
    #[serde(rename = "applicationId")]
    ApplicationId,
    #[serde(rename = "apiKeyIndex")]
    ApiKeyIndex,
    #[serde(rename = "vault")]
    Vault,
    #[serde(rename = "omniKeySeed")]
    OmniKeySeed,
    #[serde(rename = "paradexAddress")]
    ParadexAddress,
    #[serde(rename = "subkeyAddress")]
    SubkeyAddress,
    #[serde(rename = "subaccount")]
    Subaccount,
    #[serde(rename = "grvtSignerAddress")]
    GrvtSignerAddress,
    #[serde(rename = "grvtTradingAccountId")]
    GrvtTradingAccountId,
    #[serde(rename = "testnet")]
    Testnet,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeAccountCredentialSchema {
    pub exchange: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub required_fields: Vec<ExchangeAccountCredentialField>,
    pub optional_fields: Vec<ExchangeAccountCredentialField>,
}
/// An open object preserves unknown import fields and runtime values admitted by TS casts.
pub type ExchangeAccountCredentialInput = Value;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeAccountConfigError {
    pub name: String,
    pub message: String,
    pub missing_fields: Vec<String>,
}
impl ExchangeAccountConfigError {
    pub fn new(message: impl Into<String>, missing_fields: Vec<String>) -> Self {
        Self {
            name: "ExchangeAccountConfigError".into(),
            message: message.into(),
            missing_fields,
        }
    }
}
impl std::fmt::Display for ExchangeAccountConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}
impl std::error::Error for ExchangeAccountConfigError {}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseImportResult {
    pub is_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
static POLICIES: LazyLock<IndexMap<String, Value>> =
    LazyLock::new(|| serde_json::from_str(include_str!("account_policy.json")).unwrap());
fn flag(policy: &Value, key: &str) -> bool {
    policy.get(key).is_some_and(truthy)
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_none_or(|n| n != 0.0 && !n.is_nan()),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn trim_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(|s| s.trim_matches(whitespace))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn optional_string(value: Option<&Value>) -> Option<String> {
    trim_string(value)
}
fn is_known_exchange_name(exchange: &str) -> bool {
    // ts-bug: isKnownExchangeName — `in` admits Object.prototype constructor/__proto__.
    POLICIES.contains_key(exchange)
}
fn unique(values: Vec<ExchangeAccountCredentialField>) -> Vec<ExchangeAccountCredentialField> {
    let mut result = Vec::new();
    for v in values {
        if !result.contains(&v) {
            result.push(v);
        }
    }
    result
}
fn strip_undefined(
    values: impl IntoIterator<Item = (String, Option<Value>)>,
) -> Map<String, Value> {
    values
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}
fn error(name: &str, message: impl Into<String>) -> ExchangeAccountConfigError {
    ExchangeAccountConfigError {
        name: name.into(),
        message: message.into(),
        missing_fields: vec![],
    }
}
fn js_string(value: &Value) -> Result<String, ExchangeAccountConfigError> {
    crate::js_coercion::js_string(Some(value)).map_err(|e| error(e.name, e.message))
}
fn get<'a>(input: &'a Value, key: &str) -> Option<&'a Value> {
    input.as_object().and_then(|v| v.get(key))
}
fn policy(exchange: &str) -> Result<&'static Value, ExchangeAccountConfigError> {
    POLICIES
        .get(exchange)
        .ok_or_else(|| error("Error", format!("Unknown exchange metadata: {exchange}")))
}
fn normalize_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => serde_json::Number::from_f64(n.as_f64().unwrap_or(f64::NAN))
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize_numbers).collect()),
        Value::Object(o) => {
            let mut o = o;
            crate::js_coercion::normalize_object_order(&mut o);
            let entries = o.into_iter();
            Value::Object(entries.map(|(k, v)| (k, normalize_numbers(v))).collect())
        }
        v => v,
    }
}

pub fn get_exchange_account_credential_schema(
    exchange: &str,
) -> Option<ExchangeAccountCredentialSchema> {
    use ExchangeAccountCredentialField as F;
    let exchange = exchange.to_lowercase();
    if !is_known_exchange_name(&exchange) {
        return None;
    }
    let p = policy(&exchange).unwrap();
    let mut required = vec![F::ApiKey];
    let mut optional = vec![];
    if !flag(p, "requiresNoSecret") {
        required.push(F::ApiSecret);
    }
    for (key, field) in [
        ("requiresPassphrase", F::Passphrase),
        ("requiresApplicationId", F::ApplicationId),
        ("requiresApiKeyIndex", F::ApiKeyIndex),
        ("requiresOmniKeySeed", F::OmniKeySeed),
    ] {
        if flag(p, key) {
            required.push(field);
        }
    }
    if flag(p, "requiresParadexAddress") {
        required.extend([F::ParadexAddress, F::SubkeyAddress]);
    }
    if exchange == "grvt" {
        required.extend([F::GrvtSignerAddress, F::GrvtTradingAccountId]);
    }
    for (key, field) in [
        ("requiresPassphrase", F::Passphrase),
        ("requiresApplicationId", F::ApplicationId),
        ("requiresApiKeyIndex", F::ApiKeyIndex),
    ] {
        if !flag(p, key) {
            optional.push(field);
        }
    }
    for (key, field) in [
        ("supportsVault", F::Vault),
        ("supportsSubaccount", F::Subaccount),
        ("supportsTestnet", F::Testnet),
    ] {
        if flag(p, key) {
            optional.push(field);
        }
    }
    Some(ExchangeAccountCredentialSchema {
        exchange,
        display_name: p
            .get("prettyName")
            .and_then(Value::as_str)
            .map(str::to_owned),
        required_fields: unique(required),
        optional_fields: unique(optional),
    })
}
pub fn get_supported_exchange_account_credential_schemas() -> Vec<ExchangeAccountCredentialSchema> {
    POLICIES
        .keys()
        .filter(|s| !matches!(s.as_str(), "constructor" | "__proto__"))
        .filter_map(|s| get_exchange_account_credential_schema(s))
        .filter(|schema| {
            let p = policy(&schema.exchange).unwrap();
            flag(p, "enabled") && !flag(p, "hiddenFromNewAccountPicker")
        })
        .collect()
}
pub fn get_supported_exchange_account_names() -> Vec<String> {
    get_supported_exchange_account_credential_schemas()
        .into_iter()
        .map(|s| s.exchange)
        .collect()
}
pub fn normalize_exchange_account_config(
    account: &Value,
) -> Result<Value, ExchangeAccountConfigError> {
    let exchange = get(account, "exchange")
        .map(js_string)
        .transpose()?
        .unwrap_or_else(|| "undefined".into());
    let p = policy(&exchange)?;
    // Requires-CORS precedes testnet; persist-time rules deliberately omit enforceWsTunnel.
    let proxy = if flag(p, "requiresCors") {
        true
    } else if get(account, "testnet").is_some_and(truthy) {
        false
    } else {
        get(account, "tealstreetProxy").is_some_and(truthy)
    };
    let mut result = account.as_object().cloned().unwrap_or_default();
    let region = result
        .shift_remove("proxyRegion")
        .filter(truthy)
        .or_else(|| proxy.then(|| p["defaultProxyRegion"].clone()));
    result.insert("tealstreetProxy".into(), Value::Bool(proxy));
    if proxy && let Some(region) = region {
        result.insert("proxyRegion".into(), region);
    }
    Ok(normalize_numbers(Value::Object(result)))
}
pub fn build_exchange_account_config(
    input: &ExchangeAccountCredentialInput,
) -> Result<Value, ExchangeAccountConfigError> {
    if input.is_null() {
        return Err(error(
            "TypeError",
            "Cannot destructure property 'privateKey' of 'input' as it is null.",
        ));
    }
    let exchange = trim_string(get(input, "exchange")).map(|s| s.to_lowercase());
    let Some(exchange) = exchange.filter(|s| is_known_exchange_name(s)) else {
        let raw = get(input, "exchange")
            .filter(|v| truthy(v))
            .map(js_string)
            .transpose()?
            .unwrap_or_default();
        return Err(ExchangeAccountConfigError::new(
            format!("Unknown exchange: {raw}"),
            vec!["exchange".into()],
        ));
    };
    let name = trim_string(get(input, "name")).ok_or_else(|| {
        ExchangeAccountConfigError::new("Missing required field: name", vec!["name".into()])
    })?;
    let p = policy(&exchange)?;
    let api_key = optional_string(get(input, "apiKey"))
        .or_else(|| {
            optional_string(get(
                input,
                if flag(p, "requiresParadexAddress") {
                    "privateKey"
                } else {
                    "walletAddress"
                },
            ))
        })
        .unwrap_or_default();
    let secret = optional_string(get(input, "apiSecret"))
        .or_else(|| optional_string(get(input, "secret")))
        .or_else(|| {
            (!flag(p, "requiresNoSecret"))
                .then(|| optional_string(get(input, "privateKey")))
                .flatten()
        })
        .unwrap_or_default();
    let mut result = input.as_object().cloned().unwrap_or_default();
    for field in ["privateKey", "walletAddress", "secret"] {
        result.shift_remove(field);
    }
    result.insert("exchange".into(), json!(exchange));
    result.insert("name".into(), json!(name));
    result.insert("apiKey".into(), json!(api_key));
    result.insert(
        "apiSecret".into(),
        json!(if flag(p, "requiresNoSecret") && secret.is_empty() {
            api_key
        } else {
            secret
        }),
    );
    for field in [
        "passphrase",
        "secondary_apiKey",
        "secondary_apiSecret",
        "secondary_passphrase",
        "applicationId",
        "apiKeyIndex",
        "vault",
        "omniKeySeed",
        "subaccount",
        "orderlyExchangeId",
        "paradexAddress",
        "subkeyAddress",
        "grvtSignerAddress",
        "grvtTradingAccountId",
        "user",
        "color",
        "lastSymbol",
        "telegramBotToken",
        "proxyRegion",
    ] {
        let value = optional_string(get(input, field))
            .or_else(|| {
                (field == "paradexAddress")
                    .then(|| optional_string(get(input, "walletAddress")))
                    .flatten()
            })
            .map(Value::String);
        match value {
            Some(value) => {
                result.insert(field.to_owned(), value);
            }
            None => {
                result.shift_remove(field);
            }
        }
    }
    result = strip_undefined(result.into_iter().map(|(k, v)| (k, Some(v))));
    if matches!(exchange.as_str(), "bybit" | "bybitinverse" | "bybitspot")
        && get(input, "bybitSiteId").and_then(Value::as_str) == Some("BRA_BTL")
    {
        result.insert("bybitSiteId".into(), json!("BRA_BTL"));
    } else {
        result.shift_remove("bybitSiteId");
    }
    let schema = get_exchange_account_credential_schema(&exchange).unwrap();
    let mut required = vec!["name".to_owned()];
    required.extend(schema.required_fields.iter().map(|f| {
        serde_json::to_value(f)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }));
    let missing: Vec<String> = required
        .into_iter()
        .filter(|field| match result.get(field) {
            Some(Value::String(s)) => s.trim_matches(whitespace).is_empty(),
            Some(v) => !truthy(v),
            None => true,
        })
        .collect();
    if !missing.is_empty() {
        return Err(ExchangeAccountConfigError::new(
            format!(
                "Missing required fields for {}: {}",
                schema.display_name.as_deref().unwrap_or("undefined"),
                missing.join(", ")
            ),
            missing,
        ));
    }
    normalize_exchange_account_config(&Value::Object(result))
}
fn apply_api_secret_backward_compat(account: &mut Value) {
    if let Some(obj) = account.as_object_mut()
        && !obj.get("apiSecret").is_some_and(truthy)
        && let Some(secret) = obj.get("secret").filter(|v| truthy(v)).cloned()
    {
        obj.insert("apiSecret".into(), secret);
    }
}
fn validate_account(account: &Value, index: usize) -> Result<Value, String> {
    if !account.is_object() && !account.is_array() {
        return Err(format!("Account #{index} is not a valid object"));
    }
    let missing: Vec<_> = ["name", "exchange"]
        .into_iter()
        .filter(|f| !get(account, f).is_some_and(truthy))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "Account #{index} missing fields: {}",
            missing.join(", ")
        ));
    }
    build_exchange_account_config(account)
        .map_err(|error| format!("Account #{index} {}", error.message))
}
pub fn parse_import_account_data(raw: &str) -> ParseImportResult {
    let invalid = |error: String| ParseImportResult {
        is_valid: false,
        data: None,
        error: Some(error),
    };
    let Ok(mut parsed) = serde_json::from_str::<Value>(raw) else {
        return invalid("Invalid JSON format".into());
    };
    if parsed.is_object() || parsed.is_null() {
        apply_api_secret_backward_compat(&mut parsed);
        return match validate_account(&parsed, 1) {
            Ok(data) => ParseImportResult {
                is_valid: true,
                data: Some(data),
                error: None,
            },
            Err(e) => invalid(e),
        };
    }
    if let Some(accounts) = parsed.as_array_mut() {
        if accounts.is_empty() {
            return invalid("Empty array - no accounts to import".into());
        }
        for (index, account) in accounts.iter_mut().enumerate() {
            apply_api_secret_backward_compat(account);
            match validate_account(account, index + 1) {
                Ok(value) => *account = value,
                Err(e) => return invalid(e),
            }
        }
        return ParseImportResult {
            is_valid: true,
            data: Some(parsed),
            error: None,
        };
    }
    invalid("Data must be an account object or array of accounts".into())
}
pub fn is_valid_exchange_account(account: &Value) -> bool {
    (account.is_object() || account.is_array()) && build_exchange_account_config(account).is_ok()
}

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    use super::ExchangeAccountCredentialField;
    #[cfg(test)]
    use super::ExchangeAccountCredentialSchema;
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeAccountCredentialField")]
    enum WireExchangeAccountCredentialField {
        #[serde(rename = "apiKey")]
        ApiKey,
        #[serde(rename = "apiSecret")]
        ApiSecret,
        #[serde(rename = "passphrase")]
        Passphrase,
        #[serde(rename = "applicationId")]
        ApplicationId,
        #[serde(rename = "apiKeyIndex")]
        ApiKeyIndex,
        #[serde(rename = "vault")]
        Vault,
        #[serde(rename = "omniKeySeed")]
        OmniKeySeed,
        #[serde(rename = "paradexAddress")]
        ParadexAddress,
        #[serde(rename = "subkeyAddress")]
        SubkeyAddress,
        #[serde(rename = "subaccount")]
        Subaccount,
        #[serde(rename = "grvtSignerAddress")]
        GrvtSignerAddress,
        #[serde(rename = "grvtTradingAccountId")]
        GrvtTradingAccountId,
        #[serde(rename = "testnet")]
        Testnet,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeAccountCredentialField {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeAccountCredentialField::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeAccountCredentialSchema")]
    #[serde(rename_all = "camelCase")]
    struct WireExchangeAccountCredentialSchema {
        exchange: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        required_fields: Vec<ExchangeAccountCredentialField>,
        optional_fields: Vec<ExchangeAccountCredentialField>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeAccountCredentialSchema {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeAccountCredentialSchema::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;
        #[test]
        fn every_shared_codec_has_a_narrow_input_shape() {
            crate::strict_decode::tests::string_union::<ExchangeAccountCredentialField>("ExchangeAccountCredentialField", WireExchangeAccountCredentialField::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeAccountCredentialField::ApiKey).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::ApiSecret).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::Passphrase).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::ApplicationId).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::ApiKeyIndex).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::Vault).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::OmniKeySeed).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::ParadexAddress).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::SubkeyAddress).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::Subaccount).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::GrvtSignerAddress).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::GrvtTradingAccountId).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountCredentialField::Testnet).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeAccountCredentialSchema>(
                "ExchangeAccountCredentialSchema",
            );
        }
    }
}
