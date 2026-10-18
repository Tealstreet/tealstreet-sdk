#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::{JsCoercionError, JsRootCollator};
use safe_cex_types::OrderStopTriggerType;
use safe_cex_venue_kraken::helpers;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    sync::OnceLock,
};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/helpers-oracle.cjs"
            ))
            .env("LANG", "en_US.UTF-8")
            .env("LC_ALL", "en_US.UTF-8")
            .output()
            .expect("actual TypeScript helper oracle");
        assert!(
            output.status.success(),
            "actual TypeScript helper oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        eprintln!("module helper capture header: {}", value["header"]);
        assert!(
            value["header"]["node"].as_str().unwrap().starts_with('v'),
            "Node capture version"
        );
        assert!(
            !value["header"]["arch"].as_str().unwrap().is_empty(),
            "CPU capture architecture"
        );
        assert_eq!(value["header"]["nanoid"], "6.0.1", "actual nanoid version");
        cardinality::source_census("helpers", &value);
        value
    })
}
fn optional_raw(value: &Value) -> Option<Value> {
    value.as_str().map(|s| serde_json::from_str(s).unwrap())
}
fn absent() -> Value {
    json!({"undefined":true})
}
#[path = "../../parity/evidence/retained_model_values.rs"]
pub mod retained_model_values;
fn capture(value: &(impl serde::Serialize + ?Sized)) -> Value {
    retained_model_values::observe(
        value,
        |value: f64| json!({"number":format!("{:016x}",if value.is_nan(){0x7ff8000000000000}else{value.to_bits()})}),
    )
}

