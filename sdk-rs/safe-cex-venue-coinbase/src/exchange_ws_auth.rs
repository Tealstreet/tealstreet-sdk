//! CoinbaseExchange.signWsAuth over live inert request/options roots.
use crate::{
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use serde_json::json;
use std::rc::Rc;
pub type ExchangeWsAuthAsync =
    Result<Promise<Result<Option<Frame>, PortfolioFailure>>, PortfolioFailure>;
#[derive(Clone)]
pub struct ExchangeWsAuthResponse {
    /// id is always an own property, including an undefined id.
    pub id: Option<Frame>,
    pub error: Option<Frame>,
    pub signature: Option<Frame>,
    pub has_error: bool,
    pub has_signature: bool,
}
pub trait ExchangeWsAuthHost {
    fn options(&self) -> Frame;
    fn sign_local(&self, key: Option<Frame>, secret: Option<Frame>) -> ExchangeWsAuthAsync;
}
pub struct ExchangeWsAuthReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
fn prop(value: Option<&Frame>, name: &str) -> Result<Option<Frame>, PortfolioFailure> {
    match value {
        None | Some(Frame::Null) => Err(Rc::new(PortfolioThrown::from_value(Some(
            json!({"name":"TypeError","message":format!("Cannot read properties of {} (reading '{name}')",if value.is_none(){"undefined"}else{"null"})}),
        )))),
        Some(v) => Ok(v.get(name)),
    }
}
fn text(v: &str) -> Frame {
    Frame::String(v.encode_utf16().collect())
}
fn truthy(v: Option<&Frame>) -> bool {
    match v {
        None | Some(Frame::Null) => false,
        Some(Frame::Bool(v)) => *v,
        Some(Frame::Number(v)) => *v != 0.0 && !v.is_nan(),
        Some(Frame::String(v)) => !v.is_empty(),
        _ => true,
    }
}
fn reject(id: Option<Frame>, message: Frame) -> ExchangeWsAuthResponse {
    ExchangeWsAuthResponse {
        id,
        error: Some(message),
        signature: None,
        has_error: true,
        has_signature: false,
    }
}
pub fn sign_ws_auth<H: ExchangeWsAuthHost + 'static>(
    r: Rc<ExchangeWsAuthReceiver<H>>,
    request: Option<Frame>,
) -> Promise<Result<ExchangeWsAuthResponse, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let ws = prop(request.as_ref(), "wsAuth")?;
            let format = ws.as_ref().and_then(|v| v.get("outputFormat"));
            if !matches!(&format,Some(Frame::String(v)) if v.iter().copied().eq("coinbase-jwt".encode_utf16()))
            {
                let id = prop(request.as_ref(), "id")?;
                let ws = prop(request.as_ref(), "wsAuth")?;
                let format = ws.as_ref().and_then(|v| v.get("outputFormat"));
                let mut message = "Unsupported Coinbase WS auth outputFormat: "
                    .encode_utf16()
                    .collect::<Vec<_>>();
                message.extend(crate::public_data::string(format).map_err(|e| {
                    Rc::new(PortfolioThrown::from_value(Some(
                        json!({"name":e.name,"message":e.message}),
                    )))
                })?);
                return Ok(reject(id, Frame::String(message)));
            }
            if !truthy(prop(Some(&r.host.options()), "key")?.as_ref())
                || !truthy(prop(Some(&r.host.options()), "secret")?.as_ref())
            {
                return Ok(reject(
                    prop(request.as_ref(), "id")?,
                    text("No Coinbase key or secret available for WS auth signing"),
                ));
            }
            let result = async {
                let id = prop(request.as_ref(), "id")?;
                let key = prop(Some(&r.host.options()), "key")?;
                let secret = prop(Some(&r.host.options()), "secret")?;
                let signature = r.host.sign_local(key, secret)?.await?;
                Ok::<_, PortfolioFailure>(ExchangeWsAuthResponse {
                    id,
                    error: None,
                    signature,
                    has_error: false,
                    has_signature: true,
                })
            }
            .await;
            match result {
                Ok(v) => Ok(v),
                Err(e) => {
                    let id = prop(request.as_ref(), "id")?;
                    let message = e.value.as_ref().and_then(|v| v.get("message"));
                    Ok(reject(
                        id,
                        if safe_cex_num::js_truthy(message) {
                            Frame::from_json(message.expect("truthy message"))
                        } else {
                            text("Coinbase WS auth signing failed")
                        },
                    ))
                }
            }
        }),
        &spawn,
    )
}
