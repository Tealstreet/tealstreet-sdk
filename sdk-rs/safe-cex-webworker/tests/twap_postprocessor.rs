#[path = "support/model_identity.rs"]
mod model_identity;
use safe_cex_types::{ActiveTWAP, StoreData};
use safe_cex_webworker::{
    postprocessors::TwapPostprocessor,
    task_types::{TwapOptions, TwapState, TwapStatus},
};
use serde_json::{Value, json};
fn number(v: &Value) -> f64 {
    let s = v["$num"].as_str().unwrap();
    if s == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap())
    }
}
fn tagged(n: f64) -> Value {
    json!({"$num":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => tagged(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn materialize(v: Value) -> Value {
    match v {
        Value::Object(ref o) if o.contains_key("$num") => {
            let n = number(&v);
            json!(if n.is_finite() { n } else { 0.0 })
        }
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, materialize(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(materialize).collect()),
        v => v,
    }
}
fn inputs(v: &Value) -> Vec<(String, TwapOptions, TwapState)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let o = &i["options"];
            let s = &i["state"];
            let mut options: TwapOptions = serde_json::from_value(materialize(o.clone())).unwrap();
            options.size = number(&o["size"]);
            options.duration = number(&o["duration"]);
            let mut state: TwapState = serde_json::from_value(materialize(s.clone())).unwrap();
            state.orders_sizes = s["ordersSizes"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect();
            (i["id"].as_str().unwrap().to_owned(), options, state)
        })
        .collect()
}
fn active(values: &[ActiveTWAP]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|v| {
                use safe_cex_types::{OrderSide, store_model::{TaskOrigin, TwapStatus, SliceAction}};
                let mut value=json!({
                    "id":v.id,"symbol":v.symbol,"accountName":v.account_name,"activeMarket":v.active_market,
                    "side":match v.side {OrderSide::Buy=>"buy",OrderSide::Sell=>"sell"},
                    "status":match v.status {TwapStatus::Idle=>"idle",TwapStatus::Running=>"running",TwapStatus::Paused=>"paused",TwapStatus::Completed=>"completed"},
                });
                if let Some(origin)=v.origin {value["origin"]=json!(match origin {TaskOrigin::Web=>"web",TaskOrigin::Cli=>"cli",TaskOrigin::Webhook=>"webhook",TaskOrigin::Automation=>"automation"});}
                for (key,action) in [("timeoutAction",v.timeout_action),("failAction",v.fail_action)] {
                    if let Some(action)=action {value[key]=json!(match action {SliceAction::Market=>"market",SliceAction::Next=>"next"});}
                }
                if let Some(cleanup)=v.mark_for_cleanup {value["markForCleanup"]=json!(cleanup);}
                value["reduceOnly"] = json!(v.reduce_only);
                value["useLimitOrders"] = json!(v.use_limit_orders);
                value["useChaserOrders"] = json!(v.use_chaser_orders);
                for (k, n) in [
                    ("size", v.size),
                    ("duration", v.duration),
                    ("ordersCount", v.orders_count),
                    ("chaserDelay", v.chaser_delay),
                    ("randomnessPercentage", v.randomness_percentage),
                    ("ordersExecuted", v.orders_executed),
                    ("nextOrderAt", v.next_order_at),
                ] {
                    value[k] = tagged(n);
                }
                for (k, n) in [
                    ("timeoutPct", v.timeout_pct),
                    ("createdAt", v.created_at),
                    ("startedAt", v.started_at),
                ] {
                    if let Some(n) = n {
                        value[k] = tagged(n);
                    }
                }
                value["ordersSizes"] =
                    Value::Array(v.orders_sizes.iter().copied().map(tagged).collect());
                encode(value)
            })
            .collect(),
    )
}
fn process(store: &mut StoreData, input: &[(String, TwapOptions, TwapState)]) {
    let view: Vec<_> = input.iter().map(|(id, o, s)| (id.as_str(), o, s)).collect();
    TwapPostprocessor.process(store, "target", &view);
}
#[test]
fn projection_matches_live_manager_filter_spread_order_and_aliases() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/twap_postprocessor_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let identities = data["marketIdentity"].as_array().unwrap();
    assert_eq!(identities.len(), 26, "TWAP-PROJECTION market cases execute");
    for case in identities {
        let input = vec![(
            "identity-market".to_owned(),
            serde_json::from_value(case["options"].clone()).unwrap(),
            serde_json::from_value(case["state"].clone()).unwrap(),
        )];
        let mut store = safe_cex_store::store::DEFAULT_STORE.clone();
        process(&mut store, &input);
        assert_eq!(store.active_twaps.len(), 1);
        assert_eq!(
            model_identity::market(&store.active_twaps[0].active_market),
            case["expected"],
            "TWAP-PROJECTION native market identity {}",
            case["path"]
        );
    }
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 432);
    for (index, row) in rows.iter().enumerate() {
        let mut input = inputs(&row["input"]);
        let mut store = safe_cex_store::store::DEFAULT_STORE.clone();
        process(&mut store, &input);
        assert_eq!(
            active(&store.active_twaps),
            row["expected"],
            "TWAP-PROJECTION row {index}"
        );
        input[3].2.orders_sizes.push(77.0);
        input[3].2.active_market.base = "from-state".into();
        let held = store
            .active_twaps
            .iter_mut()
            .find(|t| t.id == "constructor")
            .unwrap();
        assert_eq!(row["alias"]["orders"][3], tagged(77.0));
        assert_eq!(row["alias"]["base"], "from-state");
        assert_eq!(
            held.orders_sizes.len(),
            3,
            "SNAPSHOT state-to-output array remains frozen"
        );
        assert_eq!(
            held.active_market.base, "BTC",
            "SNAPSHOT state-to-output market remains frozen"
        );
        held.active_market.quote = "from-output".into();
        assert_eq!(row["alias"]["quote"], "from-output");
        assert_eq!(
            input[3].2.active_market.quote, "USD",
            "SNAPSHOT output-to-state market independent"
        );
        input[0].1.account_name = "different".into();
        input[3].2.status = TwapStatus::Completed;
        process(&mut store, &input);
        assert_eq!(
            active(&store.active_twaps),
            row["cleared"],
            "TWAP-CLEAR row {index}"
        );
    }
    assert_eq!(
        data["emptyProperty"],
        json!({"own":true,"value":"NaN"}),
        "ts-bug: named array property survives source projection"
    );
    let mut empty = inputs(&rows[0]["input"]);
    empty[3].2.orders_sizes.clear();
    let mut store = safe_cex_store::store::DEFAULT_STORE.clone();
    process(&mut store, &empty);
    assert_eq!(
        serde_json::to_value(&store.active_twaps.last().unwrap().orders_sizes).unwrap(),
        json!([]),
        "FINDING typed Vec has no named -1 output property"
    );
    assert_eq!(
        data["explicitUndefined"],
        json!({"own":true,"type":"undefined","serializedOwn":false})
    );
    let mut undefined = inputs(&rows[0]["input"]);
    undefined[3].1.origin = Some(safe_cex_webworker::task_types::TwapOrigin::Cli);
    undefined[3].2.origin = None;
    process(&mut store, &undefined);
    assert_eq!(
        store.active_twaps.last().unwrap().origin,
        Some(safe_cex_webworker::task_types::TwapOrigin::Cli),
        "FINDING optional state origin cannot distinguish absent from own undefined"
    );
    assert_eq!(data["loneSurrogate"]["codeUnit"], 0xd800);
    let text = data["loneSurrogate"]["json"].as_str().unwrap();
    assert!(text.contains("\\ud800"));
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "shared UTF-8 refusal of a valid source identifier"
    );
}

#[test]
fn source_market_mutation_cannot_rewrite_the_captured_input() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/twap_postprocessor_oracle.mjs"
        ))
        .arg("--mutate-market-copy")
        .output()
        .unwrap();
    assert!(!output.status.success(), "source copy mutation must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("SOURCE-TWAP-PROJECTION market copy identity"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