#[test]
fn trigger_live_oracle() {
    for case in cardinality::cases("helpers", "triggers", oracle()) {
        let v = optional_raw(&case["raw"]);
        let actual = match helpers::parse_trigger_signal(v.as_ref()) {
            Ok(None) => absent(),
            Ok(Some(value)) => json!(match value {
                OrderStopTriggerType::MarkPrice => "mark_price",
                OrderStopTriggerType::IndexPrice => "index_price",
                OrderStopTriggerType::LastPrice => "last_price",
            }),
            Err(e) => json!({"error":e.name,"message":e.message}),
        };
        assert_eq!(
            actual, case["result"],
            "parseTriggerSignal native enum raw={}",
            case["raw"]
        );
    }
}
#[test]
fn sort_live_oracle() {
    let collator = JsRootCollator::try_new().unwrap();
    for case in cardinality::cases("helpers", "sorts", oracle()) {
        let actual = helpers::sort(
            &collator,
            case["left"].as_str().unwrap(),
            case["right"].as_str().unwrap(),
        ) as i8;
        assert_eq!(
            json!(actual),
            case["result"],
            "sort localeCompare left={} right={}",
            case["left"],
            case["right"]
        );
    }
}
#[test]
fn error_live_oracle() {
    for case in cardinality::cases("helpers", "errors", oracle()) {
        let response = optional_raw(&case["responseRaw"]);
        let cause = optional_raw(&case["causeRaw"]);
        let error = helpers::create_kraken_api_error(
            case["message"].as_str().unwrap(),
            response.as_ref(),
            cause.as_ref(),
        );
        let expected = &case["result"];
        assert_eq!(
            json!(error.name),
            expected["name"],
            "createKrakenApiError name"
        );
        assert_eq!(
            json!(error.message),
            expected["message"],
            "createKrakenApiError message"
        );
        assert_eq!(
            error.to_string(),
            case["message"].as_str().unwrap(),
            "CHOSEN message-only Error Display"
        );
        let source_string = if error.message.is_empty() {
            "Error".to_owned()
        } else {
            format!("Error: {}", error.message)
        };
        assert_eq!(
            json!(source_string),
            expected["string"],
            "source Error string representation"
        );
        assert_eq!(
            error
                .response_json_view()
                .as_ref()
                .map(capture)
                .unwrap_or_else(absent),
            expected["response"],
            "createKrakenApiError response native field"
        );
        assert_eq!(
            error
                .metadata
                .original_error
                .as_ref()
                .map(capture)
                .unwrap_or_else(absent),
            expected["cause"],
            "createKrakenApiError cause native field"
        );
        assert_eq!(
            json!(error.response_json_view().is_some()),
            expected["hasResponse"],
            "createKrakenApiError response presence"
        );
        assert_eq!(
            json!(error.metadata.original_error.is_some()),
            expected["hasCause"],
            "createKrakenApiError cause presence"
        );
        assert_eq!(
            expected["responseIdentity"], true,
            "source response identity"
        );
        assert_eq!(expected["causeIdentity"], true, "source cause identity");
    }
}
#[test]
fn client_id_live_oracle() {
    let mut comparisons = 0usize;
    for scenario in cardinality::cases("helpers", "ids", oracle()) {
        let state = Cell::new(0x13579bdf_u32);
        let fail = Cell::new(false);
        let draws = RefCell::new(Vec::new());
        let mut generate = helpers::create_kraken_spot_client_order_id(|buffer| {
            draws.borrow_mut().push(buffer.len());
            if fail.replace(false) {
                return Err(JsCoercionError {
                    name: "Error",
                    message: "entropy failure",
                });
            }
            for byte in buffer {
                let mut x = state.get();
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                state.set(x);
                *byte = (x & 255) as u8;
            }
            Ok(())
        });
        let mut scenario_comparisons = 0usize;
        for i in 0..1320 {
            let case = scenario["cases"]
                .as_array()
                .unwrap()
                .get(i)
                .expect("KRAKEN-CARDINALITY ID expected/native index coverage");
            if i as i64 == scenario["failAt"].as_i64().unwrap() {
                fail.set(true);
            }
            draws.borrow_mut().clear();
            let actual = match generate() {
                Ok(id) => json!(id),
                Err(e) => json!({"error":e.name,"message":e.message}),
            };
            assert_eq!(
                actual, case["result"],
                "createKrakenSpotClientOrderId actual nanoid output call={i}"
            );
            assert_eq!(
                json!(*draws.borrow()),
                case["draws"],
                "createKrakenSpotClientOrderId entropy requests call={i}"
            );
            scenario_comparisons += 1;
            comparisons += 1;
        }
        assert_eq!(
            scenario_comparisons, 1320,
            "KRAKEN-CARDINALITY ID per-scenario native comparisons"
        );
    }
    assert_eq!(
        comparisons, 2640,
        "KRAKEN-CARDINALITY ID total native comparisons"
    );
}

#[test]
fn chosen_helper_boundaries() {
    let boundary = &oracle()["boundaries"];
    assert_eq!(boundary["trigger"], absent(), "source surrogate trigger");
    assert_eq!(boundary["sort"], 0, "source surrogate sort");
    assert_eq!(
        boundary["errorMessageUnits"],
        json!([55296]),
        "source surrogate error message"
    );
    assert!(
        serde_json::from_str::<String>(r#""\ud800""#).is_err(),
        "shared UTF8 helper refusal"
    );
    let mut response = json!({"status":403});
    let mut cause = json!({"code":"before"});
    let error = helpers::create_kraken_api_error("alias", Some(&response), Some(&cause));
    response["status"] = json!(429);
    cause["code"] = json!("after");
    assert_eq!(
        boundary["alias"]["response"], response,
        "source live response alias"
    );
    assert_eq!(boundary["alias"]["cause"], cause, "source live cause alias");
    assert_eq!(
        error.response_json_view(),
        Some(json!({"status":403})),
        "CHOSEN response snapshot"
    );
    assert_eq!(
        error.metadata.original_error,
        Some(json!({"code":"before"})),
        "CHOSEN cause snapshot"
    );
    assert_eq!(
        boundary["sourceOverrideSize"], 1,
        "source accepts custom id size"
    );
    let mut generate = helpers::create_kraken_spot_client_order_id(|buffer| {
        buffer.fill(1);
        Ok(())
    });
    assert_eq!(
        generate().unwrap().len(),
        32,
        "CHOSEN fixed venue id length"
    );
}
