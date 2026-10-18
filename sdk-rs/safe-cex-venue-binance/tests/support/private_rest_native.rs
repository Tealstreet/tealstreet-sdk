use super::*;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_venue_binance::exchange_classes::Method;
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

struct Http {
    raw: &'static str,
    paths: Mutex<Vec<String>>,
    urls: Mutex<Vec<String>>,
}
impl RestAdapter for Http {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.urls.lock().unwrap().push(request.url.clone());
        let path = request.url.split('?').next().unwrap().to_owned();
        self.paths.lock().unwrap().push(path.clone());
        let data = if path.to_ascii_lowercase().contains("algo") {
            "[]"
        } else {
            self.raw
        };
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(data.as_bytes().to_vec()),
            })
        })
    }
}
fn source(pm: bool, raw: &str) -> Value {
    let mut process = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-rest-native-source.cjs"
        ))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&json!({"pm":pm,"raw":raw})).unwrap())
        .unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn owner(raw: &'static str, pm: bool) -> (Rc<ExchangeOwner>, Rc<Runtime>, Arc<Http>) {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http {
        raw,
        paths: Mutex::new(vec![]),
        urls: Mutex::new(vec![]),
    });
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let owner = ExchangeOwner::new(
        if pm { Venue::PM } else { Venue::Futures },
        if pm {
            safe_cex_types::ExchangeName::Binancepm
        } else {
            safe_cex_types::ExchangeName::Binance
        },
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        dependencies,
    )
    .unwrap();
    (owner, runtime, http)
}
fn units(value: &EventValue) -> Vec<u16> {
    value.as_str().expect("native string").to_vec()
}
#[test]
fn native_non_array_order_diagnostics_keep_unpaired_units_and_stringify_values() {
    for raw in [
        r#"{"ignored":"\ud800","nested":[null,0]}"#,
        r#"{"ignored":"\udc00","n":1e400}"#,
    ] {
        let expected = source(false, raw);
        let (owner, runtime, http) = owner(raw, false);
        let messages = Rc::new(RefCell::new(vec![]));
        let observed = messages.clone();
        owner.base.on_args(
            "log",
            Rc::new(move |_, args| {
                observed.borrow_mut().push(units(&args[0]));
                Ok(())
            }),
        );
        let rows = run(&runtime, owner.invoke(Method::Orders, vec![])).unwrap();
        let EventValue::Array(rows) = rows else {
            panic!("native orders array")
        };
        assert!(rows.borrow().is_empty());
        assert_eq!(expected["rows"], 0);
        assert_eq!(expected["errors"], 0);
        assert_eq!(json!(*messages.borrow()), expected["log"]);
        assert_eq!(
            http.paths.lock().unwrap().as_slice(),
            ["/fapi/v1/openOrders", "/fapi/v1/openAlgoOrders"]
        );
        owner.dispose().unwrap();
    }
}
#[test]
fn native_pm_order_shape_check_ignores_unread_surrogate_children() {
    for raw in [
        r#"{"ignored":"\ud800"}"#,
        r#"{"ignored":{"deeper":"\udc00"},"code":0,"msg":"bad"}"#,
        r#"{"msg":"\ud800"}"#,
    ] {
        let expected = source(true, raw);
        let (owner, runtime, http) = owner(raw, true);
        let error = run(&runtime, owner.invoke(Method::RegularOrders, vec![])).unwrap_err();
        assert_eq!(error.name, expected["name"]);
        let native = safe_cex_exchange::js_runtime::value::Thrown::from((*error).clone()).value;
        assert_eq!(
            json!(units(&native.get("message").unwrap())),
            expected["message"]
        );
        assert_eq!(
            http.paths.lock().unwrap().as_slice(),
            ["/papi/v1/um/openOrders"]
        );
        owner.dispose().unwrap();
    }
}
#[test]
fn native_order_alias_lookup_does_not_traverse_unread_array_children() {
    let raw = r#"[["\ud800"]]"#;
    let expected = source(false, raw);
    let (owner, runtime, http) = owner(raw, false);
    let rows = run(&runtime, owner.invoke(Method::Orders, vec![])).unwrap();
    let EventValue::Array(rows) = rows else {
        panic!("native orders array")
    };
    assert_eq!(rows.borrow().len(), 1);
    let row = rows.borrow()[0].clone().unwrap();
    assert!(row.get("id").unwrap().is_undefined());
    assert!(matches!(row.get("remaining").unwrap(), EventValue::Number(n) if n.is_nan()));
    let info = row.get("info").unwrap();
    assert_eq!(json!(units(&info.get("0").unwrap())), expected["info"]);
    assert_eq!(expected["rows"], 1);
    assert_eq!(expected["errors"], 0);
    assert!(expected["log"].as_array().unwrap().is_empty());
    assert_eq!(http.paths.lock().unwrap().len(), 2);
    owner.dispose().unwrap();
}

