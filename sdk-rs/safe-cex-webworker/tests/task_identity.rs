#[path = "support/model_identity.rs"]
mod model_identity;
use model_identity::market;
use safe_cex_webworker::task_types::*;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

macro_rules! fields {
    ($v:expr; $($wire:literal => $field:ident),* $(,)?) => {json!({$($wire: &$v.$field),*})};
}
fn side(v: safe_cex_types::OrderSide) -> &'static str {
    match v {
        safe_cex_types::OrderSide::Buy => "buy",
        safe_cex_types::OrderSide::Sell => "sell",
    }
}
fn origin(v: ChaserOrigin) -> &'static str {
    match v {
        ChaserOrigin::Web => "web",
        ChaserOrigin::Cli => "cli",
        ChaserOrigin::Webhook => "webhook",
        ChaserOrigin::Automation => "automation",
    }
}
fn action(v: TwapTimeoutAction) -> &'static str {
    match v {
        TwapTimeoutAction::Market => "market",
        TwapTimeoutAction::Next => "next",
    }
}
fn terminate(v: &ChaserTerminate) -> Value {
    json!({"type":match v.kind {ChaserTerminateKind::Price=>"price",ChaserTerminateKind::Percent=>"percent",ChaserTerminateKind::Usd=>"usd"},"value":v.value,"anchorPrice":v.anchor_price})
}
fn chaser(v: &ChaserOrderOptions) -> Value {
    let mut out = fields!(v; "symbol"=>symbol,"size"=>size,"side"=>side,"maxAttempts"=>max_attempts,
        "reduceOnly"=>reduce_only,"delay"=>delay,"origin"=>origin,"accountName"=>account_name,"clientId"=>client_id);
    out["side"] = json!(side(v.side));
    out["origin"] = json!(v.origin.map(origin));
    out["terminate"] = terminate(v.terminate.as_ref().unwrap());
    out
}
fn complete(v: &ChaserCompleteEvent) -> Value {
    let mut out = fields!(v; "id"=>id,"clientId"=>client_id,"symbol"=>symbol,"side"=>side,
        "reduceOnly"=>reduce_only,"size"=>size,"filled"=>filled,"stopReason"=>stop_reason);
    out["side"] = json!(side(v.side));
    out
}
fn state(v: &ChaserOrderState) -> Value {
    fields!(v; "attempts"=>attempts,"isStopped"=>is_stopped,"stopReason"=>stop_reason,"currentOrdersIds"=>current_orders_ids)
}
fn twap(v: &TwapOptions) -> Value {
    let mut out = fields!(v; "accountName"=>account_name,"size"=>size,"duration"=>duration,"ordersCount"=>orders_count,
        "side"=>side,"chaserDelay"=>chaser_delay,"reduceOnly"=>reduce_only,"randomnessPercentage"=>randomness_percentage,
        "useLimitOrders"=>use_limit_orders,"useChaserOrders"=>use_chaser_orders,"timeoutPct"=>timeout_pct,
        "timeoutAction"=>timeout_action,"failAction"=>fail_action,"origin"=>origin);
    out["side"] = json!(side(v.side));
    out["origin"] = json!(v.origin.map(origin));
    out["activeMarket"] = market(&v.active_market);
    out["timeoutAction"] = json!(v.timeout_action.map(action));
    out["failAction"] = json!(v.fail_action.map(action));
    out
}
fn twap_state(v: &TwapState) -> Value {
    let mut out = fields!(v; "accountName"=>account_name,"size"=>size,"ordersSizes"=>orders_sizes,"side"=>side,
        "status"=>status,"ordersCount"=>orders_count,"ordersExecuted"=>orders_executed,"nextOrderAt"=>next_order_at,
        "markForCleanup"=>mark_for_cleanup,"origin"=>origin,"createdAt"=>created_at,"startedAt"=>started_at);
    out["side"] = json!(side(v.side));
    out["origin"] = json!(v.origin.map(origin));
    out["activeMarket"] = market(&v.active_market);
    out["status"] = json!(match v.status {
        TwapStatus::Idle => "idle",
        TwapStatus::Running => "running",
        TwapStatus::Paused => "paused",
        TwapStatus::Completed => "completed",
    });
    out
}
#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}
fn probe<T: DeserializeOwned>(name: &str, schema: &Value, read: impl Fn(&T) -> Value) {
    let (case_count, leaf_count, collection) = match name {
        "ChaserTerminate" => (5, 3, None),
        "ChaserOrderOptions" => (19, 12, None),
        "ChaserOrderState" => (5, 4, Some("/currentOrdersIds")),
        "TwapOptions" => (59, 43, None),
        "TwapState" => (56, 41, Some("/ordersSizes")),
        _ => panic!("TASK-CARDINALITY unknown record"),
    };
    let cases = schema["cases"].as_array().unwrap();
    let leaves = schema["leaves"].as_array().unwrap();
    assert_eq!(cases.len(), case_count, "TASK-CARDINALITY cases {name}");
    assert_eq!(leaves.len(), leaf_count, "TASK-CARDINALITY leaves {name}");
    let distinct: std::collections::BTreeSet<_> =
        leaves.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(
        distinct.len(),
        leaf_count,
        "TASK-CARDINALITY distinct leaves {name}"
    );
    let mut comparisons = 0;
    for index in 0..case_count {
        let case = cases
            .get(index)
            .expect("TASK-CARDINALITY missing fixed case");
        let input = &case["input"];
        let raw = serde_json::to_string(input).unwrap();
        let outer: Envelope<T> = serde_json::from_value(json!({"data":input})).unwrap();
        let decoded: [T; 4] = [
            serde_json::from_str(&raw).unwrap(),
            serde_json::from_value(input.clone()).unwrap(),
            T::deserialize(input).unwrap(),
            outer.data,
        ];
        for value in decoded {
            let native = read(&value);
            if let Some(path) = collection {
                assert_eq!(
                    input.pointer(path).unwrap().as_array().unwrap().len(),
                    1,
                    "TASK-CARDINALITY source collection {name}{path}"
                );
                assert_eq!(
                    native.pointer(path).unwrap().as_array().unwrap().len(),
                    1,
                    "TASK-CARDINALITY native collection {name}{path}"
                );
            }
            for index in 0..leaf_count {
                let path = leaves
                    .get(index)
                    .expect("TASK-CARDINALITY missing fixed leaf")
                    .as_str()
                    .unwrap();
                assert!(
                    input.pointer(path).is_some(),
                    "TASK-IDENTITY absent source field {name}{path}"
                );
                assert_eq!(
                    native.pointer(path),
                    input.pointer(path),
                    "TASK-IDENTITY native field {name}{path}; varied {}",
                    case["path"]
                );
                comparisons += 1;
            }
        }
    }
    assert_eq!(
        comparisons,
        case_count * 4 * leaf_count,
        "TASK-CARDINALITY comparisons {name}"
    );
}
#[test]
fn task_native_field_identity_survives_outer_decode() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/task_identity_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source: Value = serde_json::from_slice(&result.stdout).unwrap();
    let r = &source["records"];
    assert_eq!(
        r.as_object().unwrap().len(),
        6,
        "TASK-CARDINALITY record total"
    );
    probe("ChaserTerminate", &r["ChaserTerminate"], terminate);
    probe("ChaserOrderOptions", &r["ChaserOrderOptions"], chaser);
    complete_output(&r["ChaserCompleteEvent"]);
    probe("ChaserOrderState", &r["ChaserOrderState"], state);
    probe("TwapOptions", &r["TwapOptions"], twap);
    probe("TwapState", &r["TwapState"], twap_state);
}

fn complete_output(schema: &Value) {
    use safe_cex_exchange::js_runtime::value::{from_typed, to_json};
    let cases = schema["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    assert_eq!(schema["leaves"].as_array().unwrap().len(), 8);
    for case in cases {
        let input = &case["input"];
        let event = ChaserCompleteEvent {
            id: input["id"].as_str().unwrap().into(),
            client_id: input["clientId"].as_str().map(str::to_owned),
            symbol: input["symbol"].as_str().unwrap().into(),
            side: match input["side"].as_str().unwrap() {
                "buy" => safe_cex_types::OrderSide::Buy,
                "sell" => safe_cex_types::OrderSide::Sell,
                _ => panic!("source side"),
            },
            reduce_only: input["reduceOnly"].as_bool().unwrap(),
            size: input["size"].as_f64().unwrap(),
            filled: input["filled"].as_f64().unwrap(),
            stop_reason: input["stopReason"].as_str().unwrap().into(),
        };
        assert_eq!(
            complete(&event),
            *input,
            "COMPLETION original field identity case"
        );
        assert_eq!(
            to_json(&from_typed(&event).unwrap()).unwrap(),
            *input,
            "COMPLETION native output preserves all original field identities"
        );
    }
}
