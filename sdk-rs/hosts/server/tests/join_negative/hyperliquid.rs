use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::admission::{
    Family,
    wire::{Request, Response},
};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::envelope::InitPayload;
use serde_json::{Value, json};

pub struct Hyperliquid;
fn scalar(last: u8) -> String {
    let mut bytes = [0; 32];
    bytes[31] = last;
    safe_cex_crypto::hex(&bytes)
}
fn address(last: u8) -> String {
    safe_cex_crypto::ethereum::private_key_address(&scalar(last)).unwrap()
}
impl Venue for Hyperliquid {
    const NAME: &'static str = "hyperliquid";
    fn algorithm() -> Algorithm {
        Algorithm::Secp256k1Digest
    }
    fn init() -> InitPayload {
        serde_json::from_value(json!({"accountName":ACCOUNT,"exchangeName":Self::NAME,"signingMode":"cosigned",
            "instanceId":"negative-join-registration","key":address(7),"hyperliquidSignerAddress":address(7)})).unwrap()
    }
    fn credentials(other: bool) -> Value {
        let last = if other { 8 } else { 7 };
        json!({"key":address(last),"secret":scalar(last)})
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let request = Request::from_json(&frame["d"].to_string()).unwrap();
        let preimage = request.statement().message(Family::Ethereum);
        let signature =
            safe_cex_crypto::ethereum::sign_digest(&preimage, &scalar(if other { 8 } else { 7 }))
                .unwrap();
        let signature = signature.strip_prefix("0x").unwrap_or(&signature);
        let bytes: Vec<_> = (0..signature.len())
            .step_by(2)
            .map(|n| u8::from_str_radix(&signature[n..n + 2], 16).unwrap())
            .collect();
        (
            "local-join-proof".into(),
            preimage,
            serde_json::to_value(Response::signed(request.id(), Family::Ethereum, &bytes).unwrap())
                .unwrap(),
        )
    }
    fn http(method: &Method, uri: &Uri, _: &HeaderMap, body: &Value, seen: &mut Observed) -> Value {
        assert_eq!(method, Method::POST);
        if !uri.path().ends_with("/info") {
            seen.writes += 1;
            panic!("Hyperliquid join attempted a write");
        }
        match body["type"]
            .as_str()
            .expect("native Hyperliquid info request")
        {
            "metaAndAssetCtxs" => {
                json!([{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100"}]])
            }
            "spotMetaAndAssetCtxs" => json!([{"tokens":[],"universe":[]},[]]),
            "perpDexs" => json!([null]),
            "userRole" => json!({"role":"user"}),
            "userAbstraction" => json!("disabled"),
            "maxBuilderFee" => json!(1000),
            "clearinghouseState" => {
                json!({"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":1000})
            }
            "spotClearinghouseState" => json!({"balances":[]}),
            "openOrders"
            | "frontendOpenOrders"
            | "userFills"
            | "userFillsByTime"
            | "userFunding"
            | "userNonFundingLedgerUpdates"
            | "candleSnapshot" => json!([]),
            "userFees" => json!({"userCrossRate":"0.00045","userAddRate":"0.00015"}),
            other => panic!("unhandled native Hyperliquid read: {other}"),
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        match request["method"].as_str().unwrap() {
            "subscribe" | "unsubscribe" => json!({"channel":"subscriptionResponse","data":request}),
            "ping" => json!({"channel":"pong"}),
            _ => {
                seen.writes += 1;
                panic!("Hyperliquid join attempted a socket write");
            }
        }
    }
}
