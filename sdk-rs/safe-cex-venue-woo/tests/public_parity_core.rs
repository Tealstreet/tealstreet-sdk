use safe_cex_num::JsJsonValue;
use safe_cex_venue_woo::public_parity::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-parity-core-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual public parity source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn number(n: f64) -> ParityValue {
    ParityValue::new(JsJsonValue::Number(n))
}
fn assert_value(actual: &ParityValue, expected: &Value, refs: &[ParityValue]) {
    match expected["kind"].as_str().unwrap() {
        "undefined" => assert!(
            matches!(actual.value(), JsJsonValue::Undefined),
            "native undefined value"
        ),
        "null" => assert!(
            matches!(actual.value(), JsJsonValue::Null),
            "native null value"
        ),
        "number" => {
            let JsJsonValue::Number(n) = actual.value() else {
                panic!("native number variant")
            };
            assert_eq!(
                format!("{:016x}", n.to_bits()),
                expected["bits"],
                "native mismatch number bits"
            );
        }
        "string" => {
            let JsJsonValue::String(s) = actual.value() else {
                panic!("native string variant")
            };
            assert_eq!(
                s,
                expected["value"].as_str().unwrap(),
                "native string value"
            );
        }
        "bigint" => {
            let JsJsonValue::BigInt(n) = actual.value() else {
                panic!("native bigint variant")
            };
            assert_eq!(n.to_string(), expected["value"], "native bigint value");
        }
        "reference" => assert!(
            actual.object_is(&refs[expected["id"].as_u64().unwrap() as usize - 1]),
            "native retained reference identity"
        ),
        other => panic!("unknown source value tag {other}"),
    }
}
fn assert_mismatch(actual: &WooV3PublicParityMismatch, expected: &Value, refs: &[ParityValue]) {
    let kind = match expected["kind"].as_str().unwrap() {
        "value" => ParityMismatchKind::Value,
        "missing-in-v3" => ParityMismatchKind::MissingInV3,
        "unexpected-in-v3" => ParityMismatchKind::UnexpectedInV3,
        "duplicate-key" => ParityMismatchKind::DuplicateKey,
        _ => panic!("kind"),
    };
    assert_eq!(actual.kind(), kind, "native mismatch kind");
    assert_eq!(actual.path(), expected["path"], "native mismatch path");
    assert_value(actual.legacy_value(), &expected["legacy"], refs);
    assert_value(actual.v3_value(), &expected["v3"], refs);
    assert_eq!(
        actual.provenance().legacy(),
        expected["legacyProvenance"],
        "native legacy provenance"
    );
    assert_eq!(
        actual.provenance().v3(),
        expected["v3Provenance"],
        "native v3 provenance"
    );
}

