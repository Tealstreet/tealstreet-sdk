//! Private WebSocket method bodies with host-supplied transport, logging and token fetch.
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::{JsJsonValue, js_json_stringify};
use std::{cell::RefCell, future::Future};

#[derive(Debug, Default)]
pub struct FuturesAuthState {
    pub waiting_challenge: bool,
}

#[derive(Debug)]
pub struct SpotAuthState {
    pub authenticated: bool,
    pub token: JsJsonValue,
}

impl Default for SpotAuthState {
    fn default() -> Self {
        Self {
            authenticated: false,
            token: JsJsonValue::Undefined,
        }
    }
}

pub fn set_token(state: &RefCell<SpotAuthState>, token: JsJsonValue) {
    state.borrow_mut().token = token;
}

fn error_text(error: &ExchangeError) -> String {
    if error.message.is_empty() {
        error.js_error_string()
    } else {
        error.message.clone()
    }
}

fn stringify(value: JsJsonValue) -> Result<String, ExchangeError> {
    js_json_stringify(&value)
        .map_err(|error| {
            let mut native = ExchangeError::new(error.message);
            native.name = error.name.into();
            native
        })?
        .ok_or_else(|| ExchangeError::new("Undefined WebSocket frame"))
}

pub async fn authenticate(
    state: &RefCell<FuturesAuthState>,
    key: Option<&str>,
    secret: Option<&str>,
    remote_signer: bool,
    mut send: impl FnMut(String) -> Result<(), ExchangeError>,
    mut log: impl FnMut(String, LogSeverity) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    let present = |text: Option<&str>| text.is_some_and(|text| !text.is_empty());
    if !present(key) || (!present(secret) && !remote_signer) {
        log(
            "Missing API key or secret/remoteSigner for authentication".into(),
            LogSeverity::Warning,
        )?;
        return Ok(());
    }
    state.borrow_mut().waiting_challenge = true;
    let frame = JsJsonValue::Object(vec![
        ("event".into(), JsJsonValue::String("challenge".into())),
        ("api_key".into(), JsJsonValue::String(key.unwrap().into())),
    ]);
    if let Err(error) = stringify(frame).and_then(&mut send) {
        log(
            format!("Failed to send challenge: {}", error_text(&error)),
            LogSeverity::Error,
        )?;
        state.borrow_mut().waiting_challenge = false;
    }
    Ok(())
}

pub async fn futures_on_open<F: Future<Output = Result<(), ExchangeError>>>(
    is_disposed: impl Fn() -> bool,
    authenticate: impl FnOnce() -> Result<F, ExchangeError>,
    mut log: impl FnMut(String, LogSeverity) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if !is_disposed() {
        let result = match authenticate() {
            Ok(future) => future.await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            log(
                format!("Kraken private WS auth failed: {}", error_text(&error)),
                LogSeverity::Error,
            )?;
        }
    }
    Ok(())
}

fn truthy(value: &JsJsonValue) -> bool {
    match value {
        JsJsonValue::Undefined | JsJsonValue::Null => false,
        JsJsonValue::Bool(value) => *value,
        JsJsonValue::Number(value) => *value != 0.0 && !value.is_nan(),
        JsJsonValue::String(value) => !value.is_empty(),
        JsJsonValue::BigInt(value) => value.to_string() != "0",
        JsJsonValue::Array(_) | JsJsonValue::Object(_) => true,
    }
}

pub async fn spot_on_open<F: Future<Output = Result<JsJsonValue, ExchangeError>>>(
    state: &RefCell<SpotAuthState>,
    is_disposed: impl Fn() -> bool,
    fetch_token: impl FnOnce() -> Result<F, ExchangeError>,
    mut send: impl FnMut(String) -> Result<(), ExchangeError>,
    mut log: impl FnMut(String, LogSeverity) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if is_disposed() {
        return Ok(());
    }
    {
        let mut state = state.borrow_mut();
        state.authenticated = false;
        state.token = JsJsonValue::Undefined;
    }
    let fetched = match fetch_token() {
        Ok(future) => future.await,
        Err(error) => Err(error),
    };
    match fetched {
        Ok(token) => {
            if is_disposed() {
                return Ok(());
            }
            state.borrow_mut().token = token;
        }
        Err(_) => {
            log("Failed to fetch WebSocket token".into(), LogSeverity::Error)?;
            return Ok(());
        }
    }
    if !truthy(&state.borrow().token) {
        log("No WebSocket token available".into(), LogSeverity::Error)?;
        return Ok(());
    }
    state.borrow_mut().authenticated = true;
    let token = state.borrow().token.clone();
    send(stringify(JsJsonValue::Object(vec![
        ("method".into(), JsJsonValue::String("subscribe".into())),
        (
            "params".into(),
            JsJsonValue::Object(vec![
                ("channel".into(), JsJsonValue::String("executions".into())),
                ("token".into(), token),
                ("snap_orders".into(), JsJsonValue::Bool(true)),
                ("snap_trades".into(), JsJsonValue::Bool(false)),
                ("order_status".into(), JsJsonValue::Bool(true)),
            ]),
        ),
    ]))?)?;
    let token = state.borrow().token.clone();
    send(stringify(JsJsonValue::Object(vec![
        ("method".into(), JsJsonValue::String("subscribe".into())),
        (
            "params".into(),
            JsJsonValue::Object(vec![
                ("channel".into(), JsJsonValue::String("balances".into())),
                ("token".into(), token),
                ("snapshot".into(), JsJsonValue::Bool(true)),
            ]),
        ),
    ]))?)?;
    Ok(())
}
