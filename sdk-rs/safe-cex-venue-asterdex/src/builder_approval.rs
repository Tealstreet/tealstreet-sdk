//! Wallet approval workflow. Hosts provide EIP-1193 requests and fetch; all
//! validation, nonce/signature, request construction and result decisions live here.
use crate::{
    api_helpers as api,
    builder::{
        self, AsterBuilderApprovalCheckResult, AsterBuilderApprovalResult,
        AsterBuilderApprovalStatus, AsterBuilderFeeConfig,
    },
};
use safe_cex_num::{js_string, js_truthy};
use safe_cex_transport::TransportRegistry;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc};

#[derive(Debug, Clone)]
pub struct ApprovalFailure {
    pub value: Value,
    pub error_message: Option<String>,
}
impl ApprovalFailure {
    pub fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            value: json!({"message":message}),
            error_message: Some(message),
        }
    }
    fn message(&self, fallback: &str) -> String {
        builder::get_provider_error_message(
            Some(&self.value),
            self.error_message.as_deref().filter(|s| !s.is_empty()),
            fallback,
        )
    }
    fn error_only(&self, fallback: &str) -> String {
        self.error_message
            .clone()
            .unwrap_or_else(|| fallback.into())
    }
}
pub type ApprovalFuture<T> = Pin<Box<dyn Future<Output = Result<T, ApprovalFailure>>>>;
pub trait BrowserWallet {
    fn request(&self, request: Value) -> ApprovalFuture<Value>;
}
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}
pub struct ApprovalResponse {
    pub status: u16,
    pub ok: bool,
    pub json: Result<Value, ApprovalFailure>,
}
pub trait ApprovalHost {
    fn wallet(&self) -> Option<Rc<dyn BrowserWallet>>;
    fn current_time_ms(&self) -> f64;
    fn registry(&self) -> &TransportRegistry;
    fn fetch(&self, request: ApprovalRequest) -> ApprovalFuture<ApprovalResponse>;
}
fn result(success: bool, message: impl Into<String>) -> AsterBuilderApprovalResult {
    AsterBuilderApprovalResult {
        success,
        message: message.into(),
    }
}
fn no_wallet() -> AsterBuilderApprovalResult {
    result(
        false,
        "No browser wallet detected. Please install MetaMask or another wallet.",
    )
}

#[cfg(test)]
mod accepted_no_wallet {
    #[test]
    fn no_wallet_returns_the_source_failure_record() {
        let result = super::no_wallet();
        assert!(!result.success);
        assert_eq!(
            result.message,
            "No browser wallet detected. Please install MetaMask or another wallet."
        );
    }
}

async fn wallet_request(host: &dyn ApprovalHost, request: Value) -> Result<Value, ApprovalFailure> {
    let wallet = host.wallet().ok_or_else(|| {
        ApprovalFailure::error("Cannot read properties of undefined (reading 'request')")
    })?;
    wallet.request(request).await
}
// ts: switchToAsterSignatureChain
pub async fn switch_to_aster_signature_chain(
    host: &dyn ApprovalHost,
) -> AsterBuilderApprovalResult {
    let Some(wallet) = host.wallet() else {
        return no_wallet();
    };
    match wallet.request(json!({"method":"wallet_switchEthereumChain","params":[{"chainId":"0x38"}]})).await {
        Ok(_) => result(true, "Wallet network ready"),
        Err(error) => match builder::get_provider_error_code(Some(&error.value)) {
            Some(4902.0) => match wallet_request(host, json!({"method":"wallet_addEthereumChain","params":[{"chainId":"0x38","chainName":"BNB Smart Chain","nativeCurrency":{"name":"BNB","symbol":"BNB","decimals":18},"rpcUrls":["https://bsc-dataseed.binance.org"],"blockExplorerUrls":["https://bscscan.com"]}]})).await {
                Ok(_) => result(true, "Wallet network ready"),
                Err(error) => result(false, error.message("Failed to add BNB Smart Chain to wallet")),
            },
            Some(4001.0) => result(false, "Aster builder approval was rejected by user"),
            Some(-32601.0) | Some(4200.0) => result(true, "Wallet network switch unsupported; continuing to signature request"),
            _ => result(false, error.message("Failed to switch wallet to BNB Smart Chain")),
        }
    }
}
// ts: normalizeAddress (complete venue wrapper around the shared checksum primitive)
pub fn normalize_address(value: &str, field: &str) -> Result<String, ApprovalFailure> {
    if !crate::helpers::is_aster_dex_address(Some(value)) {
        return Err(ApprovalFailure::error(format!(
            "{field} must be an EVM address"
        )));
    }
    let address = safe_cex_crypto::ethereum::get_address(value)
        .map_err(|e| ApprovalFailure::error(e.to_string()))?;
    if address == "0x0000000000000000000000000000000000000000" {
        return Err(ApprovalFailure::error(format!(
            "{field} cannot be the zero address"
        )));
    }
    Ok(address)
}
#[derive(Debug, Clone)]
pub struct CheckParams {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub builder_config: Option<AsterBuilderFeeConfig>,
    pub builder_address: Option<String>,
    pub required_fee_rate: Option<String>,
    pub signer_address: String,
    pub signer_private_key: String,
    pub testnet: Option<bool>,
    pub user_address: String,
}

