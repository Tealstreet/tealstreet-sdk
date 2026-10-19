//! Exact non-UTF8 FIX-audit rows through the constructed production Scope.
use super::accepted_read_receivers::terminal::{Extras, Harness, harness, harness_with, options};
use crate::{private_accounts::IncomeHost, rest_account::ReadHost};
use safe_cex_exchange::{
    exchange::Exchange,
    journal::JournalFolder,
    js_runtime::{
        promise::await_js,
        value::{Js, from_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn source() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/non-utf8-audit-source.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn native(value: &Value) -> Js {
    if value.get("undefined") == Some(&Value::Bool(true)) {
        return Js::Undefined;
    }
    if let Some(bits) = value.get("bits").and_then(Value::as_str) {
        return Js::Number(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()));
    }
    match value {
        Value::Array(rows) => Js::array(rows.iter().map(native).collect()),
        Value::Object(row) => {
            Js::object(row.iter().map(|(k, v)| (k.as_str(), native(v))).collect())
        }
        _ => from_json(value),
    }
}
fn assert_native(actual: &Js, expected: &Value, path: &str) {
    if expected.get("undefined") == Some(&Value::Bool(true)) {
        assert!(actual.is_undefined(), "{path}: own undefined");
    } else if let Some(bits) = expected.get("bits").and_then(Value::as_str) {
        let n = actual
            .as_f64()
            .unwrap_or_else(|| panic!("{path}: native number"));
        let observed = if n.is_nan() {
            f64::NAN.to_bits()
        } else {
            n.to_bits()
        };
        assert_eq!(format!("{observed:016x}"), bits, "{path}: native bits");
    } else if let Some(rows) = expected.as_array() {
        assert_eq!(
            actual.get("length").unwrap().as_f64(),
            Some(rows.len() as f64),
            "{path}: cardinality"
        );
        for (i, row) in rows.iter().enumerate() {
            assert_native(
                &actual.get(&i.to_string()).unwrap(),
                row,
                &format!("{path}[{i}]"),
            );
        }
    } else if let Some(row) = expected.as_object() {
        for (key, value) in row {
            assert!(actual.has_own(key), "{path}.{key}: source own field");
            assert_native(&actual.get(key).unwrap(), value, &format!("{path}.{key}"));
        }
    } else {
        assert!(
            actual.strict_equals(&from_json(expected)),
            "{path}: exact scalar"
        );
    }
}
fn settle(h: &Harness, promise: Js) -> Result<Js, ExchangeError> {
    let runtime: Rc<dyn ExchangeRuntime> = h.runtime.clone();
    h.drive(Box::pin(async move {
        await_js(&runtime, promise)
            .await
            .map_err(|e| e.to_exchange_error())
    }))
}

#[test]
fn six_read_entries_start_without_polling_and_finish_exactly_once() {
    for (venue, member, verb, path, body, args) in [
        (
            ExchangeName::Krakenspot,
            "fetchOHLCV",
            "GET",
            "/0/public/OHLC",
            json!({"error":[],"result":{}}),
            vec![from_json(&json!({"symbol":"BTC/USD","interval":"1m"}))],
        ),
        (
            ExchangeName::Krakenspot,
            "fetchBalance",
            "POST",
            "/0/private/Balance",
            json!({"error":[],"result":{"ZUSD":"13.25"}}),
            vec![],
        ),
        (
            ExchangeName::Kraken,
            "fetchTickers",
            "GET",
            "/v3/tickers",
            json!({"result":"success","tickers":[]}),
            vec![],
        ),
        (
            ExchangeName::Kraken,
            "fetchOrders",
            "GET",
            "/v3/openorders",
            json!({"result":"success","openOrders":[]}),
            vec![],
        ),
        (
            ExchangeName::Kraken,
            "fetchOHLCV",
            "GET",
            "https://futures.kraken.com/api/charts/v1/trade/PF_XBTUSD/1m",
            json!({"candles":[]}),
            vec![from_json(
                &json!({"symbol":"PF_XBTUSD","interval":"1m","from":1000,"to":120000}),
            )],
        ),
        (
            ExchangeName::Kraken,
            "validateAccount",
            "GET",
            "/v3/accounts",
            json!({"result":"failure","error":["invalid key"]}),
            vec![],
        ),
    ] {
        let h = harness(venue.clone(), options(), |http| http.ok(verb, path, body));
        h.http.hold_responses.set(true);
        let promise = h.exchange.rt.this.call_method(member, args).unwrap();
        // This runtime queues spawn_local; it never eagerly polls a fixture spawn.
        h.runtime.pump();
        assert_eq!(
            h.http.calls(path).len(),
            1,
            "{member}: HTTP starts with no waiter poll"
        );
        let request = &h.http.calls(path)[0];
        assert_eq!(request.method, verb, "{member}: method");
        if verb == "POST" || member == "fetchOrders" || member == "validateAccount" {
            assert!(
                !request.headers[if venue == ExchangeName::Kraken {
                    "Authent"
                } else {
                    "API-Sign"
                }]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
                "{member}: real signed client"
            );
        }
        if member == "fetchOHLCV" && venue == ExchangeName::Kraken {
            assert_eq!(request.url, format!("{path}?from=1&to=120"));
        }
        h.http.release_responses();
        let result = settle(&h, promise).unwrap();
        if member == "fetchBalance" {
            assert_eq!(result.get("total").unwrap().as_f64(), Some(13.25));
        } else if member == "validateAccount" {
            assert!(
                result
                    .get("error")
                    .unwrap()
                    .to_display()
                    .contains("Invalid API key")
            );
        } else {
            assert_eq!(result.get("length").unwrap().as_f64(), Some(0.0));
        }
        assert_eq!(h.http.calls(path).len(), 1, "{member}: no restart on await");
        assert!(
            h.runtime.reports.borrow().is_empty(),
            "{member}: no hidden runtime failure"
        );
    }
}

#[test]
fn fill_scalar_rows_reach_the_live_mapper_without_the_old_dto_boundary() {
    let source = source();
    let cases = source["fillsCases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let h = harness(ExchangeName::Kraken, options(), |_| {});
        let input = native(&case["rows"]);
        let result = ReadHost::map_fills(&h.exchange.rt, Rc::new(input.clone())).unwrap();
        let actual = Js::array(result.iter().map(|v| (**v).clone()).collect());
        assert_native(&actual, &case["output"], case["label"].as_str().unwrap());
        if case["label"] == "missing" {
            assert!(input.get("0").unwrap().has_own("price"));
            assert!(input.get("0").unwrap().get("price").unwrap().is_undefined());
        }
    }
}

#[test]
fn income_scalar_rows_survive_native_missing_and_nonfinite_cells() {
    let source = source();
    let cases = source["incomeCases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let input = native(&case["rows"]);
        let body = Js::object(vec![("logs", input.clone())]);
        let h = harness(ExchangeName::Kraken, options(), |http| {
            http.route_native("GET", "/history/v3/account-log", 200.0, body)
        });
        let pending = IncomeHost::account_log_page(
            &h.exchange.rt,
            Rc::new(from_json(&json!({"startTime":1,"limit":37}))),
        )
        .unwrap();
        let actual = h
            .drive(Box::pin(async {
                pending.await.map_err(|e| e.to_exchange_error())
            }))
            .unwrap_or_else(|e| panic!("{}: {}", case["label"], e.message));
        assert_native(&actual, &case["output"], case["label"].as_str().unwrap());
        assert_eq!(h.http.calls("/history/v3/account-log").len(), 1);
        assert_eq!(
            h.http.calls("/history/v3/account-log")[0].url,
            "/history/v3/account-log?count=37&since=1"
        );
        if case["label"] == "missing" {
            assert!(input.get("0").unwrap().has_own("realized_pnl"));
        }
        assert!(h.runtime.reports.borrow().is_empty());
    }
}

#[derive(Default)]
struct Folder {
    calls: RefCell<Vec<Value>>,
}
impl JournalFolder for Folder {
    fn build_folded_position_records<'a>(
        &'a self,
        venue: &'a dyn Exchange,
        account: &'a str,
        from: f64,
        to: f64,
        scope: bool,
        settle: Box<dyn Fn(&str) -> Result<String, ExchangeError> + 'a>,
    ) -> ExchangeFuture<'a, Vec<Value>> {
        self.calls.borrow_mut().push(json!({"name":venue.base().name,"account":account,"from":from,"to":to,"scope":scope,"settle":settle("MISSING").unwrap()}));
        Box::pin(async {
            Ok(vec![
                json!({"id":"first","amount":-0.0}),
                json!({"id":"second","amount":3.25}),
            ])
        })
    }
}
#[test]
fn historical_positions_use_real_venue_folder_and_default_window() {
    let folder = Rc::new(Folder::default());
    let h = harness_with(
        ExchangeName::Kraken,
        options(),
        Extras {
            journal: Some(folder.clone()),
            ..Extras::default()
        },
        |_| {},
    );
    let promise = h
        .exchange
        .rt
        .this
        .call_method(
            "fetchHistoricalPositions",
            vec![from_json(&json!({"account":"acct-A"}))],
        )
        .unwrap();
    assert_eq!(
        folder.calls.borrow().len(),
        1,
        "journal entry precedes waiting"
    );
    let actual = settle(&h, promise).unwrap();
    assert_eq!(
        folder.calls.borrow()[0],
        json!({"name":"kraken","account":"acct-A","from":1700000000000.0-30.0*86400000.0,"to":1700000000000.0,"scope":false,"settle":"USDT"})
    );
    assert_native(
        &actual,
        &json!([{"id":"first","amount":{"bits":"8000000000000000"}},{"id":"second","amount":{"bits":"400a000000000000"}}]),
        "journal",
    );
    assert_eq!(folder.calls.borrow().len(), 1);
    assert!(h.http.requests.borrow().is_empty());
}

#[test]
fn audit_entry_prefixes_match_unchanged_typescript_arrows() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/eager-wrapper-source.cjs"
        ))
        .arg("--audit")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["receipts"].as_array().unwrap().len(), 8);
    assert_eq!(value["synchronousHttpPrefixes"], 7);
    assert_eq!(value["detachedContinuation"], true);
    assert_eq!(value["rejectionPrefix"], true);
}