#[test]
fn decoded_native_routes_and_field_same_value_semantics() {
    for (text, expected) in [
        ("markets", WooV3PublicParityRouteId::Markets),
        ("tickers", WooV3PublicParityRouteId::Tickers),
        ("orderbook", WooV3PublicParityRouteId::Orderbook),
        ("recent-klines", WooV3PublicParityRouteId::RecentKlines),
        (
            "historical-klines",
            WooV3PublicParityRouteId::HistoricalKlines,
        ),
    ] {
        let actual: WooV3PublicParityRouteId = serde_json::from_value(json!(text)).unwrap();
        assert_eq!(actual, expected, "native decoded route variant");
    }
    assert!(serde_json::from_value::<WooV3PublicParityRouteId>(json!("unknown")).is_err());
    let src = source();
    assert_eq!(
        src["fields"].as_array().unwrap().len(),
        8,
        "independent field floor"
    );
    let object = ParityValue::new(JsJsonValue::Object(vec![(
        "x".into(),
        JsJsonValue::Number(1.),
    )]));
    let other = ParityValue::new(JsJsonValue::Object(vec![(
        "x".into(),
        JsJsonValue::Number(1.),
    )]));
    let refs = [object.clone(), other.clone()];
    let pairs = [
        (number(f64::NAN), number(f64::NAN)),
        (number(0.), number(-0.)),
        (
            ParityValue::undefined(),
            ParityValue::new(JsJsonValue::Null),
        ),
        (object.clone(), object.clone()),
        (object, other),
        (number(5.), number(7.)),
        (
            ParityValue::new(JsJsonValue::BigInt("2".parse().unwrap())),
            ParityValue::new(JsJsonValue::BigInt("2".parse().unwrap())),
        ),
        (
            ParityValue::new(JsJsonValue::BigInt("2".parse().unwrap())),
            ParityValue::new(JsJsonValue::BigInt("3".parse().unwrap())),
        ),
    ];
    let counts = [0, 1, 1, 0, 1, 1, 0, 1];
    for (i, (l, v)) in pairs.into_iter().enumerate() {
        let trace = RefCell::new(Vec::new());
        let left = |_: &()| -> Result<_, ()> {
            trace.borrow_mut().push("legacy");
            Ok(l.clone())
        };
        let right = |_: &()| -> Result<_, ()> {
            trace.borrow_mut().push("v3");
            Ok(v.clone())
        };
        let fields = [
            None,
            Some(FieldComparison {
                path: "value",
                legacy: &left,
                v3: &right,
                legacy_provenance: "L",
                v3_provenance: "V",
            }),
        ];
        let out = compare_fields(&(), &(), "root", &fields).unwrap();
        assert_eq!(
            out.len(),
            counts[i],
            "native Object.is mismatch count case {i}"
        );
        assert_eq!(
            src["fields"][i]["out"].as_array().unwrap().len(),
            counts[i],
            "independent source field count case {i}"
        );
        assert_eq!(
            *trace.borrow(),
            ["legacy", "v3"],
            "native field callback order"
        );
        for (m, e) in out.iter().zip(src["fields"][i]["out"].as_array().unwrap()) {
            assert_mismatch(m, e, &refs);
        }
    }
    assert_eq!(src["chosen"]["compoundAliasAfter"], 9);
    let JsJsonValue::Object(data) = refs[0].value() else {
        panic!()
    };
    assert_eq!(
        data[0].1,
        JsJsonValue::Number(1.),
        "native chosen immutable compound snapshot"
    );
    assert_eq!(src["chosen"]["metadataPath"], "metadata.after");
    let left = |_: &()| -> Result<_, ()> { Ok(number(1.)) };
    let right = |_: &()| -> Result<_, ()> { Ok(number(2.)) };
    let fixed = compare_fields(
        &(),
        &(),
        "metadata",
        &[Some(FieldComparison {
            path: "before",
            legacy: &left,
            v3: &right,
            legacy_provenance: "L",
            v3_provenance: "V",
        })],
    )
    .unwrap();
    assert_eq!(
        fixed[0].path(),
        "metadata.before",
        "native chosen fixed callback metadata"
    );
}

#[derive(Clone)]
struct Row {
    key: &'static str,
    value: f64,
}
#[test]
fn keyed_native_first_winner_order_and_stored_undefined() {
    let src = source();
    let trace = RefCell::new(Vec::new());
    let legacy = [
        Some(Row {
            key: "OTHER",
            value: 11.,
        }),
        Some(Row {
            key: "42",
            value: 22.,
        }),
        Some(Row {
            key: "OTHER",
            value: 99.,
        }),
        Some(Row {
            key: "missing",
            value: 33.,
        }),
    ];
    let v3 = [
        Some(Row {
            key: "42",
            value: 23.,
        }),
        Some(Row {
            key: "new",
            value: 44.,
        }),
        Some(Row {
            key: "42",
            value: 88.,
        }),
        Some(Row {
            key: "OTHER",
            value: 12.,
        }),
    ];
    let lk = |x: &Option<Row>| -> Result<_, ()> {
        let x = x.as_ref().unwrap();
        trace.borrow_mut().push(format!("LK:{}", x.key));
        Ok(x.key.into())
    };
    let vk = |x: &Option<Row>| -> Result<_, ()> {
        let x = x.as_ref().unwrap();
        trace.borrow_mut().push(format!("VK:{}", x.key));
        Ok(x.key.into())
    };
    let l = |x: &Option<Row>| -> Result<_, ()> {
        let x = x.as_ref().unwrap();
        trace.borrow_mut().push(format!("L:{}", x.key));
        Ok(number(x.value))
    };
    let v = |x: &Option<Row>| -> Result<_, ()> {
        let x = x.as_ref().unwrap();
        trace.borrow_mut().push(format!("V:{}", x.key));
        Ok(number(x.value))
    };
    let fields = [Some(FieldComparison {
        path: "value",
        legacy: &l,
        v3: &v,
        legacy_provenance: "LF",
        v3_provenance: "VF",
    })];
    let out = compare_keyed(KeyedComparison {
        path: "rows",
        legacy: &legacy,
        v3: &v3,
        legacy_key: &lk,
        v3_key: &vk,
        fields: &fields,
        legacy_provenance: "LR",
        v3_provenance: "VR",
    })
    .unwrap();
    assert_eq!(out.len(), 6, "independent native keyed count");
    assert_eq!(
        out.iter().map(|m| m.path()).collect::<Vec<_>>(),
        [
            "rows[OTHER]",
            "rows[42]",
            "rows[OTHER].value",
            "rows[42].value",
            "rows[missing]",
            "rows[new]"
        ],
        "native keyed insertion order OTHER before 42"
    );
    assert_value(
        out[2].legacy_value(),
        &json!({"kind":"number","bits":"4026000000000000"}),
        &[],
    );
    for (m, e) in out.iter().zip(src["keyed"].as_array().unwrap()) {
        assert_mismatch(m, e, &[]);
    }
    assert_eq!(
        trace.borrow().len(),
        12,
        "independent native callback floor"
    );
    assert_eq!(
        json!(*trace.borrow()),
        src["trace"],
        "native keyed callbacks"
    );
    let key = |_: &Option<Row>| -> Result<_, ()> { Ok("K".into()) };
    let never = |_: &Option<Row>| -> Result<ParityValue, ()> {
        panic!("undefined match must not invoke a field")
    };
    let fields = [Some(FieldComparison {
        path: "value",
        legacy: &never,
        v3: &never,
        legacy_provenance: "L",
        v3_provenance: "V",
    })];
    let out = compare_keyed(KeyedComparison {
        path: "rows",
        legacy: &legacy[..1],
        v3: &[None],
        legacy_key: &key,
        v3_key: &key,
        fields: &fields,
        legacy_provenance: "LR",
        v3_provenance: "VR",
    })
    .unwrap();
    assert_eq!(
        out.len(),
        1,
        "native stored undefined is missing, not unexpected"
    );
    assert_mismatch(&out[0], &src["undefined"][0], &[]);
    assert_eq!(
        src["chosen"]["liveArrayPaths"],
        json!(["live[first]", "live[appended]"])
    );
    let native_fixed = compare_keyed(KeyedComparison {
        path: "live",
        legacy: &[Some(Row {
            key: "first",
            value: 0.,
        })],
        v3: &[],
        legacy_key: &|r: &Option<Row>| -> Result<String, ()> { Ok(r.as_ref().unwrap().key.into()) },
        v3_key: &key,
        fields: &[],
        legacy_provenance: "L",
        v3_provenance: "V",
    })
    .unwrap();
    assert_eq!(native_fixed.len(), 1, "native chosen fixed slice iteration");
}

