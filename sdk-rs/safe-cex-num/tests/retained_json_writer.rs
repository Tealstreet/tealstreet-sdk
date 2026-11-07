use safe_cex_num::{js_retained_json_stringify as stringify, js_retained_truthy};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::process::Command;
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
#[test]
fn retained_writer_matches_node_for_every_utf16_unit() {
    let output = Command::new("node").args(["-e", "process.stdout.write(JSON.stringify(Array.from({length:65536},(_,i)=>JSON.stringify(String.fromCharCode(i)))))"]).output().unwrap();
    assert!(output.status.success());
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 65536);
    for (unit, expected) in expected.into_iter().enumerate() {
        let actual = stringify(&Input::Text(vec![unit as u16])).unwrap().unwrap();
        assert_eq!(actual, expected, "unit {unit:x}");
        assert_eq!(
            Input::parse_json(&units(&actual)).unwrap(),
            Input::Text(vec![unit as u16])
        );
    }
}
#[test]
fn retained_writer_keeps_native_data_until_wire_projection() {
    let value = Input::Object(vec![
        (units("z"), Input::Float(-0.0)),
        (units("10"), Input::Float(f64::NAN)),
        (units("2"), Input::Text(vec![0xd800])),
        (units("omit"), Input::Undefined),
        (
            vec![0xdfff],
            Input::Array(vec![
                Input::Undefined,
                Input::Float(f64::INFINITY),
                Input::Text(vec![0xd83d, 0xde80]),
            ]),
        ),
        (units("z"), Input::Float(23.0)),
        (units("01"), Input::Float(f64::NEG_INFINITY)),
    ]);
    let output = Command::new("node").args(["-e", "const v={z:-0,10:NaN,2:'\\ud800',omit:undefined,'\\udfff':[undefined,Infinity,'🚀']};v.z=23;v['01']=-Infinity;process.stdout.write(JSON.stringify(v))"]).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        stringify(&value).unwrap().unwrap().as_bytes(),
        output.stdout
    );
    assert_eq!(stringify(&Input::Undefined).unwrap(), None);
    assert_eq!(
        stringify(&Input::Float(-0.0)).unwrap().as_deref(),
        Some("0")
    );
    let Input::Object(fields) = &value else {
        panic!("object")
    };
    assert_eq!(fields.len(), 7);
    assert!(matches!(fields[1].1, Input::Float(v) if v.is_nan()));
    assert!(matches!(fields[3].1, Input::Undefined));
    for value in [
        Input::Undefined,
        Input::Null,
        Input::Bool(false),
        Input::Float(f64::NAN),
        Input::Float(-0.0),
        Input::Text(vec![]),
    ] {
        assert!(!js_retained_truthy(&value));
    }
    for value in [
        Input::Text(vec![0xd800]),
        Input::Array(vec![]),
        Input::Object(vec![]),
        Input::Float(f64::INFINITY),
    ] {
        assert!(js_retained_truthy(&value));
    }
}
