// Native callback observations compared with actual TypeScript venue constructors.
use safe_cex_exchange::{
    js_runtime::value::Js,
    websocket::{HandlerList, MessageHandler},
};
use std::{cell::{Cell, RefCell}, collections::VecDeque, path::Path, rc::Rc};

pub fn source_frames(venue: &str, slot: &str) -> Vec<serde_json::Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("node")
        .current_dir(root)
        .args(["--import", "tsx", "sdk-rs/safe-cex-exchange/tests/websocket-retained-handler-source.cjs"])
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24, "six actual socket owners, four native string profiles each");
    let frames: Vec<_> = cases.iter().filter(|case| case["venue"] == venue && case["slot"] == slot).cloned().collect();
    assert_eq!(frames.len(), 4, "all four maintained {venue}/{slot} frames");
    frames
}

pub fn pair(cases: &[serde_json::Value], envelope: bool) -> (HandlerList, Rc<Cell<usize>>) {
    let units = Rc::new(RefCell::new(cases.iter().map(|case| {
        assert_eq!(case["callbacks"], 2);
        assert_eq!(case["sameReference"], true);
        assert_eq!(case["earlierMutationVisible"], true);
        assert_eq!(case["ownUndefined"], true);
        case["units"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap() as u16).collect::<Vec<_>>()
    }).collect::<VecDeque<_>>()));
    let retained = Rc::new(RefCell::new(None::<Js>));
    let calls = Rc::new(Cell::new(0));
    let first: MessageHandler = {
        let retained = retained.clone(); let calls = calls.clone();
        Rc::new(move |message| {
            let row = if envelope { message.get("0")? } else { message.clone() };
            let expected = units.borrow_mut().pop_front().expect("one source row per parsed frame");
            let entries = row.get("unused")?.own_entries()?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].0.as_ref(), expected.as_slice(), "native UTF-16 property key");
            let text = entries[0].1.get("0")?;
            assert!(matches!(text, Js::String(value) if value.as_ref() == expected.as_slice()), "native UTF-16 string value");
            assert!(matches!(row.get("overflow")?, Js::Number(value) if value == f64::INFINITY));
            assert!(matches!(row.get("zero")?, Js::Number(value) if value.to_bits() == 0x8000_0000_0000_0000));
            if let Some(previous) = retained.replace(Some(message.clone())) {
                assert!(!previous.strict_equals(message), "new parsed frame has a fresh graph");
            }
            row.set("mutated", Js::Number(31.25))?;
            row.define("presentUndefined", Js::Undefined)?;
            calls.set(calls.get() + 1);
            Ok(())
        })
    };
    let second: MessageHandler = {
        let calls = calls.clone();
        Rc::new(move |message| {
            assert!(retained.borrow().as_ref().unwrap().strict_equals(message), "same parsed graph across core callbacks");
            let row = if envelope { message.get("0")? } else { message.clone() };
            assert!(row.get("mutated")?.strict_equals(&Js::Number(31.25)), "earlier callback mutation remains visible");
            assert!(row.has_own("presentUndefined"));
            assert!(matches!(row.get("presentUndefined")?, Js::Undefined));
            calls.set(calls.get() + 1);
            Ok(())
        })
    };
    (Rc::new(RefCell::new(vec![first, second])), calls)
}
