//! Installed splice, including its pre-coercion length and checked mutation seam.
use safe_cex_exchange::js_runtime::value::Js;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
fn source() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_array_splice_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn number(tag: &str) -> Js {
    match tag {
        "undefined" => Js::Undefined,
        "nan" => Js::Number(f64::NAN),
        "minusInfinity" => Js::Number(f64::NEG_INFINITY),
        "infinity" => Js::Number(f64::INFINITY),
        "null" => Js::Null,
        "zero" => Js::Number(0.),
        "negative" => Js::Number(-2.),
        "fraction" => Js::Number(1.9),
        "large" => Js::Number(100.),
        _ => panic!("source tag"),
    }
}
fn view(a: &Js) -> Value {
    let len = a.get("length").unwrap().as_f64().unwrap() as usize;
    json!({"length":len,"slots":(0..len).map(|i|if a.has_index(i){json!([true,a.get(&i.to_string()).unwrap().to_display()])}else{json!([false,null])}).collect::<Vec<_>>()})
}
#[test]
fn installed_splice_matches_node_coercion_holes_and_deleted_values() {
    let source = source();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 165);
    for row in cases {
        let a = Js::holey_array(vec![
            Some(Js::Number(0.)),
            None,
            Some(Js::Number(2.)),
            Some(Js::Number(3.)),
        ]);
        let mut args = vec![];
        if row["arity"] != json!(0) {
            args.push(number(row["start"].as_str().unwrap()));
        }
        if row.get("arity").is_none() {
            args.push(number(row["count"].as_str().unwrap()));
        }
        if row["insert"] == true {
            args.extend([Js::str("x"), Js::str("y")]);
        }
        let deleted = a
            .call_method("splice", args)
            .expect("installed source splice");
        assert_eq!(view(&a), row["array"], "array {row}");
        assert_eq!(view(&deleted), row["deleted"], "deleted {row}");
    }
}
#[test]
fn installed_splice_captures_length_before_coercion_and_deleted_getters() {
    let source = source();
    for row in source["reentrant"].as_array().unwrap() {
        let a = Js::array((0..4).map(|i| Js::Number(i as f64)).collect());
        let trace = Rc::new(RefCell::new(Vec::new()));
        let seen = trace.clone();
        let weak = a.clone();
        let start = if row["where"] == "start" {
            Js::object(vec![(
                "valueOf",
                Js::function("shrink", move |_, _| {
                    seen.borrow_mut().push("start");
                    weak.set("length", Js::Number(1.))?;
                    Ok(Js::Number(1.))
                }),
            )])
        } else {
            // Weak capture prevents this test accessor retaining its own array.
            let Js::Array(array) = &a else { unreachable!() };
            let weak = Rc::downgrade(array);
            a.define_accessor(
                "1",
                Some(Rc::new(move |_| {
                    seen.borrow_mut().push("deleted");
                    let array = Js::Array(weak.upgrade().unwrap());
                    array.set("length", Js::Number(2.))?;
                    Ok(Js::Number(8.))
                })),
                None,
            )
            .unwrap();
            Js::Number(1.)
        };
        let deleted = a
            .call_method("splice", vec![start, Js::Number(1.)])
            .unwrap();
        assert_eq!(view(&a), row["array"], "{}", row["where"]);
        assert_eq!(view(&deleted), row["deleted"]);
        assert_eq!(json!(*trace.borrow()), row["trace"]);
    }
}
#[test]
fn installed_splice_keeps_removed_identity_and_stops_on_readonly_write() {
    let token = Js::object(vec![]);
    let a = Js::array(vec![token.clone(), Js::Number(-0.)]);
    let removed = a
        .call_method("splice", vec![Js::Number(0.), Js::Number(1.)])
        .unwrap();
    assert!(removed.get("0").unwrap().same_reference(&token));
    assert_eq!(
        a.get("0").unwrap().as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    let g = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&g, std::rc::Weak::new());
    g.get("Object")
        .unwrap()
        .call_method("freeze", vec![a.clone()])
        .unwrap();
    let error = a
        .call_method("splice", vec![Js::Number(0.), Js::Number(1.)])
        .unwrap_err();
    assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    assert_eq!(a.get("length").unwrap().as_f64(), Some(1.));
}
