// Finite observers call production methods; binding.rs supplies scripted HTTP only.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/binding.rs"));

pub struct AcceptedBinding(Fixture);
impl AcceptedBinding {
    pub fn new() -> Self {
        Self(fixture())
    }
    pub fn exchange(&self) -> Rc<BreakoutpropExchange> {
        self.0.exchange.clone()
    }
    pub fn host(&self) -> Rc<ProductionHost> {
        self.0.host.clone()
    }
    pub fn runtime(&self) -> Rc<dyn WsRuntime> {
        self.0.runtime.clone()
    }
    pub fn now(&self, value: u64) {
        self.0.runtime.now.set(value);
    }
    pub fn http(&self) -> usize {
        self.0.wire.requests.borrow().len()
    }
    pub fn run<T>(&self, pending: impl Future<Output = T>) -> T {
        self.0.run(pending)
    }
}
fn native_order_fixture() -> Value {
    json!({"account":"default:A","version":1,"orderId":3,"orderCode":"bound-order","clientOrderId":"client","actionCode":"ACCEPT","legCount":1,"type":"LIMIT","instrument":"BTCUSD","status":"WORKING","finalStatus":false,"legs":[{"instrument":"BTCUSD","positionEffect":"CLOSE","positionCode":"p","side":"BUY","price":17.25,"legRatio":1,"quantity":11.5,"filledQuantity":3.25,"remainingQuantity":8.25,"averagePrice":16}],"side":"BUY","tif":"GTC","marginRate":0.2,"issueTime":"2026-01-01T00:00:00Z","transactionTime":"2026-01-01T00:00:00Z","executions":[],"cashTransactions":[]})
}
fn prepare_portfolio(f: &Fixture, input: &Value) {
    let mut body = portfolio();
    if input["withOrder"] == true {
        body["portfolios"][0]["orders"] = json!([native_order_fixture()]);
    }
    f.wire
        .faults
        .borrow_mut()
        .push_back(("accounts/default:A/portfolio".into(), 200, body));
}
fn positions_native(rows: Vec<Position>) -> Value {
    json!(rows.iter().map(|p| json!({"symbol":p.symbol,"side":format!("{:?}",p.side),"contracts":format!("{:016x}",p.contracts.to_bits())})).collect::<Vec<_>>())
}
fn orders_native(rows: Vec<Order>) -> Value {
    json!(rows.iter().map(|o|json!({"id":o.id,"symbol":o.symbol,"side":format!("{:?}",o.side),"type":format!("{:?}",o.r#type),"price":format!("{:016x}",o.price.to_bits()),"amount":format!("{:016x}",o.amount.to_bits()),"filled":format!("{:016x}",o.filled.to_bits()),"remaining":format!("{:016x}",o.remaining.to_bits()),"reduceOnly":o.reduce_only})).collect::<Vec<_>>())
}
pub fn accepted_positions(
    input: &Value,
    invoke: impl FnOnce(
        Rc<BreakoutpropExchange>,
        Option<Vec<String>>,
    ) -> ExchangeFuture<'static, Vec<Position>>,
) -> Value {
    let f = fixture();
    f.fetch_markets();
    let before = f.store.positions().len();
    let symbols = Some(
        input["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
    );
    let rows = f.run(invoke(f.exchange.clone(), symbols)).unwrap();
    json!({"rows":positions_native(rows),"storeUnchanged":f.store.positions().len()==before,"loaded":f.store.loaded().positions,"portfolioRequests":f.wire.requests.borrow().iter().filter(|r|r["url"]=="accounts/default:A/portfolio").count()})
}
pub fn accepted_orders(
    input: &Value,
    invoke: impl FnOnce(Rc<BreakoutpropExchange>) -> ExchangeFuture<'static, Vec<Order>>,
) -> Value {
    let f = fixture();
    prepare_portfolio(&f, input);
    let rows = f.run(invoke(f.exchange.clone())).unwrap();
    json!({"rows":orders_native(rows),"storeEmpty":f.store.orders().is_empty(),"loaded":f.store.loaded().orders,"auth":f.request("GET","accounts/default:A/portfolio")["headers"]["Coinbase-Authorization"]})
}
pub fn accepted_dispatch(
    input: &Value,
    invoke: impl FnOnce(Rc<BreakoutpropExchange>, &'static str, Vec<Js>) -> ExchangeFuture<'static, Js>,
) -> Value {
    let f = fixture();
    let name = match input["member"].as_str().unwrap() {
        "fetchPositions" => "fetchPositions",
        "fetchOrders" => "fetchOrders",
        "placeOrder" => "placeOrder",
        "missing" => "missing",
        _ => panic!("unreviewed native member"),
    };
    if name == "fetchPositions" {
        f.fetch_markets();
    }
    if name == "fetchOrders" {
        prepare_portfolio(&f, input);
    }
    let args = if name == "fetchPositions" {
        vec![from_json(&input["symbols"])]
    } else if name == "placeOrder" {
        vec![from_json(&input["opts"])]
    } else {
        vec![]
    };
    let result = f.run(invoke(f.exchange.clone(), name, args));
    match result {
        Ok(value) => {
            let rows = if name == "fetchPositions" {
                positions_native(safe_cex_exchange::js_runtime::value::to_typed(&value).unwrap())
            } else {
                orders_native(safe_cex_exchange::js_runtime::value::to_typed(&value).unwrap())
            };
            json!({"ok":true,"rows":rows,"portfolioRequests":f.wire.requests.borrow().iter().filter(|r|r["url"]=="accounts/default:A/portfolio").count()})
        }
        Err(error) => json!({"ok":false,"name":error.name,"http":f.wire.requests.borrow().len()}),
    }
}
pub fn accepted_report(
    input: &Value,
    invoke: impl FnOnce(&BreakoutpropExchange, &str) -> Result<(), ExchangeError>,
) -> Value {
    let f = fixture();
    let logs = Rc::new(RefCell::new(Vec::new()));
    let sink = logs.clone();
    f.exchange.base.on_args(
        "error",
        Rc::new(move |_, value| {
            sink.borrow_mut().push(json!(value));
            Ok(())
        }),
    );
    if input["disposed"] == true {
        f.exchange.dispose().unwrap();
    }
    let result = invoke(&f.exchange, input["message"].as_str().unwrap());
    json!({"ok":result.is_ok(),"events":*logs.borrow(),"reports":f.runtime.exceptions.borrow().len(),"http":f.wire.requests.borrow().len()})
}
