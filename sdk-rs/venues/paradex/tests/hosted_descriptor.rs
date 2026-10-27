//! Hosted (mode 2) descriptor originals: closed parsing, the native presence
//! table, time policy, and digests equal to independent starknet-js hashes and
//! to local signing (RUST_PARADEX_HOSTED_STARK_CONSUMER.md).
use safe_cex_venue_paradex::hosted::{
    DescriptorError, ParadexBinding, ParadexDescriptor, validate, validated_digests,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    chain: String,
    address: String,
    now: u64,
    short_auth: Hashed,
    orders: Vec<Vector>,
}

#[derive(Deserialize)]
struct Hashed {
    hash: String,
}

#[derive(Deserialize)]
struct Vector {
    name: String,
    hash: String,
}

fn corpus() -> Vec<Case> {
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }
    serde_json::from_str::<Corpus>(include_str!("sign-vectors.json"))
        .unwrap()
        .cases
}

fn vector<'a>(case: &'a Case, name: &str) -> &'a str {
    &case.orders.iter().find(|o| o.name == name).unwrap().hash
}

fn hex(digest: &[u8; 32]) -> String {
    format!("{:#x}", starknet_crypto::Felt::from_bytes_be(digest))
}

fn parse(value: &Value) -> Result<ParadexDescriptor, DescriptorError> {
    ParadexDescriptor::from_json(&value.to_string())
}

fn binding(case: &Case) -> ParadexBinding {
    ParadexBinding::new(&case.address, &case.chain).unwrap()
}

fn auth(now_ms: u64) -> Value {
    json!({"family": "paradex", "op": "auth", "revision": "paradex/1",
        "timestampS": now_ms / 1000, "expirationS": now_ms / 1000 + 300})
}

fn order(now_ms: u64) -> Value {
    json!({"timestampMs": now_ms, "market": "BTC-USD-PERP", "side": "BUY",
        "orderType": "LIMIT", "size": "0.00000999", "price": "0.00000999",
        "triggerPrice": null, "instruction": "GTC", "flags": null,
        "stp": "EXPIRE_TAKER", "clientId": "ts_1"})
}

fn create(target: &str, order: Value) -> Value {
    json!({"family": "paradex", "op": "orderCreate", "revision": "paradex/1",
        "target": target, "order": order})
}

fn batch(items: Vec<Value>) -> Value {
    json!({"family": "paradex", "op": "orderBatch", "revision": "paradex/1", "items": items})
}

fn modify(now_ms: u64, kind: &str, price: &str, trigger: Option<&str>) -> Value {
    json!({"family": "paradex", "op": "orderModify", "revision": "paradex/1",
        "id": "123456789012345678901", "timestampMs": now_ms, "market": "BTC-USD-PERP",
        "side": "BUY", "orderType": kind, "size": "2.5", "price": price,
        "triggerPrice": trigger})
}

/// Digests at the corpus clock, inside its deadline.
fn signable(
    descriptor: &ParadexDescriptor,
    case: &Case,
) -> Result<safe_cex_venue_paradex::hosted::Digests, DescriptorError> {
    let now = case.now as f64;
    validated_digests(descriptor, &binding(case), now, now + 1_000.)
}

fn single(value: &Value, case: &Case) -> String {
    let descriptor = parse(value).unwrap();
    let digests = signable(&descriptor, case).unwrap();
    assert_eq!(digests.as_slice().len(), 1);
    hex(&digests.as_slice()[0])
}

#[test]
fn auth_create_and_modify_digests_equal_independent_starknet_js() {
    for case in corpus() {
        assert_eq!(single(&auth(case.now), &case), case.short_auth.hash);
        assert_eq!(
            single(&create("orders", order(case.now)), &case),
            vector(&case, "limit-tiny")
        );
        let mut market = order(case.now);
        market["market"] = json!("ETH-USD-PERP");
        market["side"] = json!("SELL");
        market["orderType"] = json!("MARKET");
        market["size"] = json!("1.234567899");
        market["price"] = Value::Null;
        assert_eq!(
            single(&create("orders", market), &case),
            vector(&case, "market")
        );
        // Native num::format renders 1e-8 as 0.00000001; both quantize to 1.
        let mut stop = order(case.now);
        stop["side"] = json!("SELL");
        stop["orderType"] = json!("STOP_MARKET");
        stop["size"] = json!("0.00000001");
        stop["price"] = Value::Null;
        stop["triggerPrice"] = json!("60000");
        stop["flags"] = json!(["REDUCE_ONLY"]);
        stop["stp"] = Value::Null;
        stop["clientId"] = Value::Null;
        assert_eq!(
            single(&create("orders", stop), &case),
            vector(&case, "stop-market")
        );
        assert_eq!(
            single(&modify(case.now, "LIMIT", "12500.12345678", None), &case),
            vector(&case, "modify")
        );
    }
}