#[test]
fn native_error_identity_and_no_later_getter_call() {
    let src = source();
    assert_eq!(src["sameError"], true);
    let error = Rc::new("unchanged");
    let trace = RefCell::new(Vec::new());
    let left = |_: &()| -> Result<ParityValue, Rc<&str>> {
        trace.borrow_mut().push("L");
        Err(error.clone())
    };
    let right = |_: &()| -> Result<ParityValue, Rc<&str>> {
        trace.borrow_mut().push("V");
        Ok(number(2.))
    };
    let out = compare_fields(
        &(),
        &(),
        "x",
        &[Some(FieldComparison {
            path: "value",
            legacy: &left,
            v3: &right,
            legacy_provenance: "L",
            v3_provenance: "V",
        })],
    )
    .unwrap_err();
    assert!(Rc::ptr_eq(&out, &error), "native callback error identity");
    assert_eq!(*trace.borrow(), ["L"], "native failure stops later getter");
    assert_eq!(json!(*trace.borrow()), src["errorTrace"]);
}

#[test]
fn native_blocker_presence_result_priority_and_snapshot_identity() {
    let src = source();
    let absent = blocker(
        ParityBlockerCode::InvalidRequestContext,
        "detail",
        ParityBlockerContext::default(),
    );
    let present = blocker(
        ParityBlockerCode::RequestSymbolMismatch,
        "detail",
        ParityBlockerContext {
            path: Some(None),
            expected: Some(ParityValue::undefined()),
            actual: Some(ParityValue::new(JsJsonValue::Null)),
        },
    );
    assert_eq!(
        absent.code(),
        ParityBlockerCode::InvalidRequestContext,
        "native blocker code"
    );
    assert_eq!(
        present.code(),
        ParityBlockerCode::RequestSymbolMismatch,
        "native present blocker code"
    );
    assert_eq!(absent.detail(), "detail");
    assert_eq!(present.detail(), "detail");
    assert_eq!(absent.path(), None, "native absent path");
    assert_eq!(present.path(), Some(None), "native present undefined path");
    assert!(absent.expected().is_none());
    assert!(absent.actual().is_none());
    assert_value(
        present.expected().unwrap(),
        &src["blockers"][1]["expected"],
        &[],
    );
    assert_value(
        present.actual().unwrap(),
        &src["blockers"][1]["actual"],
        &[],
    );
    assert_eq!(src["blockers"][0]["pathOwn"], false);
    assert_eq!(src["blockers"][1]["pathOwn"], true);
    let m = mismatch(
        ParityMismatchKind::DuplicateKey,
        "rows[OTHER]",
        ParityValue::string("OTHER"),
        ParityValue::undefined(),
        "LR",
        "VR",
    );
    let a = RefCell::new(1);
    let o = RefCell::new(2);
    let cases = [
        (vec![], None, ParityStatus::Match, 0, 0),
        (
            vec![Some(m.clone())],
            Some(vec![]),
            ParityStatus::Mismatch,
            1,
            0,
        ),
        (
            vec![Some(m.clone())],
            Some(vec![Some(present.clone())]),
            ParityStatus::Blocked,
            1,
            1,
        ),
        (vec![None], Some(vec![None]), ParityStatus::Blocked, 1, 1),
    ];
    assert_eq!(
        src["results"].as_array().unwrap().len(),
        4,
        "independent result floor"
    );
    for (i, (mut ms, bs, status, nm, nb)) in cases.into_iter().enumerate() {
        let r = result(
            WooV3PublicParityRouteId::Markets,
            &a,
            &o,
            "LC",
            "VC",
            &ms,
            bs.as_deref(),
        );
        assert_eq!(
            r.status(),
            status,
            "native result blocker priority case {i}"
        );
        assert_eq!(
            r.route_id(),
            WooV3PublicParityRouteId::Markets,
            "native result route"
        );
        assert!(
            std::ptr::eq(r.authoritative(), &a),
            "native authoritative identity"
        );
        assert!(std::ptr::eq(r.observed(), &o), "native observed identity");
        assert_eq!(r.legacy_canonical(), "LC", "native legacy canonical");
        assert_eq!(r.v3_canonical(), "VC", "native v3 canonical");
        assert_eq!(r.store_action(), "retain-legacy");
        assert_eq!(r.mismatches().len(), nm, "native mismatch array copy count");
        assert_eq!(r.blockers().len(), nb, "native blocker array copy count");
        if let Some(Some(x)) = r.mismatches().first() {
            assert!(Rc::ptr_eq(x, &m), "native shallow mismatch identity");
        }
        if let Some(Some(x)) = r.blockers().first() {
            assert!(Rc::ptr_eq(x, &present), "native shallow blocker identity");
        }
        ms.push(Some(m.clone()));
        assert_eq!(
            r.mismatches().len(),
            nm,
            "native array copy remains unchanged"
        );
        *a.borrow_mut() = 10;
        assert_eq!(
            *r.authoritative().borrow(),
            10,
            "native live authoritative alias"
        );
        assert_eq!(src["results"][i]["mismatches"], nm);
        assert_eq!(src["results"][i]["blockers"], nb);
    }
    assert_eq!(src["chosen"]["contextOverride"]["code"], "override");
    assert_eq!(
        present.code(),
        ParityBlockerCode::RequestSymbolMismatch,
        "native typed context cannot override code"
    );
    assert_eq!(src["chosen"]["customResultStatus"], "mismatch");
    assert_eq!(src["chosen"]["customResultCount"], 1);
    let fixed = result(
        WooV3PublicParityRouteId::Markets,
        &a,
        &o,
        "LC",
        "VC",
        &[],
        None,
    );
    assert_eq!(
        fixed.status(),
        ParityStatus::Match,
        "native chosen standard slice copy"
    );
    assert_eq!(fixed.mismatches().len(), 0);
}

