use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_transport::{
    RestBody, RestRequest,
    retry::ReadClassification,
    signature_rejection::{IsSignatureRejection, SignatureRetry},
};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_bingx::{api::*, data_reads::ReadTransport};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fake"),
    }
}
struct Host {
    clock: Cell<f64>,
}
impl ApiHost for Host {
    fn create(
        &self,
        _: ApiDefaults,
        _: &ExchangeOptions,
        _: &Rc<BaseExchange>,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError> {
        unreachable!("the api is assembled directly")
    }
    fn retry(
        &self,
        _: &Rc<dyn ReadTransport>,
        _: usize,
        _: ReadClassification,
        _: IsSignatureRejection,
    ) -> Result<(), ExchangeError> {
        unreachable!("the api is assembled directly")
    }
    fn install_adapter(
        &self,
        _: &Rc<dyn ReadTransport>,
        _: &ExchangeOptions,
        _: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError> {
        unreachable!("the api is assembled directly")
    }
    fn serialize(
        &self,
        params: &safe_cex_types::retained_json::RetainedJsonInput,
    ) -> Result<String, ExchangeError> {
        Ok(safe_cex_num::js_retained_json_stringify(params)
            .unwrap()
            .unwrap())
    }
    fn hmac_sha256_hex(&self, _: &str, message: &str) -> Result<String, ExchangeError> {
        Ok(format!("sig({message})"))
    }
    fn now_ms(&self) -> f64 {
        self.clock.set(self.clock.get() + 1.0);
        self.clock.get()
    }
}
#[derive(Default)]
struct Venue {
    sent: RefCell<Vec<(RestRequest, bool)>>,
    replies: RefCell<VecDeque<Value>>,
    stamps: RefCell<Vec<SignatureRetry>>,
}
impl ReadTransport for Venue {
    fn send(&self, _: RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        unreachable!("the api always passes its signature stamp")
    }
    fn send_with_signature_retry(
        &self,
        request: RestRequest,
        retry: &SignatureRetry,
    ) -> ExchangeFuture<'_, JsNumberInput> {
        self.sent.borrow_mut().push((request, retry.has_retried()));
        self.stamps.borrow_mut().push(retry.clone());
        let reply = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unscripted extra attempt");
        Box::pin(async move { Ok(serde_json::from_value(reply).unwrap()) })
    }
}
fn api(replies: Vec<Value>) -> (BingxApi, Rc<Venue>) {
    let venue = Rc::new(Venue {
        replies: RefCell::new(replies.into()),
        ..Default::default()
    });
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
    let api = BingxApi {
        options,
        host: Rc::new(Host {
            clock: Cell::new(1000.0),
        }),
        client: venue.clone(),
    };
    (api, venue)
}
fn signed(request: &RestRequest) -> serde_json::Map<String, Value> {
    match (&request.params, &request.body) {
        (Some(Value::Object(params)), None) => params.clone(),
        (
            _,
            Some(RestBody::Untransformed(safe_cex_types::retained_json::RetainedJsonInput::Text(
                text,
            ))),
        ) => serde_json::from_str(&String::from_utf16(text).unwrap()).unwrap(),
        other => panic!("unexpected signed shape {other:?}"),
    }
}
fn code(input: JsNumberInput) -> Option<f64> {
    serde_json::to_value(&input).unwrap()["code"].as_f64()
}

#[test]
fn bingx_resends_one_resolved_signature_rejection_re_signed() {
    let rejection = json!({"code":100001,"msg":"Signature verification failed"});
    for method in ["GET", "POST"] {
        let request = RestRequest {
            method: method.into(),
            url: "/openApi/swap/v2/trade/order".into(),
            params: Some(json!({"symbol":"BTC-USDT"})),
            body: (method == "POST").then(|| {
                RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"symbol":"BTC-USDT"}),
                    )
                    .unwrap(),
                )
            }),
            ..Default::default()
        };
        let (bingx, venue) = api(vec![rejection.clone(), json!({"code":0})]);
        let reply = run(bingx.send(request.clone())).unwrap();
        assert_eq!(code(reply), Some(0.0), "native resend result {method}");
        let sent = venue.sent.borrow();
        assert_eq!(sent.len(), 2, "native resend attempts {method}");
        assert_eq!(
            sent.iter().map(|(_, stamped)| *stamped).collect::<Vec<_>>(),
            [false, true],
            "native stamp shared with the retry policy {method}"
        );
        let (first, second) = (signed(&sent[0].0), signed(&sent[1].0));
        assert_ne!(
            first["timestamp"], second["timestamp"],
            "native fresh timestamp {method}"
        );
        assert_ne!(
            first["signature"], second["signature"],
            "native re-signed {method}"
        );
        let mut unsigned = second.clone();
        unsigned.shift_remove("signature");
        assert!(
            !second["signature"].as_str().unwrap().contains("signature"),
            "native resend signs without the previous signature {method}"
        );
        assert_eq!(
            unsigned.keys().collect::<Vec<_>>(),
            ["symbol", "timestamp"],
            "native resend payload {method}"
        );
        let stamps = venue.stamps.borrow();
        assert!(
            stamps[0].has_retried(),
            "native stamps are one per call {method}"
        );
    }
    let (bingx, venue) = api(vec![rejection.clone(), rejection.clone()]);
    let reply = run(bingx.send(RestRequest {
        method: "GET".into(),
        url: "/openApi/swap/v2/user/balance".into(),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(
        code(reply),
        Some(100001.0),
        "native second rejection surfaces"
    );
    assert_eq!(venue.sent.borrow().len(), 2, "native single resend");
}
