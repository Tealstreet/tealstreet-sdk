//! Source-only pagination and concurrency witnesses; no completed order read claim.
#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn source(cases: &[Value], setting: Option<(&str, &str)>) -> std::process::Output {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/spot-orders.cjs"
    ));
    if let Some((key, value)) = setting {
        command.env(key, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn capture(cases: &[Value], expected_cases: usize) -> Value {
    assert_eq!(
        cases.len(),
        expected_cases,
        "KUCOIN-CARDINALITY orders reference input total"
    );
    let output = source(cases, None);
    assert!(
        output.status.success(),
        "live Spot orders source capture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captured: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        captured["cases"].as_array().unwrap().len(),
        expected_cases,
        "KUCOIN-CARDINALITY orders reference source case count"
    );
    captured
}

fn assert_snapshot_counts(capture: &Value, phase: &str, counts: &[(usize, usize)]) {
    let mut compared_rows = 0;
    for (index, &(calls, rows)) in counts.iter().enumerate() {
        let snapshot = &capture["cases"][index][phase];
        assert_eq!(
            snapshot["calls"].as_array().unwrap().len(),
            calls,
            "KUCOIN-CARDINALITY orders {phase} case {index} calls"
        );
        for field in ["wire", "bits", "types"] {
            assert_eq!(
                snapshot[field].as_array().unwrap().len(),
                rows,
                "KUCOIN-CARDINALITY orders {phase} case {index} {field}"
            );
        }
        for row in 0..rows {
            assert_eq!(
                snapshot["bits"][row].as_array().unwrap().len(),
                5,
                "KUCOIN-CARDINALITY orders {phase} case {index} numeric fields"
            );
            compared_rows += 1;
        }
    }
    assert_eq!(
        compared_rows,
        counts.iter().map(|(_, rows)| rows).sum::<usize>(),
        "KUCOIN-CARDINALITY orders {phase} compared rows"
    );
}

fn row(id: &str) -> Value {
    json!({"id":id,"orderId":"unused-order-id","symbol":"BTC-USDT","type":"limit","side":"sell","status":"open","price":"11.5tail","size":"7","dealSize":"2","remainSize":"5","timeInForce":"GTC","createdAt":"1700000000123","marker":"before"})
}
fn page(items: Vec<Value>, total: Value) -> Value {
    let mut data = json!({"code":"200000","data":{"items":items}});
    if !total.is_null() {
        data["data"]["totalPage"] = total;
    }
    json!({"data":data})
}
fn stored(id: &str) -> Value {
    json!({"id":id,"symbol":"STORED-USDT","type":"market","side":"buy","status":"closed","price":13,"amount":17,"filled":3,"remaining":14,"lastUpdate":1700000000777_i64,"reduceOnly":false})
}
#[test]
fn live_spot_order_pagination_guard_and_partial_results() {
    let full: Vec<Value> = (0..50).map(|i| row(&format!("full-{i}"))).collect();
    let mut cases = vec![];
    for total in [
        Value::Null,
        json!("bad"),
        json!(0),
        json!(-1),
        json!("1tail"),
    ] {
        cases.push(json!({"symbols":["BTC-USDT"],"responses":[page(full.clone(),total)]}));
    }
    cases.push(json!({"symbols":["BTC-USDT"],"responses":[page(vec![row("first")],json!(2)),{"data":{"code":"bad","msg":"page rejected"}}]}));
    let repeated: Vec<Value> = (0..100)
        .map(|i| page(vec![row(&format!("cap-{i}"))], json!(1000)))
        .collect();
    cases.push(json!({"symbols":["BTC-USDT"],"responses":repeated}));
    let output = capture(&cases, 7);
    assert_eq!(
        output["limits"],
        json!({"pageSize":50,"maxPages":100}),
        "live Spot pagination constants"
    );
    for value in &output["cases"].as_array().unwrap()[..5] {
        assert_eq!(
            value["atReturn"]["calls"].as_array().unwrap().len(),
            1,
            "live Spot pagination retained totalPage guard"
        );
        assert_eq!(
            value["atReturn"]["wire"].as_array().unwrap().len(),
            50,
            "live Spot pagination full first page retained"
        );
    }
    assert_eq!(
        output["cases"][6]["atReturn"]["calls"]
            .as_array()
            .unwrap()
            .len(),
        100,
        "KUCOIN-CARDINALITY pagination capped calls"
    );
    let mut comparisons = 0;
    for index in 0..100 {
        let call = &output["cases"][6]["atReturn"]["calls"][index];
        assert_eq!(
            call["params"],
            json!({"symbol":"BTC-USDT","pageNum":index+1,"pageSize":50}),
            "live Spot pagination request identity"
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 100,
        "KUCOIN-CARDINALITY pagination compared calls"
    );
    let partial = &output["cases"][5]["atReturn"];
    assert_eq!(
        partial["wire"][0]["id"], "first",
        "live Spot partial page result"
    );
    assert_eq!(partial["errors"], json!(["page rejected"]));
    assert_eq!(partial["storeAlias"], false);
    assert_eq!(
        output["cases"][6]["atReturn"]["calls"]
            .as_array()
            .unwrap()
            .len(),
        100,
        "live Spot page cap"
    );
    assert_snapshot_counts(
        &output,
        "atReturn",
        &[
            (1, 50),
            (1, 50),
            (1, 50),
            (1, 50),
            (1, 50),
            (2, 1),
            (100, 100),
        ],
    );
    assert_snapshot_counts(
        &output,
        "after",
        &[
            (1, 50),
            (1, 50),
            (1, 50),
            (1, 50),
            (1, 50),
            (2, 1),
            (100, 100),
        ],
    );
}
#[test]
fn live_spot_order_concurrency_return_and_sibling_effects() {
    let mut a = page(vec![row("A-order")], json!(1));
    a["defer"] = json!(true);
    let mut b = page(vec![row("B-order")], json!(1));
    b["defer"] = json!(true);
    let success = json!({"symbols":["A","B"],"responses":[a,b],"releaseBeforeReturn":[1,0]});
    let rejection = json!({"symbols":["A","B"],"existing":[stored("old")],"responses":[{"failure":{"message":"first"},"storeOrders":[stored("during-rejection")]},{"defer":true,"data":{"code":"200000","data":{"items":[],"totalPage":2}}},{"data":{"code":"bad","msg":"sibling-after-return"}}],"releaseAfterReturn":[1],"afterCalls":3,"afterErrors":2,"alias":true,"replaceOrders":[stored("replacement")]});
    let output = capture(&[success, rejection], 2);
    let success_trace = json!([
        ["request", 0],
        ["request", 1],
        ["release", 1],
        ["respond", 1],
        ["release", 0],
        ["respond", 0],
        ["return"]
    ]);
    for phase in ["atReturn", "after"] {
        assert_eq!(
            output["cases"][0][phase]["events"], success_trace,
            "KUCOIN-TRACE success {phase} event identity"
        );
    }
    assert_eq!(
        output["cases"][1]["atReturn"]["events"],
        json!([
            ["request", 0],
            ["reject", 0],
            ["request", 1],
            ["emit", "first"],
            ["return"]
        ]),
        "KUCOIN-TRACE bad-code atReturn event identity"
    );
    assert_eq!(
        output["cases"][1]["after"]["events"],
        json!([
            ["request", 0],
            ["reject", 0],
            ["request", 1],
            ["emit", "first"],
            ["return"],
            ["release", 1],
            ["respond", 1],
            ["request", 2],
            ["respond", 2],
            ["emit", "sibling-after-return"]
        ]),
        "KUCOIN-TRACE bad-code after event identity"
    );
    let first = &output["cases"][0]["atReturn"];
    assert_eq!(
        first["wire"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["A-order", "B-order"],
        "live Spot orders flatten follows symbol order"
    );
    assert_eq!(first["calls"][0]["params"]["symbol"], "A");
    assert_eq!(first["calls"][1]["params"]["symbol"], "B");
    let failed = &output["cases"][1];
    assert_eq!(
        failed["atReturn"]["wire"][0]["id"], "during-rejection",
        "live Spot orders fallback observes current Store"
    );
    assert_eq!(
        failed["atReturn"]["storeAlias"], true,
        "live Spot orders fallback retains array identity"
    );
    assert_eq!(
        failed["atReturn"]["calls"].as_array().unwrap().len(),
        2,
        "live Spot orders launches both symbols before fallback"
    );
    assert_eq!(failed["atReturn"]["errors"], json!(["first"]));
    assert_eq!(
        failed["after"]["calls"][2]["params"],
        json!({"symbol":"B","pageNum":2,"pageSize":50}),
        "live Spot orders sibling sends after outer return"
    );
    assert_eq!(
        failed["after"]["errors"],
        json!(["first", "sibling-after-return"]),
        "live Spot orders sibling emits after outer return"
    );
    let mut retained = stored("during-rejection");
    retained["price"] = json!(91);
    assert_eq!(
        failed["mutatedStore"],
        json!([retained.clone()]),
        "KUCOIN-ALIAS mutated Store complete record identity"
    );
    assert_eq!(
        failed["afterReplacement"]["result"],
        json!([retained]),
        "KUCOIN-ALIAS retained result complete record identity"
    );
    assert_eq!(
        failed["afterReplacement"]["store"],
        json!([stored("replacement")]),
        "KUCOIN-ALIAS replacement Store complete record identity"
    );
    assert_eq!(failed["mutatedStore"][0]["price"], 91);
    assert_eq!(
        failed["afterReplacement"]["result"][0]["id"],
        "during-rejection"
    );
    assert_eq!(failed["afterReplacement"]["store"][0]["id"], "replacement");
    assert_eq!(failed["afterReplacement"]["sameArray"], false);
    assert_snapshot_counts(&output, "atReturn", &[(2, 2), (2, 1)]);
    assert_snapshot_counts(&output, "after", &[(2, 2), (3, 1)]);
}
#[test]
fn live_spot_order_discovery_preserves_nonstring_symbols() {
    let symbols = json!([null,7,{"wire":"object"},"BTC-USDT","BTC-USDT"]);
    let mut responses = vec![json!({"data":{"code":"200000","data":{"symbols":symbols}}})];
    responses.extend((0..5).map(|_| page(vec![], json!(1))));
    let output = capture(
        &[
            json!({"responses":responses}),
            json!({"symbols":[],"responses":responses}),
        ],
        2,
    );
    assert_eq!(
        symbols.as_array().unwrap().len(),
        5,
        "KUCOIN-CARDINALITY discovery input symbols"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        let value = &output["cases"][index];
        let calls = value["atReturn"]["calls"].as_array().unwrap();
        assert_eq!(
            calls.len(),
            6,
            "live Spot orders discovery and duplicate fanout"
        );
        for symbol_index in 0..5 {
            let call = &calls[symbol_index + 1];
            let symbol = &symbols[symbol_index];
            assert_eq!(
                &call["params"]["symbol"], symbol,
                "live Spot discovery symbol identity"
            );
            assert_eq!(call["keys"], json!(["symbol", "pageNum", "pageSize"]));
            comparisons += 1;
        }
    }
    assert_eq!(
        comparisons, 10,
        "KUCOIN-CARDINALITY discovery compared calls"
    );
    assert_snapshot_counts(&output, "atReturn", &[(6, 0), (6, 0)]);
    assert_snapshot_counts(&output, "after", &[(6, 0), (6, 0)]);
}
#[test]
fn live_spot_order_later_pages_rejections_and_info_alias() {
    let full: Vec<Value> = (0..50).map(|i| row(&format!("later-{i}"))).collect();
    let cases = vec![
        json!({"symbols":["A"],"responses":[page(vec![row("first")],json!(3)),page(full.clone(),Value::Null),page(full,Value::Null)]}),
        json!({"symbols":["A"],"responses":[page(vec![row("first")],json!(3)),page(vec![row("second")],json!(1))]}),
        json!({"symbols":["A"],"existing":[stored("fallback")],"responses":[page(vec![row("first")],json!({"toString":null}))]}),
        json!({"symbols":["A"],"existing":[stored("fallback")],"responses":[page(vec![row("discarded")],json!(2)),{"failure":{"message":"second page transport"}}]}),
        json!({"symbols":["A","B"],"existing":[stored("fallback")],"responses":[{"defer":true,"failure":{"message":"A later"}},{"defer":true,"failure":{"message":"B first"}}],"releaseBeforeReturn":[1],"releaseAfterReturn":[0],"afterCalls":2,"afterErrors":1}),
        json!({"symbols":["A"],"responses":[page(vec![row("info")],json!(1))],"infoAlias":true}),
    ];
    let output = capture(&cases, 6);
    assert_eq!(
        output["cases"][0]["atReturn"]["calls"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "live Spot retained prior totalPage"
    );
    assert_eq!(
        output["cases"][0]["atReturn"]["wire"]
            .as_array()
            .unwrap()
            .len(),
        101
    );
    assert_eq!(
        output["cases"][1]["atReturn"]["calls"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "live Spot changing totalPage"
    );
    for index in [2, 3, 4] {
        assert_eq!(
            output["cases"][index]["atReturn"]["wire"][0]["id"], "fallback",
            "live Spot rejected page discards partial result"
        );
    }
    assert_eq!(
        output["cases"][2]["atReturn"]["errors"],
        json!(["Cannot convert object to primitive value"])
    );
    assert_eq!(
        output["cases"][3]["atReturn"]["errors"],
        json!(["second page transport"])
    );
    assert_eq!(
        output["cases"][4]["after"]["errors"],
        json!(["B first"]),
        "live Spot first rejection identity"
    );
    assert_eq!(
        output["cases"][4]["atReturn"]["events"],
        json!([
            ["request", 0],
            ["request", 1],
            ["release", 1],
            ["reject", 1],
            ["emit", "B first"],
            ["return"]
        ]),
        "KUCOIN-TRACE transport atReturn event identity"
    );
    assert_eq!(
        output["cases"][4]["after"]["events"],
        json!([
            ["request", 0],
            ["request", 1],
            ["release", 1],
            ["reject", 1],
            ["emit", "B first"],
            ["return"],
            ["release", 0],
            ["reject", 0]
        ]),
        "KUCOIN-TRACE transport after event identity"
    );
    let events = output["cases"][4]["after"]["events"].as_array().unwrap();
    let returned = events
        .iter()
        .position(|event| event == &json!(["return"]))
        .unwrap();
    let rejected = events
        .iter()
        .position(|event| event == &json!(["reject", 0]))
        .unwrap();
    assert!(
        rejected > returned,
        "live Spot later rejection settles after outer return"
    );
    assert_eq!(output["cases"][5]["infoBefore"]["marker"], "before");
    assert_eq!(output["cases"][5]["infoAfter"]["result"]["marker"], "after");
    assert_eq!(output["cases"][5]["infoAfter"]["wire"]["marker"], "after");
    assert_eq!(
        output["cases"][5]["infoAfter"]["same"], true,
        "live Spot mapped info identity"
    );
    assert_snapshot_counts(
        &output,
        "atReturn",
        &[(3, 101), (2, 2), (1, 1), (2, 1), (2, 1), (1, 1)],
    );
    assert_snapshot_counts(
        &output,
        "after",
        &[(3, 101), (2, 2), (1, 1), (2, 1), (2, 1), (1, 1)],
    );
}

#[test]
fn live_spot_order_field_identity_and_raw_responses() {
    let output = capture(
        &[
            json!({"symbols":["A"],"responses":[page(vec![row("identity")],json!(1))]}),
            json!({"symbols":["A"],"responses":[{"raw":""}]}),
            json!({"symbols":["A"],"responses":[{"raw":"not-json"}]}),
            json!({"symbols":["A"],"responses":[{"failure":{"message":"transport fallback","response":{"raw":"not-json"}}}]}),
            json!({"symbols":["A"],"responses":[{"raw":page(vec![row("raw-json")],json!(1))["data"].to_string()}]}),
            json!({"symbols":["A"],"responses":[{"failure":{"message":"transport hidden","response":{"raw":"{\"msg\":\"body wins\"}"}}}]}),
            json!({"symbols":["A"],"responses":[page(vec![json!({"id":"clock","symbol":"ETH-USDT","size":"17","dealSize":"3","remainSize":"14"})],json!(1))]}),
        ],
        7,
    );
    let value = &output["cases"][0]["atReturn"];
    assert_eq!(
        value["clockCalls"], 0,
        "live Spot explicit timestamp bypasses clock"
    );
    for (index, (field, expected)) in [
        ("price", 11.5_f64),
        ("amount", 7.0),
        ("filled", 2.0),
        ("remaining", 5.0),
        ("lastUpdate", 1700000000123.0),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            value["bits"][0][index],
            numeric_bits::bits(expected),
            "live Spot order numeric identity: {field}"
        );
    }
    for (field, expected) in [
        ("id", "identity"),
        ("symbol", "BTC-USDT"),
        ("status", "open"),
        ("type", "limit"),
        ("side", "sell"),
        ("timeInForce", "GoodTillCancel"),
    ] {
        assert_eq!(
            value["wire"][0][field], expected,
            "live Spot order string identity: {field}"
        );
        assert_eq!(
            value["types"][0][field], "string",
            "live Spot order field type: {field}"
        );
    }
    for index in [1, 2] {
        assert_eq!(
            output["cases"][index]["atReturn"]["wire"],
            json!([]),
            "live Spot raw Axios empty result"
        );
        assert_eq!(
            output["cases"][index]["atReturn"]["errors"],
            json!(["Failed to fetch KuCoin Spot orders"]),
            "live Spot raw Axios diagnostic"
        );
    }
    assert_eq!(
        output["cases"][3]["atReturn"]["errors"],
        json!(["transport fallback"]),
        "live Spot raw Axios failure message"
    );
    assert_eq!(
        output["cases"][4]["atReturn"]["wire"][0]["id"], "raw-json",
        "live Spot raw JSON success"
    );
    assert_eq!(
        output["cases"][5]["atReturn"]["errors"],
        json!(["body wins"]),
        "live Spot raw failure body precedence"
    );
    assert_eq!(
        output["cases"][6]["atReturn"]["clockCalls"], 1,
        "live Spot missing timestamp clock calls"
    );
    assert_eq!(
        output["cases"][6]["atReturn"]["bits"][0][4],
        format!("{:016x}", 1700000000123.0_f64.to_bits()),
        "live Spot fallback timestamp bits"
    );
    assert_snapshot_counts(
        &output,
        "atReturn",
        &[(1, 1), (1, 0), (1, 0), (1, 0), (1, 1), (1, 0), (1, 1)],
    );
    assert_snapshot_counts(
        &output,
        "after",
        &[(1, 1), (1, 0), (1, 0), (1, 0), (1, 1), (1, 0), (1, 1)],
    );
}

#[test]
fn live_spot_order_source_controls_fail_by_name() {
    for (mutation, test, assertion) in [
        (
            "sequential",
            "live_spot_order_concurrency_return_and_sibling_effects",
            "live Spot orders concurrent request 1 did not settle",
        ),
        (
            "empty-fallback",
            "live_spot_order_concurrency_return_and_sibling_effects",
            "live Spot orders fallback observes current Store",
        ),
        (
            "page-guard",
            "live_spot_order_pagination_guard_and_partial_results",
            "live Spot pagination retained totalPage guard",
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env("KUCOIN_ORDERS_MUTATION", mutation)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(101), "source control {mutation}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(assertion),
            "source control {mutation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
#[test]
fn live_spot_order_missing_dependencies_fail_loudly() {
    for setting in [
        ("KUCOIN_ORDERS_MISSING_RUNTIME", "1"),
        ("SAFE_CEX_TYPESCRIPT_PATH", "missing-kucoin-orders-compiler"),
    ] {
        let output = source(&[], Some(setting));
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
    }
}
