use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::admission::{
    Family,
    wire::{Request, Response},
};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::envelope::InitPayload;
use serde_json::{Value, json};

pub struct Backpack;
fn key(seed: u8) -> String {
    STANDARD.encode(safe_cex_crypto::ed25519::public_key(&[seed; 32]).unwrap())
}
fn verifies(key: &str, text: &str, signature: &str) -> bool {
    let (Ok(public), Ok(signature)) = (STANDARD.decode(key), STANDARD.decode(signature)) else {
        return false;
    };
    (key == self::key(7) || key == self::key(8))
        && safe_cex_crypto::ed25519::verify(text.as_bytes(), &public, &signature).unwrap_or(false)
}
impl Venue for Backpack {
    const NAME: &'static str = "backpack";
    fn algorithm() -> Algorithm {
        Algorithm::Ed25519
    }
    fn init() -> InitPayload {
        InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: key(7),
            signing_mode: Some("cosigned".into()),
            instance_id: Some("negative-join-registration".into()),
            ..Default::default()
        }
    }
    fn credentials(other: bool) -> Value {
        let seed = if other { 8 } else { 7 };
        json!({"key":key(seed),"secret":STANDARD.encode([seed;32])})
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let request = Request::from_json(&frame["d"].to_string()).unwrap();
        let bytes = request.statement().message(Family::Ed25519);
        let signature =
            safe_cex_crypto::ed25519::sign(&bytes, &[if other { 8 } else { 7 }; 32]).unwrap();
        (
            "local-join-proof".into(),
            bytes,
            serde_json::to_value(
                Response::signed(request.id(), Family::Ed25519, &signature).unwrap(),
            )
            .unwrap(),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        _: &Value,
        seen: &mut Observed,
    ) -> Value {
        if *method != Method::GET {
            seen.writes += 1;
            panic!("Backpack admission attempted a venue write");
        }
        let path = uri
            .path()
            .strip_prefix("/https://api.backpack.exchange")
            .unwrap_or(uri.path());
        let instruction = match path {
            "/api/v1/account" => Some("accountQuery"),
            "/api/v1/position" => Some("positionQuery"),
            "/api/v1/capital/collateral" => Some("collateralQuery"),
            "/api/v1/orders" => Some("orderQueryAll"),
            _ => None,
        };
        if let Some(instruction) = instruction {
            seen.signed_reads += 1;
            let header = |name| {
                headers
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
            };
            let query = uri
                .query()
                .filter(|q| !q.is_empty())
                .map_or(String::new(), |q| format!("&{q}"));
            let text = format!(
                "instruction={instruction}{query}&timestamp={}&window={}",
                header("x-timestamp"),
                header("x-window")
            );
            if !verifies(header("x-api-key"), &text, header("x-signature")) {
                seen.rejections += 1;
                return json!({"code":"UNAUTHORIZED","message":"synthetic signature refusal"});
            }
        }
        match path {
            "/api/v1/markets" => {
                json!([{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}])
            }
            "/api/v1/tickers" => {
                json!([{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}])
            }
            "/api/v1/markPrices" => json!([]),
            "/api/v1/account" => json!({"leverageLimit":"10"}),
            "/api/v1/capital/collateral" => {
                json!({"netEquity":"1000","netEquityAvailable":"1000","netEquityLocked":"0","pnlUnrealized":"0"})
            }
            "/api/v1/position" | "/api/v1/orders" => json!([]),
            other => panic!("unhandled Backpack read: {other}"),
        }
    }
    fn http_status(reply: &Value) -> StatusCode {
        if reply["code"] == "UNAUTHORIZED" {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        match request["method"].as_str().unwrap() {
            "PING" => json!({"method":"PONG"}),
            "SUBSCRIBE" | "UNSUBSCRIBE" => {
                if let Some(auth) = request.get("signature") {
                    let text = format!(
                        "instruction=subscribe&timestamp={}&window={}",
                        auth[2].as_str().unwrap(),
                        auth[3].as_str().unwrap()
                    );
                    if !verifies(auth[0].as_str().unwrap(), &text, auth[1].as_str().unwrap()) {
                        seen.rejections += 1;
                        return json!({"error":{"code":"UNAUTHORIZED","message":"synthetic socket refusal"}});
                    }
                }
                json!({"result":null,"id":request["id"]})
            }
            other => {
                seen.writes += 1;
                panic!("Backpack admission attempted socket write: {other}");
            }
        }
    }
}