#[test]
fn a_batch_returns_one_digest_per_item_in_order() {
    let case = &corpus()[0];
    let mut items = Vec::new();
    for index in 0..10 {
        let mut item = order(case.now);
        item["clientId"] = json!(format!("ts_{index}"));
        item["price"] = json!(format!("{}", 100 + index));
        if index == 3 {
            item["flags"] = json!(["REDUCE_ONLY"]);
            item["instruction"] = json!("POST_ONLY");
        }
        items.push(item);
    }
    let descriptor = parse(&batch(items.clone())).unwrap();
    let all = signable(&descriptor, case).unwrap();
    assert_eq!(all.as_slice().len(), 10);
    for (item, digest) in items.into_iter().zip(all.as_slice()) {
        assert_eq!(hex(digest), single(&create("orders", item), case));
    }
    assert!(parse(&batch(vec![order(case.now)])).is_ok(), "one item");
}

#[test]
fn a_reduce_only_limit_is_valid_single_and_batch_single() {
    let case = &corpus()[0];
    let mut item = order(case.now);
    item["flags"] = json!(["REDUCE_ONLY"]);
    item["instruction"] = json!("IOC");
    for target in ["orders", "ordersBatchSingle"] {
        assert!(parse(&create(target, item.clone())).is_ok(), "{target}");
    }
    // Native keeps a caller trigger price on a plain LIMIT/MARKET create.
    item["triggerPrice"] = json!("1");
    assert!(parse(&create("orders", item.clone())).is_ok());
    assert_eq!(
        parse(&create("ordersBatchSingle", item)),
        Err(DescriptorError::Presence)
    );
}

#[test]
fn modify_admits_each_native_trigger_kind_and_refuses_create_only_types() {
    let now = corpus()[0].now;
    for kind in [
        "STOP_LIMIT",
        "STOP_MARKET",
        "STOP_LOSS_LIMIT",
        "STOP_LOSS_MARKET",
        "TAKE_PROFIT_LIMIT",
        "TAKE_PROFIT_MARKET",
    ] {
        assert!(
            parse(&modify(now, kind, "0", Some("100"))).is_ok(),
            "{kind}"
        );
        assert_eq!(
            parse(&modify(now, kind, "0", None)),
            Err(DescriptorError::Presence),
            "{kind} needs its trigger"
        );
    }
    assert_eq!(
        parse(&modify(now, "MARKET", "0", None)),
        Err(DescriptorError::Malformed)
    );
    let mut create_only = order(now);
    create_only["orderType"] = json!("TAKE_PROFIT_LIMIT");
    assert_eq!(
        parse(&create("orders", create_only)),
        Err(DescriptorError::Malformed)
    );
}

#[test]
fn closed_shape_refuses_unknown_missing_and_swapped_fields() {
    let now = corpus()[0].now;
    let good = create("orders", order(now));
    let mut cases: Vec<Value> = Vec::new();
    for key in ["triggerPrice", "flags", "stp", "clientId", "price"] {
        let mut missing = good.clone();
        missing["order"].as_object_mut().unwrap().remove(key);
        cases.push(missing);
    }
    let mut extra = good.clone();
    extra["order"]["digest"] = json!("0x1");
    cases.push(extra);
    let mut root_extra = good.clone();
    root_extra["digest"] = json!("0x1");
    cases.push(root_extra);
    for (key, bad) in [
        ("family", json!("kraken")),
        ("revision", json!("paradex/2")),
        ("op", json!("transfer")),
        ("op", json!("cancelBySignature")),
        ("target", json!("ordersBatch")),
    ] {
        let mut wrong = good.clone();
        wrong[key] = bad;
        cases.push(wrong);
    }
    for (key, bad) in [
        ("timestampMs", json!(9_007_199_254_740_992u64)),
        ("timestampMs", json!(-1)),
        ("timestampMs", json!(1.5)),
        ("timestampMs", json!("1717171717171")),
        ("size", json!("1e-8")),
        ("size", json!("+1")),
        ("size", json!("01")),
        ("size", json!("1.0")),
        ("size", json!(1)),
        ("side", json!("buy")),
        ("instruction", json!("FOK")),
        ("clientId", json!("")),
        ("clientId", json!("a\nb")),
        ("market", json!("")),
    ] {
        let mut wrong = good.clone();
        wrong["order"][key] = bad;
        cases.push(wrong);
    }
    for case in cases {
        assert_eq!(parse(&case), Err(DescriptorError::Malformed), "{case}");
    }
}

#[test]
fn presence_table_refuses_combinations_native_never_emits() {
    let now = corpus()[0].now;
    let variant = |edit: &dyn Fn(&mut Value)| {
        let mut item = order(now);
        edit(&mut item);
        create("orders", item)
    };
    for case in [
        variant(&|o| o["price"] = Value::Null),    // LIMIT needs price
        variant(&|o| o["size"] = json!("0")),      // positive size
        variant(&|o| o["clientId"] = Value::Null), // stp without client id
        variant(&|o| o["stp"] = Value::Null),      // client id without stp
        variant(&|o| {
            o["orderType"] = json!("MARKET"); // MARKET carries no price
        }),
        variant(&|o| {
            o["orderType"] = json!("STOP_LIMIT");
            o["triggerPrice"] = json!("1");
            o["flags"] = json!(["REDUCE_ONLY"]); // never reduce-only natively
        }),
        variant(&|o| {
            o["orderType"] = json!("STOP_MARKET"); // stops always reduce-only
            o["price"] = Value::Null;
            o["triggerPrice"] = json!("1");
        }),
    ] {
        assert_eq!(parse(&case), Err(DescriptorError::Presence), "{case}");
    }
}

