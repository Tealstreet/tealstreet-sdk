//! Local and remote auth share the core-owned signer binding.
use super::*;
use safe_cex_crypto::{Encoding, SigningOptions, sign_payload};
use safe_cex_types::ExchangeName;

fn fixed_zero(value: f64) -> String {
    if value == 0. {
        "0".into()
    } else if !value.is_finite() || value.abs() >= 1e21 {
        safe_cex_num::js_number_string(value)
    } else {
        format!("{:.0}", value.round())
    }
}

pub(crate) async fn auth_local(
    socket: &Rc<BaseWebSocket>,
    now_ms: f64,
) -> Result<bool, ExchangeError> {
    let expires = now_ms + 10_000.;
    let to_sign = format!("GET/realtime{}", safe_cex_num::js_number_string(expires));
    let options = socket.emitter().options();
    // No shared state is changed before this await; cancellation must not send auth.
    let signed = sign_payload(
        &SigningOptions {
            key: &options.key,
            secret: &options.secret,
            payload_signer: None,
        },
        ExchangeName::Bybit,
        &to_sign,
        Encoding::Hex,
    )
    .await
    .map_err(ExchangeError::new)?;
    socket.send_with_log(&serde_json::json!({"op":"auth","args":[options.key, fixed_zero(expires), signed.signature]}).to_string())?;
    Ok(true)
}

pub(crate) async fn auth(
    socket: &Rc<BaseWebSocket>,
    parent: &safe_cex_exchange::exchange::BaseExchange,
    account_name: Option<String>,
    now_ms: f64,
) -> Result<bool, ExchangeError> {
    use safe_cex_transport::signing::{RemoteSignerRequest, RemoteSignerResponse};
    use safe_cex_types::ws_auth::{WsAuth, WsAuthRequest};
    let Some(signer) = &parent.signers.remote_signer else {
        return auth_local(socket, now_ms).await;
    };
    let expires = now_ms + 10_000.;
    let expires_json = if expires.is_finite() {
        safe_cex_num::js_number_string(expires)
    } else {
        "null".into()
    };
    let extra_data =
        serde_json::value::RawValue::from_string(format!("{{\"expires\":{expires_json}}}"))
            .map_err(|error| ExchangeError::new(error.to_string()))?;
    let response = signer
        .sign(RemoteSignerRequest::WsAuth {
            exchange_name: ExchangeName::Bybit,
            account_name,
            request: WsAuthRequest {
                id: Some(serde_json::Value::String(parent.runtime.unique_id())),
                request_type: None,
                ws_auth: Some(WsAuth {
                    to_sign: Some(format!(
                        "GET/realtime{}",
                        safe_cex_num::js_number_string(expires)
                    )),
                    output_format: Some(serde_json::Value::String("hex".into())),
                    extra_data: Some(extra_data),
                }),
            },
        })
        .await
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    let response = match response {
        RemoteSignerResponse::WsAuth(response) => response,
        RemoteSignerResponse::Http(_) => {
            return Err(ExchangeError::new(
                "Bybit WS auth received HTTP signer response",
            ));
        }
    };
    if let Some(error) = response.error.filter(|error| !error.is_empty()) {
        socket.emit_and_log_error(
            &format!("Remote WS signing unavailable: {error}"),
            None,
            None,
        )?;
        return Ok(false);
    }
    socket.send_with_log(&serde_json::json!({
        "op": "auth",
        "args": [socket.emitter().options().key, fixed_zero(expires), response.ws_auth_signature]
    }).to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn to_fixed_zero_keeps_negative_fraction_and_rounds_ties_away() {
        assert_eq!(fixed_zero(-0.), "0");
        assert_eq!(fixed_zero(-0.1), "-0");
        assert_eq!(fixed_zero(2.5), "3");
        assert_eq!(fixed_zero(-2.5), "-3");
        assert_eq!(
            fixed_zero(1_000_000_000_000_000_100.),
            "1000000000000000128"
        );
    }
}
