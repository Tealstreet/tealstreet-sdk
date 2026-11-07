use safe_cex_num::js_date_to_iso_string;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn date_iso_matches_native_js_time_clip_and_extended_years() {
    let mut values = vec![
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -0.0,
        0.9,
        -0.9,
        1.0,
        -1.0,
        951782400123.0,
        -62167219200000.0,
        -62167219200001.0,
        253402300799999.0,
        253402300800000.0,
        8.64e15,
        8.64e15 - 1.0,
        8.64e15 + 1.0,
        -8.64e15,
        -8.64e15 + 1.0,
        -8.64e15 - 1.0,
    ];
    let mut bits = 0x4bf065c9ca18e721u64;
    for _ in 0..512 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        values.push(f64::from_bits(bits));
        values.push((bits as i64 % 8_640_000_000_000_004) as f64 + 0.75);
    }
    assert_eq!(values.len(), 1044, "native ISO input census");
    let inputs: Vec<_> = values
        .iter()
        .map(|n| format!("{:016x}", n.to_bits()))
        .collect();
    let mut node = Command::new("node").args(["-e", r#"
const inputs=JSON.parse(require('fs').readFileSync(0,'utf8'));
const rows=inputs.map(hex=>{const n=Buffer.from(hex,'hex').readDoubleBE();
try{return {value:new Date(n).toISOString()};}catch(e){return {error:{name:e.name,message:e.message}};}});
process.stdout.write(JSON.stringify({rows,count:inputs.length}));
"#]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    node.stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&inputs).unwrap())
        .unwrap();
    let output = node.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected["count"], 1044, "actual JS ISO corpus census");
    let rows = expected["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1044, "source output cardinality before pairing");
    let mut compared = 0;
    for (value, row) in values.into_iter().zip(rows) {
        let actual = match js_date_to_iso_string(value) {
            Ok(value) => json!({"value":value}),
            Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
        };
        assert_eq!(
            &actual,
            row,
            "native Date ISO bits {:016x}",
            value.to_bits()
        );
        compared += 1;
    }
    assert_eq!(compared, 1044, "completed native ISO comparisons");
}