#[test]
fn batch_bounds_eligibility_and_unique_client_ids() {
    let now = corpus()[0].now;
    let numbered = |count: usize| {
        (0..count)
            .map(|index| {
                let mut item = order(now);
                item["clientId"] = json!(format!("ts_{index}"));
                item
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(parse(&batch(Vec::new())), Err(DescriptorError::Presence));
    assert_eq!(parse(&batch(numbered(11))), Err(DescriptorError::Presence));
    let mut duplicate = numbered(3);
    duplicate[2]["clientId"] = json!("ts_0");
    assert_eq!(parse(&batch(duplicate)), Err(DescriptorError::Presence));
    let mut ineligible = numbered(3);
    ineligible[1]["orderType"] = json!("MARKET");
    ineligible[1]["price"] = Value::Null;
    assert_eq!(parse(&batch(ineligible)), Err(DescriptorError::Presence));
    let mut malformed = numbered(3);
    malformed[1]["size"] = json!("1e1");
    assert_eq!(parse(&batch(malformed)), Err(DescriptorError::Malformed));
}

#[test]
fn auth_expiration_is_exactly_three_hundred_seconds() {
    let now = corpus()[0].now;
    let mut value = auth(now);
    value["expirationS"] = json!(now / 1000 + 301);
    assert_eq!(parse(&value), Err(DescriptorError::Presence));
}

#[test]
fn validate_refuses_nonfinite_and_negative_times_before_anything_else() {
    let now = corpus()[0].now as f64;
    let descriptor = parse(&auth(now as u64)).unwrap();
    for (peer, deadline) in [
        (f64::NAN, now + 1000.),
        (now, f64::NAN),
        (f64::INFINITY, now),
        (now, f64::INFINITY),
        (f64::NEG_INFINITY, now),
        (-1., now),
        (now, -1.),
    ] {
        assert_eq!(
            validate(&descriptor, peer, deadline),
            Err(DescriptorError::Time),
            "{peer} {deadline}"
        );
    }
    // A fractional raw time is valid and compared exactly against the deadline.
    assert_eq!(validate(&descriptor, now + 0.25, now + 0.5), Ok(()));
    assert_eq!(
        validate(&descriptor, now + 0.5, now + 0.5),
        Err(DescriptorError::Deadline)
    );
    assert_eq!(
        validate(&descriptor, now + 0.75, now + 0.5),
        Err(DescriptorError::Deadline)
    );
}

#[test]
fn skew_bounds_are_inclusive_at_thirty_seconds() {
    let now = corpus()[0].now;
    let auth = parse(&auth(now)).unwrap();
    let floor = (now / 1000 * 1000) as f64;
    let far = 1e15;
    assert_eq!(validate(&auth, floor + 30_000., far), Ok(()));
    assert_eq!(
        validate(&auth, floor + 31_000., far),
        Err(DescriptorError::Skew)
    );
    let order = parse(&create("orders", order(now))).unwrap();
    let at = now as f64;
    for edge in [-30_000., 30_000., 30_000.75] {
        assert_eq!(validate(&order, at + edge, far), Ok(()), "{edge}");
    }
    for edge in [-30_001., 30_001.] {
        assert_eq!(
            validate(&order, at + edge, far),
            Err(DescriptorError::Skew),
            "{edge}"
        );
    }
}

#[test]
fn binding_refuses_a_zero_account_or_unknown_chain() {
    assert_eq!(
        ParadexBinding::new("0x0", "PRIVATE_SN_POTC_SEPOLIA"),
        Err(DescriptorError::Binding)
    );
    assert_eq!(
        ParadexBinding::new(
            "0x123",
            "not a chain id because it is too long for a short string"
        ),
        Err(DescriptorError::Binding)
    );
}

#[test]
fn expired_or_skewed_descriptors_yield_no_digests() {
    let case = &corpus()[0];
    let descriptor = parse(&create("orders", order(case.now))).unwrap();
    let now = case.now as f64;
    let bind = binding(case);
    assert_eq!(
        validated_digests(&descriptor, &bind, now, now),
        Err(DescriptorError::Deadline)
    );
    assert_eq!(
        validated_digests(&descriptor, &bind, now + 30_001., now + 60_000.),
        Err(DescriptorError::Skew)
    );
    assert_eq!(
        validated_digests(&descriptor, &bind, f64::NAN, now),
        Err(DescriptorError::Time)
    );
    assert!(validated_digests(&descriptor, &bind, now, now + 1.).is_ok());
}