#[test]
fn native_string_surface_refuses_source_observable_units() {
    let src = source();
    let rows = src["refusals"].as_array().unwrap();
    assert_eq!(rows.len(), 5, "independent forced record floor");
    for (row, count) in rows.iter().zip([3, 3, 1, 3, 3]) {
        assert_eq!(row["slots"].as_array().unwrap().len(), count);
        assert_eq!(row["units"].as_array().unwrap().len(), count);
        assert_eq!(row["stringWires"].as_array().unwrap().len(), count);
        for unit in row["units"].as_array().unwrap() {
            assert_eq!(unit, &json!(0xd800), "source returned observable unit");
        }
        assert!(
            serde_json::from_str::<Value>(row["wire"].as_str().unwrap())
                .map(JsJsonValue::from)
                .is_err(),
            "native serde JSON ingress refuses whole record before JsJsonValue conversion"
        );
        for wire in row["stringWires"].as_array().unwrap() {
            assert!(
                serde_json::from_str::<String>(wire.as_str().unwrap()).is_err(),
                "native required string refuses observable unit"
            );
        }
    }
    for (input, expected) in [
        (r#""ordinary""#, "ordinary"),
        (r#""\ud83d\ude00""#, "😀"),
        (r#""\\ud800""#, "\\ud800"),
    ] {
        let native: String = serde_json::from_str(input).unwrap();
        assert_eq!(native, expected, "native scalar pair literal exact");
    }
}
