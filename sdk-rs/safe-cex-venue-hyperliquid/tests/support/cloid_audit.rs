//! Audit the cloid claims through the constructed production class/imports.
use super::{HyperliquidDependencies, HyperliquidExchange, RuntimeClock};
use crate::binding_test_runtime as host;
use crate::rate_limit::RateLimiter;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_exchange::{exchange::Exchange, native_store::DefaultStore};
use serde_json::json;
use std::{io::Write, process::Stdio, rc::Rc, sync::Arc};

#[allow(clippy::arc_with_non_send_sync)]
fn fixture() -> (Rc<HyperliquidExchange>, Rc<host::Host>, Arc<host::Routes>) {
    let runtime = Rc::new(host::Host::default());
    let routes = host::Routes::new(vec![("info:orderStatus", json!({"status":"unknownOid"}))]);
    let exchange = HyperliquidExchange::new(
        serde_json::from_value(
            json!({"key":"","secret":"","applicationId":"0x1111111111111111111111111111111111111111"}),
        )
        .unwrap(),
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: runtime.clone(),
            http: routes.clone(),
            limiter: Rc::new(RateLimiter::new(
                Arc::new(RuntimeClock::new(runtime.clone())),
                1200.,
                60_000.,
            )),
            signers: Default::default(),
            external_public_data: None,
        },
    )
    .unwrap();
    (exchange, runtime, routes)
}

#[test]
fn cloid_audit_generated_wires_reach_the_real_placement_cache() {
    assert!(crate::cloid::placement_record_from_wire(
        &json!({"p":"1","s":"1","b":true,"r":"outside-placement-domain","t":{"limit":{"tif":"Gtc"}}}),
        "BTC-PERP".into(), 0.,
    ).unwrap_err().contains("reduceOnly must be a boolean"));
    let (exchange, _, _) = fixture();
    let binding = exchange.binding.borrow().as_ref().unwrap().clone();
    let mut r = binding.realm;
    let producer = r.global_binding("orderRequestToOrderWire").unwrap();
    let mut source_inputs = vec![];
    let mut observed = vec![];
    for reduce_only in [false, true] {
        for buy in [false, true] {
            for shape in ["limit", "tp", "sl"] {
                let order_type = if shape == "limit" {
                    r.object_from(vec![(
                        "limit",
                        r.object_from(vec![("tif", Js::str("Gtc"))]),
                    )])
                } else {
                    let trigger = r.object_from(vec![
                        ("triggerPx", Js::Number(23.75)),
                        ("isMarket", Js::Bool(true)),
                        ("tpsl", Js::str(shape)),
                    ]);
                    r.object_from(vec![("trigger", trigger)])
                };
                let request = r.object_from(vec![
                    ("is_buy", Js::Bool(buy)),
                    ("limit_px", Js::Number(91.25)),
                    ("sz", Js::Number(0.5)),
                    ("reduce_only", Js::Bool(reduce_only)),
                    ("order_type", order_type),
                ]);
                let wire = r
                    .call(&producer, &Js::Undefined, &[request, Js::Number(7.)])
                    .unwrap();
                assert!(
                    r.get_str(&wire, "r")
                        .unwrap()
                        .strict_equals(&Js::Bool(reduce_only)),
                    "HL_CLOID_AUDIT producer must supply boolean r"
                );
                let id = format!("0x{:032x}", source_inputs.len() + 1);
                r.set_str(&wire, "c", Js::str(&id)).unwrap();
                let raw = r.to_serde(&wire).unwrap();
                source_inputs
                    .push(json!({"wire":raw,"symbol":"BTC-PERP","placedAt":1_700_000_000_000_f64}));
                let wires = r.new_array(vec![wire]);
                r.call_method(
                    &binding.instance,
                    "recordPlacement",
                    &[wires, Js::str("BTC-PERP")],
                )
                .unwrap();
                let record = r
                    .call_method(&binding.instance, "getPlacementRecord", &[Js::str(&id)])
                    .unwrap();
                let mut row = serde_json::Map::new();
                for key in ["price", "amount", "placedAt"] {
                    let value = r.get_str(&record, key).unwrap().as_f64().unwrap();
                    row.insert(key.into(), json!(format!("{:016x}", value.to_bits())));
                }
                for key in ["type", "side", "reduceOnly", "symbol", "timeInForce"] {
                    let value = r.get_str(&record, key).unwrap();
                    row.insert(
                        key.into(),
                        if matches!(value, Js::Undefined) {
                            serde_json::Value::Null
                        } else {
                            r.to_serde(&value).unwrap()
                        },
                    );
                }
                observed.push(serde_json::Value::Object(row));
            }
        }
    }
    let mut child = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/cloid-oracle.cjs"
        ))
        .arg("--binding-audit")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&source_inputs).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(observed.len(), 12);
    assert_eq!(
        observed, expected,
        "HL_CLOID_AUDIT actual constructor/producer/cache vs maintained source"
    );
}

#[test]
fn cloid_audit_pattern_controls_the_exported_lookup_request() {
    let (exchange, runtime, routes) = fixture();
    for (id, valid) in [
        ("0x0123456789abcdef0123456789abcdef", true),
        ("0x0123456789ABCDEF0123456789ABCDEF", false),
        ("0x0123456789abcdef0123456789abcdef\n", false),
        ("0x0123456789abcdef0123456789abcde", false),
    ] {
        let before = routes.count("info:orderStatus");
        runtime
            .block_on(exchange.find_order_by_client_order_id("BTC-PERP".into(), id.into()))
            .unwrap();
        assert_eq!(
            routes.count("info:orderStatus") - before,
            usize::from(valid),
            "HL_CLOID_AUDIT lookup pattern {id:?}"
        );
    }
    assert_eq!(
        routes.requests.borrow()[0].1["oid"],
        "0x0123456789abcdef0123456789abcdef"
    );
}