#[test]
fn native_client_order_lookup_reconciles_lost_placement_response_through_signed_http() {
    use safe_cex_store::StoreUpdate;
    let raw = r#"{"orderId":17,"clientOrderId":"client-A","symbol":"BTCUSDT","status":"NEW","type":"LIMIT","side":"BUY","price":"12.5","origQty":"3","executedQty":"1","time":1000,"updateTime":1200}"#;
    let (owner, runtime, http) = owner(raw, false);
    owner.base.store.update(StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.1,"amount":0.1},"limits":{"amount":{"min":0.1,"max":10},"leverage":{"min":1,"max":100}}})).unwrap()]),
        ..Default::default()
    }, false).unwrap();
    assert!(owner.base.store.orders().is_empty());
    let result = run(
        &runtime,
        owner.invoke(
            Method::FindOrder,
            vec![string("BTCUSDT"), string("client-A")],
        ),
    )
    .unwrap();
    assert!(matches!(
        result.get("found").unwrap(),
        EventValue::Bool(true)
    ));
    let order = result.get("order").unwrap();
    assert_eq!(
        units(&order.get("id").unwrap()),
        "17".encode_utf16().collect::<Vec<_>>()
    );
    for (field, expected) in [
        ("price", 12.5_f64),
        ("amount", 3.0),
        ("filled", 1.0),
        ("remaining", 2.0),
        ("lastUpdate", 1200.0),
    ] {
        let EventValue::Number(actual) = order.get(field).unwrap() else {
            panic!("native numeric {field}")
        };
        assert_eq!(actual.to_bits(), expected.to_bits(), "{field}");
    }
    assert_eq!(
        units(&order.get("info").unwrap().get("clientOrderId").unwrap()),
        "client-A".encode_utf16().collect::<Vec<_>>()
    );
    assert_eq!(http.paths.lock().unwrap().as_slice(), ["/fapi/v1/order"]);
    let urls = http.urls.lock().unwrap();
    assert_eq!(urls.len(), 1);
    for parameter in [
        "symbol=BTCUSDT",
        "origClientOrderId=client-A",
        "timestamp=",
        "signature=",
    ] {
        assert!(
            urls[0].contains(parameter),
            "signed native lookup {parameter}: {}",
            urls[0]
        );
    }
    owner.dispose().unwrap();
}

#[test]
fn native_pm_order_diagnostics_keep_nested_nonfinite_metadata() {
    for raw in [
        r#"{"code":1e400,"msg":-1e400}"#,
        r#"{"code":-1e400,"msg":[1e400]}"#,
    ] {
        let expected = source(true, raw);
        let (owner, runtime, http) = owner(raw, true);
        let error = run(&runtime, owner.invoke(Method::RegularOrders, vec![])).unwrap_err();
        assert_eq!(error.name, expected["name"]);
        assert_eq!(
            json!(error.message.encode_utf16().collect::<Vec<_>>()),
            expected["message"]
        );
        assert_eq!(
            http.paths.lock().unwrap().as_slice(),
            ["/papi/v1/um/openOrders"]
        );
        owner.dispose().unwrap();
    }
}
#[test]
fn native_captured_error_emitter_preserves_lone_utf16_messages() {
    use safe_cex_venue_binance::position_snapshots::PositionSnapshotBindings;
    for units in [[0xd800_u16], [0xdc00_u16]] {
        let (owner, runtime, http) = owner("[]", true);
        let errors = Rc::new(RefCell::new(vec![]));
        let seen = errors.clone();
        owner.base.on(
            "error",
            Rc::new(move |_, value| {
                seen.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        let emit = owner.state.capture_emit().unwrap();
        emit(EventValue::utf16(&units)).expect("native source message reaches Base reporting");
        let mut expected = "Exchange Error: ".encode_utf16().collect::<Vec<_>>();
        expected.extend(units);
        assert_eq!(errors.borrow().len(), 1);
        assert_eq!(errors.borrow()[0].as_str().unwrap().as_ref(), expected);
        assert_eq!(&*runtime.reported.borrow(), &["ExchangeError"]);
        assert!(http.paths.lock().unwrap().is_empty());
        owner.dispose().unwrap();
    }
}

#[test]
fn native_pm_order_errors_report_retained_cause_message_units() {
    for raw in [r#"{"msg":"\ud800"}"#, r#"{"msg":"\udc00"}"#] {
        let expected = source(true, raw);
        let (owner, runtime, http) = owner(raw, true);
        let errors = Rc::new(RefCell::new(vec![]));
        let seen = errors.clone();
        owner.base.on(
            "error",
            Rc::new(move |_, value| {
                seen.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        let rows = run(&runtime, owner.invoke(Method::Orders, vec![])).unwrap();
        assert!(matches!(rows, EventValue::Array(ref rows) if rows.borrow().is_empty()));
        let mut expected_units = "Exchange Error: ".encode_utf16().collect::<Vec<_>>();
        expected_units.extend(
            expected["message"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| unit.as_u64().unwrap() as u16),
        );
        assert_eq!(errors.borrow().len(), 1);
        assert_eq!(
            errors.borrow()[0].as_str().unwrap().as_ref(),
            expected_units
        );
        assert_eq!(&*runtime.reported.borrow(), &["ExchangeError"]);
        assert_eq!(
            http.paths.lock().unwrap().as_slice(),
            ["/papi/v1/um/openOrders"]
        );
        owner.dispose().unwrap();
    }
}
