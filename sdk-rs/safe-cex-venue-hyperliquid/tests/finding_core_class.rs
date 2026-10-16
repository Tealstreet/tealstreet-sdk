#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/core_exchange_host.rs"]
mod host;
#[path = "support/private_class_host.rs"]
mod private_host;
#[path = "support/public_class_host.rs"]
mod public_host;
use cache::{N, object, s};
use host::{Host, same};
use safe_cex_venue_hyperliquid::core_exchange_class::*;
#[test]
fn native_core_class_constructs_clients_sockets_and_callable_frame() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/finding-core-class-frame.json")).unwrap();
    assert_eq!(
        fixture["fields"].as_array().unwrap().len(),
        121,
        "CORE independent field floor"
    );
    assert_eq!(
        fixture["statics"].as_array().unwrap().len(),
        12,
        "CORE independent static floor"
    );
    assert_eq!(
        fixture["metadata"].as_array().unwrap().len(),
        94,
        "CORE independent installed callable floor"
    );
    let mut h = Host::default();
    let base = object(vec![("prototype", object(vec![]))]);
    let class = create_core_exchange_class(&mut h, &base).unwrap();
    let opts = object(vec![("vault", s("wallet"))]);
    let store = object(vec![]);
    let name = object(vec![]);
    let args = [name.clone(), opts.clone(), store.clone()];
    let owner = construct_core_exchange_class(&mut h, &class, &args, None).unwrap();
    let second = construct_core_exchange_class(&mut h, &class, &args, None).unwrap();
    let N::O(o) = &owner else {
        panic!("CORE native object")
    };
    let actual = o
        .borrow()
        .fields
        .iter()
        .map(|p| p.0.clone())
        .collect::<Vec<_>>();
    let expected = [
        vec!["name".into(), "options".into(), "store".into()],
        fixture["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
    ]
    .concat();
    assert_eq!(
        actual, expected,
        "CORE native all121 fields in source order"
    );
    assert_eq!(
        actual.len(),
        124,
        "CORE independent native field count including base"
    );
    {
        let st = h.state.borrow();
        assert_eq!(
            st.functions.len(),
            169,
            "CORE independent 19 prototype entries plus two75-arrow instances"
        );
        assert_eq!(st.getters.len(), 2, "CORE native getters");
        assert_eq!(
            st.factories.len(),
            8,
            "CORE independent factory floor two instances"
        );
        assert!(same(&st.supers[0].0, &class));
        assert!(same(&st.supers[0].1[0], &name));
        assert!(same(&st.supers[0].1[1], &opts));
        assert!(same(&st.supers[0].1[2], &store));
        assert!(same(&st.supers[0].2, &class));
        assert_eq!(
            st.factories
                .iter()
                .take(4)
                .map(|p| p.0.as_str())
                .collect::<Vec<_>>(),
            [
                "createExchangeAPI",
                "rateLimit",
                "createInfoAPI",
                "createExchangeAPI"
            ],
            "CORE source factory order"
        );
        for i in [0, 2, 3] {
            assert!(
                same(&st.factories[i].1[0], &opts),
                "CORE original opts handle"
            );
            assert!(
                same(&st.factories[i].1[1], &owner),
                "CORE original owner handle"
            );
        }
    }
    for (key, binding) in [
        ("xhr", "rateLimit"),
        ("infoXhr", "createInfoAPI"),
        ("unlimitedXHR", "createExchangeAPI"),
    ] {
        let client = h.get(&owner, key).unwrap();
        assert!(
            same(&h.get(&client, "binding").unwrap(), &s(binding)),
            "CORE native attached client {key}"
        );
    }
    for (key, expected) in [
        ("BUILDER_FEE_APPROVAL_TIMEOUT_MS", 8000.),
        ("HIP3_DEX_DISCOVERY_TIMEOUT_MS", 6000.),
        ("HIP3_MARKETS_LOAD_TIMEOUT_MS", 8000.),
        ("HIP3_PER_DEX_LOAD_BUDGET_MS", 2000.),
        ("HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS", 15000.),
        ("SPOT_MARKETS_LOAD_TIMEOUT_MS", 6000.),
        ("USER_ROLE_RESOLVE_TIMEOUT_MS", 6000.),
        ("USER_ABSTRACTION_RESOLVE_TIMEOUT_MS", 6000.),
        ("SPOT_COLLATERAL_REFRESH_INTERVAL_MS", 10000.),
    ] {
        assert!(
            matches!(h.get(&class,key).unwrap(),N::Num(n)if n.to_bits()==f64::to_bits(expected)),
            "CORE native static value {key}"
        );
    }
    assert!(
        same(
            &h.get(&class, "UNIFIED_MARGIN_CURRENCY").unwrap(),
            &s("USDC")
        ),
        "CORE native static currency"
    );
    let whitelist = h.get(&class, "TESTNET_HIP3_WHITELIST").unwrap();
    let N::A(w) = whitelist else {
        panic!("CORE native whitelist array")
    };
    assert_eq!(w.borrow().len(), 4, "CORE independent whitelist floor");
    for (value, key) in w.borrow().iter().zip(["vntls", "xyz", "VOLMEX", "flx"]) {
        assert!(same(value, &s(key)), "CORE native ordered whitelist entry");
    }
    let defaults = h.get(&class, "DEFAULT_SYMBOL_CONFIG").unwrap();
    assert!(
        matches!(h.get(&defaults,"leverage").unwrap(),N::Num(n)if n.to_bits()==20f64.to_bits()),
        "CORE native default leverage"
    );
    let rate_config = h.state.borrow().factories[1].1[1].clone();
    assert!(
        matches!(h.get(&rate_config,"maxRPS").unwrap(),N::Num(n)if n.to_bits()==3f64.to_bits()),
        "CORE native constructor maxRPS"
    );
    for key in ["publicWebsocket", "privateWebsocket"] {
        let child = h.get(&owner, key).unwrap();
        assert!(
            same(&h.get(&child, "parent").unwrap(), &owner),
            "CORE actual native child parent {key}"
        );
        assert!(
            !same(&child, &h.get(&second, key).unwrap()),
            "CORE fresh child {key}"
        );
    }
    for key in [
        "_placedLimitOrderTif",
        "_cancelledOrders",
        "activeAssetData",
        "marketsLowerCaseMap",
        "_pendingRetryOrderIds",
        "_deadOrders",
    ] {
        assert!(
            !same(&h.get(&owner, key).unwrap(), &h.get(&second, key).unwrap()),
            "CORE fresh state {key}"
        );
    }
    for row in fixture["metadata"].as_array().unwrap() {
        let key = row["name"].as_str().unwrap();
        let getter = row["kind"] == "getter";
        let f = if getter {
            h.state
                .borrow()
                .getters
                .iter()
                .find(|(_, k, _)| k == key)
                .unwrap()
                .2
                .clone()
        } else {
            h.get(&owner, key).unwrap()
        };
        let N::Builtin(id) = &f else {
            panic!("CORE native callable {key}")
        };
        let st = h.state.borrow();
        let cap = &st.functions[id];
        assert!(same(&cap.class, &class), "CORE defining class {key}");
        if row["kind"] == "arrow" {
            assert!(
                same(cap.receiver.as_ref().unwrap(), &owner),
                "CORE captured lexical receiver {key}"
            );
        } else {
            assert!(
                cap.receiver.is_none(),
                "CORE prototype dynamic receiver {key}"
            );
        }
        let meta = st.metadata.iter().find(|p| same(&p.0, &f)).unwrap();
        assert_eq!(meta.1, format!("{}{key}", if getter { "get " } else { "" }));
        assert_eq!(
            meta.2 as u64,
            row["length"].as_u64().unwrap(),
            "CORE function arity {key}"
        );
    }
    let nonce = h.get(&owner, "generateNonce").unwrap();
    let n = h.invoke(&nonce, &owner, &[]).unwrap();
    assert!(
        matches!(n,N::Num(n)if n.to_bits()==100f64.to_bits()),
        "CORE installed nonce native bits"
    );
    let n = h.invoke(&nonce, &owner, &[]).unwrap();
    assert!(
        matches!(n,N::Num(n)if n.to_bits()==101f64.to_bits()),
        "CORE installed nonce increments shared field"
    );
    let identity = h.get(&owner, "hasPrivateSessionIdentity").unwrap();
    assert!(
        matches!(identity, N::B(true)),
        "CORE native identity getter"
    );
    let public = h.get(&owner, "isPublicDataOnly").unwrap();
    assert!(
        matches!(public, N::B(false)),
        "CORE native public-only getter"
    );
    let remember = h.get(&owner, "rememberLimitOrderTimeInForce").unwrap();
    h.put(&owner, "PLACED_TIF_CACHE_LIMIT", &N::Num(2.));
    let first = object(vec![]);
    let second_tif = object(vec![]);
    let third = object(vec![]);
    for (key, value) in [("a", &first), ("b", &second_tif), ("c", &third)] {
        assert!(
            matches!(
                h.invoke(&remember, &owner, &[s(key), value.clone()])
                    .unwrap(),
                N::U
            ),
            "CORE native void TIF return"
        );
    }
    let get = h.get(&owner, "getPlacedLimitOrderTimeInForce").unwrap();
    assert!(
        matches!(h.invoke(&get, &owner, &[s("a")]).unwrap(), N::U),
        "CORE native oldest eviction"
    );
    assert!(
        same(&h.invoke(&get, &owner, &[s("b")]).unwrap(), &second_tif),
        "CORE native TIF handle"
    );
    let suppress = h.get(&owner, "suppressCancelledOrder").unwrap();
    h.invoke(&suppress, &owner, &[s("oid")]).unwrap();
    let cancelled = h.get(&owner, "isOrderCancelled").unwrap();
    h.state.borrow_mut().now = 120100.;
    assert!(
        matches!(
            h.invoke(&cancelled, &owner, &[s("oid")]).unwrap(),
            N::B(true)
        ),
        "CORE native strict cancellation age boundary"
    );
    h.state.borrow_mut().now = 120101.;
    assert!(
        matches!(
            h.invoke(&cancelled, &owner, &[s("oid")]).unwrap(),
            N::B(false)
        ),
        "CORE native cancellation expiration"
    );
    let ticker = object(vec![
        ("mark", N::Num(12.)),
        ("last", N::Num(9.)),
        ("midPrice", N::Num(10.)),
        ("index", N::Num(11.)),
    ]);
    let tickers = object(vec![("BTC/USDC", ticker)]);
    h.put(&store, "tickersBySymbol", &tickers);
    let market = object(vec![("symbol", s("BTC/USDC")), ("isSpot", N::B(false))]);
    let price = h.get(&owner, "getOrderBasePrice").unwrap();
    for _ in 0..2 {
        assert!(
            matches!(h.invoke(&price,&second,&[market.clone(),s("order")]).unwrap(),N::Num(n)if n.to_bits()==12f64.to_bits()),
            "CORE installed price arrow lexical receiver and native value"
        );
    }
    {
        let st = h.state.borrow();
        assert_eq!(
            st.finds.len(),
            2,
            "CORE independent native price predicate calls"
        );
        assert!(
            same(&st.finds[0], &st.finds[1]),
            "CORE finite predicate module singleton identity"
        );
    }
    let predicate = h.state.borrow().finds[0].clone();
    h.state.borrow_mut().finite_override = Some(N::Num(-0.));
    assert!(
        matches!(h.invoke(&predicate,&N::U,&[N::Num(2.)]).unwrap(),N::Num(n)if n.to_bits()==(-0f64).to_bits()),
        "CORE native finite predicate preserves raw falsy negative-zero return"
    );
    h.state.borrow_mut().finite_override = None;
    let fold = N::Builtin("fold".into());
    h.put(&owner, "foldClosedPositionRecords", &fold);
    let history = h.get(&owner, "fetchHistoricalPositions").unwrap();
    let original = object(vec![("account", s("wallet"))]);
    let promise = h.invoke(&history, &second, &[original.clone()]).unwrap();
    let result = h
        .promise_result(&promise)
        .expect("CORE eager native promise settled")
        .unwrap();
    assert!(
        same(&result, &original),
        "CORE ordinary async installed body retains original native value"
    );
    {
        let st = h.state.borrow();
        assert_eq!(
            st.return_adoptions, 1,
            "CORE native mapped return adoption exactly once"
        );
        assert_eq!(
            st.outer_adoptions, 0,
            "CORE no duplicate outer return assimilation"
        );
        assert_eq!(st.folds.len(), 1, "CORE eager async body runs exactly once");
        assert!(
            same(&st.folds[0].0, &owner),
            "CORE retained async lexical receiver"
        );
        assert!(
            same(&st.folds[0].1[0], &original),
            "CORE retained async original argument"
        );
    }
    // Native frame/values/identity assertions precede source receipt/wire reading.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-core-class-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CORE actual source constructor: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["source_cases"], 2);
    assert_eq!(receipt["installed_callables"], 94);
    assert_eq!(
        receipt["state_cases"], 6,
        "CORE independent source state cases"
    );
    assert_eq!(
        receipt["stop"],
        "second instance fresh state and installed callables"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-history-adoption-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CORE actual same-realm return assimilation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["source_cases"], 1);
    assert_eq!(receipt["source_then_reads"], 1);
    assert_eq!(receipt["wrong_bridge_then_reads"], 2);
    assert_eq!(receipt["second_getter_error_identity"], true);
}
