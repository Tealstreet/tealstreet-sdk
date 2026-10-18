use crate::{
    private_event_value::{EventOps, EventValue, record, string},
    private_rest::{
        SnapshotBindings, SnapshotCall, SnapshotClient, SnapshotCompletion, SnapshotMethod,
        SnapshotRequestFuture,
    },
    request_weight::Failure,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub fn input(value: &Value) -> EventValue {
    let data = EventValue::from_json(value["data"].clone());
    match value["profile"].as_str() {
        Some("missing") => return EventValue::Undefined,
        Some("undefined-code") => {
            data.define("code", EventValue::Undefined).unwrap();
        }
        Some("surrogate-message") => {
            data.define("msg", EventValue::utf16(&[0xd800])).unwrap();
        }
        Some("surrogate-key") => {
            data.define_key(&[0xd800], EventValue::Null).unwrap();
        }
        Some("undefined") => {
            data.define("orderId", EventValue::Undefined).unwrap();
        }
        Some("surrogate") => {
            data.define("ignored", EventValue::utf16(&[0xd800]))
                .unwrap();
        }
        Some("nonfinite-metadata") => {
            data.define("code", EventValue::Number(f64::INFINITY))
                .unwrap();
            data.define("msg", EventValue::Number(f64::NEG_INFINITY))
                .unwrap();
        }
        Some("nan-metadata") => {
            data.define("code", EventValue::Number(f64::NAN)).unwrap();
            data.define("msg", EventValue::Number(f64::NAN)).unwrap();
        }
        Some("native-number") => return EventValue::Number(f64::INFINITY),
        Some("array") => return EventValue::array(vec![EventValue::utf16(&[0xdc00])]),
        _ => {}
    }
    data
}
pub fn observe(value: &EventValue) -> Value {
    match value {
        EventValue::Undefined => json!({"kind":"undefined"}),
        EventValue::Null => json!({"kind":"null"}),
        EventValue::Bool(v) => json!({"kind":"boolean","value":v}),
        EventValue::Number(v) => json!({"kind":"number","bits":format!("{:016x}",v.to_bits())}),
        EventValue::String(v) => json!({"kind":"string","units":v.as_ref()}),
        EventValue::Array(rows) => json!({"kind":"array","length":rows.borrow().len()}),
        _ => json!({"kind":value.type_of()}),
    }
}
pub fn result(value: Result<EventValue, Failure>) -> Value {
    match value {
        Ok(value) => observe(&value),
        Err(error) => {
            let error = safe_cex_exchange::js_runtime::value::Thrown::from((*error).clone()).value;
            json!({"name":observe(&error.get("name").unwrap()),"message":observe(&error.get("message").unwrap())})
        }
    }
}
pub fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("a ready primitive request must settle"),
    }
}
pub struct Bindings {
    pub calls: RefCell<Vec<(SnapshotClient, &'static str)>>,
    pub data: EventValue,
    pub cached: EventValue,
    pub profile: String,
}
impl Bindings {
    pub fn new(profile: &str) -> Self {
        Self {
            calls: RefCell::new(vec![]),
            data: record([
                ("amount", EventValue::Number(3.0)),
                ("filled", EventValue::Number(1.0)),
                ("id", string("client-A")),
            ]),
            cached: record([
                ("id", string("client-A")),
                ("symbol", string("BTCUSDT")),
                ("remaining", EventValue::Number(1.0)),
            ]),
            profile: profile.into(),
        }
    }
}
impl SnapshotBindings for Bindings {
    fn get(
        &mut self,
        client: SnapshotClient,
        endpoint: &'static str,
        params: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        self.calls.borrow_mut().push((client, endpoint));
        let params = params.unwrap();
        assert_eq!(
            params.get("symbol").unwrap().as_str().unwrap().as_ref(),
            "BTCUSDT".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            params
                .get("origClientOrderId")
                .unwrap()
                .as_str()
                .unwrap()
                .as_ref(),
            "client-A".encode_utf16().collect::<Vec<_>>()
        );
        let result = if self.profile == "notfound" {
            let error = safe_cex_exchange::runtime::ExchangeError::new("not found")
                .with_retained_response(json!({"data":{"code":-2013}}).into());
            Err(Rc::new(error))
        } else {
            Ok(self.data.clone())
        };
        Ok(Box::pin(async move {
            SnapshotCompletion {
                reaction_order: 1,
                result,
            }
        }))
    }
    fn capture(&mut self, _: SnapshotMethod) -> Result<SnapshotCall, Failure> {
        panic!("lookup does not capture callbacks")
    }
    fn now(&mut self) -> Result<f64, Failure> {
        Ok(12345.0)
    }
    fn orders(&mut self) -> Result<EventValue, Failure> {
        Ok(EventValue::array(if self.profile == "cached" {
            vec![self.cached.clone()]
        } else {
            vec![]
        }))
    }
    fn balance(&mut self) -> Result<EventValue, Failure> {
        panic!("no balance read")
    }
    fn positions(&mut self) -> Result<EventValue, Failure> {
        panic!("no position read")
    }
    fn asset_price(&mut self, _: EventValue) -> Result<f64, Failure> {
        panic!("no asset read")
    }
    fn market(&mut self, _: &str) -> Result<EventValue, Failure> {
        Ok(if self.profile == "missing-market" {
            EventValue::Undefined
        } else {
            record([])
        })
    }
    fn has_primary(&mut self) -> Result<bool, Failure> {
        Ok(self.profile == "primary")
    }
    fn carries_client_id(&mut self, order: &EventValue, id: &str) -> Result<bool, Failure> {
        Ok(order.get("id").unwrap().same(&string(id)))
    }
    fn map_history(&mut self, data: EventValue) -> Result<EventValue, Failure> {
        assert!(data.same(&self.data));
        Ok(data)
    }
}
pub fn lookup(value: EventValue, b: &Bindings) -> Value {
    let order = value.get("order").unwrap();
    let calls = b.calls.borrow();
    json!({"found":observe(&value.get("found").unwrap()),"definitive":observe(&value.get("definitive").unwrap()),"remaining":if order.is_undefined(){observe(&order)}else{observe(&order.get("remaining").unwrap())},"cachedIdentity":order.same(&b.cached),"infoIdentity":!order.is_undefined() && order.get("info").unwrap().same(&b.data),"calls":calls.iter().map(|(client,path)|json!({"client":format!("{client:?}"),"path":path})).collect::<Vec<_>>()})
}

pub fn child(value: &EventValue) -> Value {
    match value
        .array_items()?
        .and_then(|rows| rows.into_iter().next())
        .flatten()
    {
        Some(value) => json!({"present":true,"value":observe(&value)}),
        None => json!({"present":false,"value":observe(&EventValue::Undefined)}),
    }
}