#[derive(serde::Deserialize)]
#[serde(remote = "CheckParams", rename_all = "camelCase")]
struct CheckParamsWire {
    api_key: Option<String>,
    base_url: Option<String>,
    builder_config: Option<AsterBuilderFeeConfig>,
    builder_address: Option<String>,
    required_fee_rate: Option<String>,
    signer_address: String,
    signer_private_key: String,
    testnet: Option<bool>,
    user_address: String,
}
impl<'de> serde::Deserialize<'de> for CheckParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        CheckParamsWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}
fn check_result(
    address: String,
    status: AsterBuilderApprovalStatus,
    error: Option<String>,
    rate: Option<String>,
) -> AsterBuilderApprovalCheckResult {
    AsterBuilderApprovalCheckResult {
        approved: status == AsterBuilderApprovalStatus::Approved,
        approval_status: status,
        builder_address: address,
        error,
        max_fee_rate: rate,
    }
}
// ts: checkAsterBuilderApproved
pub async fn check_aster_builder_approved(
    host: &dyn ApprovalHost,
    params: &CheckParams,
) -> Result<AsterBuilderApprovalCheckResult, ApprovalFailure> {
    use AsterBuilderApprovalStatus as Status;
    let raw = params
        .builder_address
        .as_deref()
        .or_else(|| {
            params
                .builder_config
                .as_ref()
                .map(|c| c.builder_address.as_str())
        })
        .unwrap_or("");
    let inputs = (|| {
        Ok::<_, ApprovalFailure>((
            normalize_address(raw, "Builder address")?,
            normalize_address(&params.user_address, "Aster user address")?,
            normalize_address(&params.signer_address, "Aster signer address")?,
        ))
    })();
    let (builder_address, user, signer) = match inputs {
        Ok(inputs) => inputs,
        Err(error) => {
            return Ok(check_result(
                raw.into(),
                Status::Unverified,
                Some(error.error_only("Invalid Aster builder approval inputs")),
                None,
            ));
        }
    };
    if !crate::helpers::is_aster_dex_v3_private_key(Some(&params.signer_private_key)) {
        return Ok(check_result(
            builder_address,
            Status::Unverified,
            Some("Aster builder approval check requires a valid agent private key".into()),
            None,
        ));
    }
    // Source intentionally signs outside its fetch catch. A syntactically valid
    // zero private key or invalid clock therefore rejects the caller's future.
    let nonce = api::build_aster_dex_v3_nonce(
        &format!("{user}:{signer}:builder-check"),
        host.current_time_ms(),
    )
    .map_err(|e| ApprovalFailure::error(e.message))?;
    // These four generated strings have fixed source insertion order. Do not
    // project retained query tokens through serde_json::Value.
    let query = format!(
        "asterChain={}&user={user}&signer={signer}&nonce={nonce}",
        api::get_aster_dex_aster_chain(params.testnet.unwrap_or(false))
    );

    let signature = api::generate_aster_dex_code_message_signature(
        &query,
        &params.signer_private_key,
        params.testnet,
    )
    .map_err(|e| ApprovalFailure::error(e.to_string()))?;
    let target = format!(
        "{}/fapi/v3/builder?{query}&signature={signature}",
        builder::get_base_url(params.testnet.unwrap_or(false), params.base_url.as_deref())
    );
    let work = async {
        let (proxy_headers, url) = builder::build_aster_approval_request(
            &target,
            params.base_url.as_deref(),
            host.registry(),
        )
        .await
        .map_err(|e| ApprovalFailure::error(e.to_string()))?;
        let mut headers = BTreeMap::from([
            (
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            (
                "X-MBX-APIKEY".into(),
                params
                    .api_key
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or(signer),
            ),
        ]);
        headers.extend(proxy_headers);
        let response = host
            .fetch(ApprovalRequest {
                url,
                method: "GET".into(),
                headers,
                body: None,
            })
            .await?;
        if !response.ok {
            return Ok(check_result(
                builder_address.clone(),
                Status::Unverified,
                Some(format!(
                    "Aster builder approval check failed with HTTP {}",
                    response.status
                )),
                None,
            ));
        }
        let payload = response.json?;
        let rows = builder::extract_builder_rows(Some(&payload));
        let mut matched = None;
        for item in &rows {
            if item.is_null() {
                return Err(ApprovalFailure::error(
                    "Cannot read properties of null (reading 'builderAddress')",
                ));
            }
            let address = item
                .get("builderAddress")
                .filter(|v| !v.is_null())
                .or_else(|| item.get("builder"));
            if address
                .and_then(Value::as_str)
                .and_then(|s| safe_cex_crypto::ethereum::get_address(s).ok())
                .is_some_and(|s| s == builder_address)
            {
                matched = Some(item);
                break;
            }
        }
        let row = matched;
        let Some(row) = row else {
            return Ok(check_result(
                builder_address.clone(),
                Status::Missing,
                None,
                None,
            ));
        };
        let approved = builder::parse_fee_rate(row.get("maxFeeRate"))
            .map_err(|e| ApprovalFailure::error(e.message))?;
        let required = params.required_fee_rate.clone().unwrap_or_else(|| {
            params
                .builder_config
                .as_ref()
                .map(|c| builder::get_aster_builder_approval_fee_rate(c).to_owned())
                .unwrap_or_else(|| "0".into())
        });
        let required = builder::parse_fee_rate(Some(&Value::String(required)))
            .map_err(|e| ApprovalFailure::error(e.message))?;
        let sufficient = approved
            .zip(required)
            .is_some_and(|(a, r)| a + f64::EPSILON >= r);
        let rate = row
            .get("maxFeeRate")
            .map(|v| js_string(Some(v)))
            .transpose()
            .map_err(|e| ApprovalFailure::error(e.message))?;
        Ok::<_, ApprovalFailure>(check_result(
            builder_address.clone(),
            if sufficient {
                Status::Approved
            } else {
                Status::InsufficientFeeCap
            },
            None,
            rate,
        ))
    }
    .await;
    Ok(work.unwrap_or_else(|e| {
        check_result(
            builder_address,
            Status::Unverified,
            Some(e.error_only("Failed to check Aster builder approval")),
            None,
        )
    }))
}
#[derive(Debug, Clone)]
pub struct ApproveParams {
    pub base_url: Option<String>,
    pub builder_address: String,
    pub builder_name: Option<String>,
    pub max_fee_rate: String,
    pub testnet: Option<bool>,
    pub user_address: String,
}

#[derive(serde::Deserialize)]
#[serde(remote = "ApproveParams", rename_all = "camelCase")]
struct ApproveParamsWire {
    base_url: Option<String>,
    builder_address: String,
    builder_name: Option<String>,
    max_fee_rate: String,
    testnet: Option<bool>,
    user_address: String,
}
impl<'de> serde::Deserialize<'de> for ApproveParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ApproveParamsWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}
fn form_component(s: &str) -> String {
    let mut output = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                output.push(byte as char)
            }
            b' ' => output.push('+'),
            _ => {
                use std::fmt::Write;
                let _ = write!(output, "%{byte:02X}");
            }
        }
    }
    output
}
// ts: approveAsterBuilderFeeWithBrowserWallet
pub async fn approve_aster_builder_fee_with_browser_wallet(
    host: &dyn ApprovalHost,
    params: &ApproveParams,
) -> Result<AsterBuilderApprovalResult, ApprovalFailure> {
    if host.wallet().is_none() {
        return Ok(no_wallet());
    }
    let inputs = (|| {
        Ok::<_, ApprovalFailure>((
            normalize_address(&params.builder_address, "Builder address")?,
            normalize_address(&params.user_address, "Aster user address")?,
        ))
    })();
    let (builder_address, user) = match inputs {
        Ok(v) => v,
        Err(e) => {
            return Ok(result(
                false,
                e.error_only("Invalid Aster builder approval inputs"),
            ));
        }
    };
    let nonce =
        api::build_aster_dex_v3_nonce(&format!("{user}:approve-builder"), host.current_time_ms())
            .map_err(|e| ApprovalFailure::error(e.message))?;
    let data = builder::build_aster_approve_builder_typed_data(
        &builder_address,
        params.builder_name.as_deref(),
        &params.max_fee_rate,
        &nonce,
        params.testnet.unwrap_or(false),
        &user,
    );
    let work = async {
        let network = switch_to_aster_signature_chain(host).await;
        if !network.success {
            return Ok(network);
        }
        let accounts = wallet_request(host, json!({"method":"eth_requestAccounts"})).await?;
        let first = accounts
            .get(0)
            .filter(|v| !v.is_null())
            .unwrap_or(&Value::String(String::new()))
            .clone();
        let signer = js_string(Some(&first)).map_err(|e| ApprovalFailure::error(e.message))?;
        if signer.is_empty() {
            return Ok(result(false, "No wallet account returned by provider"));
        }
        if safe_cex_crypto::ethereum::get_address(&signer)
            .map_err(|e| ApprovalFailure::error(e.to_string()))?
            != user
        {
            return Ok(result(
                false,
                format!("Wrong wallet connected. Expected {user}, got {signer}"),
            ));
        }
        let data_json = safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(data))
            .map_err(|e| ApprovalFailure::error(e.message))?
            .expect("typed data defined");
        let signature = wallet_request(
            host,
            json!({"method":"eth_signTypedData_v4","params":[signer,data_json]}),
        )
        .await?;
        let Some(signature) = signature.as_str().filter(|s| s.starts_with("0x")) else {
            return Ok(result(
                false,
                "Wallet returned an invalid signature payload",
            ));
        };
        let body = [
            ("builder", builder_address.as_str()),
            ("maxFeeRate", params.max_fee_rate.as_str()),
            ("builderName", params.builder_name.as_deref().unwrap_or("")),
            (
                "asterChain",
                api::get_aster_dex_aster_chain(params.testnet.unwrap_or(false)),
            ),
            ("user", user.as_str()),
            ("nonce", nonce.as_str()),
            ("signature", signature),
            ("signatureChainId", "56"),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={}", form_component(v)))
        .collect::<Vec<_>>()
        .join("&");
        let target = format!(
            "{}/fapi/v3/approveBuilder",
            builder::get_base_url(params.testnet.unwrap_or(false), params.base_url.as_deref())
        );
        let (extra, url) = builder::build_aster_approval_request(
            &target,
            params.base_url.as_deref(),
            host.registry(),
        )
        .await
        .map_err(|e| ApprovalFailure::error(e.to_string()))?;
        let mut headers = BTreeMap::from([(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        )]);
        headers.extend(extra);
        let response = host
            .fetch(ApprovalRequest {
                url,
                method: "POST".into(),
                headers,
                body: Some(body),
            })
            .await?;
        let payload = response.json.ok();
        if !response.ok
            || payload
                .as_ref()
                .and_then(|p| p.get("code"))
                .and_then(Value::as_f64)
                != Some(200.0)
        {
            let message = payload.as_ref().and_then(|p| {
                p.get("msg")
                    .filter(|v| js_truthy(Some(*v)))
                    .or_else(|| p.get("error").filter(|v| js_truthy(Some(*v))))
            });
            let message = match message {
                Some(Value::String(s)) => s.clone(),
                Some(_) => {
                    return Err(ApprovalFailure::error(
                        "Non-string approval error message cannot enter native result",
                    ));
                }
                None => format!(
                    "Aster builder approval failed with HTTP {}",
                    response.status
                ),
            };
            return Ok(result(false, message));
        }
        Ok::<_, ApprovalFailure>(result(
            true,
            "Aster builder approval submitted successfully",
        ))
    }
    .await;
    Ok(
        work.unwrap_or_else(|e| {
            result(false, e.message("Failed to submit Aster builder approval"))
        }),
    )
}
