use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;

// A representable alternative: ordinary serde skips unknown fields without
// capturing their recursive contents. This is evidence of CHOSEN, not FORCED.
#[derive(Deserialize)]
struct Ignoring {}

pub fn audit<T: DeserializeOwned + Serialize + JsNumberOutput>(
    name: &str,
    base: Value,
    nested: bool,
) {
    // Compare depth-only output with the same decoder's shallow projection;
    // independent source/bit oracles validate declared field values.
    let projection = serde_json::to_value(T::deserialize(base.clone()).unwrap()).unwrap();
    audit_strings::<T>(name, &base, nested, &projection);
    let output = Command::new("node")
        .args([
            "-e",
            r#"
const assert=require('node:assert/strict');
const base=JSON.parse(process.argv[1]),nested=process.argv[2]==='true';
const rows=[0,2,125,126,127,140].map(depth=>{
 const value=structuredClone(base);let extra=17;
 for(let i=0;i<depth;i++)extra=[extra];
 (nested?value.opts:value).extra=extra;
 const encoded=JSON.stringify(value),parsed=JSON.parse(encoded);
 let leaf=(nested?parsed.opts:parsed).extra,count=0;
 while(Array.isArray(leaf)){assert.equal(leaf.length,1);count++;leaf=leaf[0];}
 assert.equal(count,depth);assert.equal(leaf,17);
 return {depth,encoded,count,leaf};
});process.stdout.write(JSON.stringify(rows));
"#,
            &base.to_string(),
            if nested { "true" } else { "false" },
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{name} Node unknown-field acceptance: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|w| w["depth"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![0, 2, 125, 126, 127, 140],
        "{name} unknown depth inventory"
    );
    for row in rows.as_array().unwrap() {
        let depth = row["depth"].as_u64().unwrap();
        assert_eq!(row["count"], row["depth"], "{name} source unknown depth");
        assert_eq!(row["leaf"], json!(17));
        let raw = row["encoded"].as_str().unwrap();
        let mut input = base.clone();
        let mut extra = json!(17);
        for _ in 0..depth {
            extra = Value::Array(vec![extra]);
        }
        if nested {
            input["opts"]["extra"] = extra;
        } else {
            input["extra"] = extra;
        }
        assert!(
            serde_json::from_str::<Ignoring>(raw).is_ok(),
            "{name} ordinary raw serde skips deep unknown fields"
        );
        assert!(
            Ignoring::deserialize(input.clone()).is_ok(),
            "{name} ordinary owned serde skips deep unknown fields"
        );
        assert!(
            Ignoring::deserialize(&input).is_ok(),
            "{name} ordinary borrowed serde skips deep unknown fields"
        );
        for (path, result) in [
            (
                "bare raw",
                serde_json::from_str::<T>(raw).map_err(|e| e.to_string()),
            ),
            (
                "bare owned",
                T::deserialize(input.clone()).map_err(|e| e.to_string()),
            ),
            (
                "bare borrowed",
                T::deserialize(&input).map_err(|e| e.to_string()),
            ),
            (
                "shared raw",
                js_from_str::<T>(raw).map_err(|e| e.to_string()),
            ),
            (
                "shared owned",
                js_from_value::<T>(input.clone()).map_err(|e| e.to_string()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<T>(&input).map_err(|e| e.to_string()),
            ),
        ] {
            let accepted = depth <= if nested { 125 } else { 126 };
            assert_eq!(
                result.is_ok(),
                accepted,
                "{name} {path} unknown-field depth {depth} nested={nested} acceptance cost"
            );
            match result {
                Ok(value) => assert_eq!(
                    serde_json::to_value(value).unwrap(),
                    projection,
                    "{name} accepted unknown field discarded"
                ),
                Err(error) => assert!(
                    error.contains("recursion limit"),
                    "{name} explicit depth refusal: {error}"
                ),
            }
        }
    }
}

fn audit_strings<T: DeserializeOwned + Serialize + JsNumberOutput>(
    name: &str,
    base: &Value,
    nested: bool,
    projection: &Value,
) {
    let output=Command::new("node").args(["-e",r#"
const assert=require('node:assert/strict'),base=JSON.parse(process.argv[1]),nested=process.argv[2]==='true';
const rows=['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
 const value=structuredClone(base);(nested?value.opts:value).extra=text;
 const encoded=JSON.stringify(value),parsed=JSON.parse(encoded),actual=(nested?parsed.opts:parsed).extra;
 assert.equal(actual,text);
 return {encoded,units:Array.from({length:actual.length},(_,i)=>actual.charCodeAt(i)),representable:text.isWellFormed()};
});process.stdout.write(JSON.stringify(rows));
"#,&base.to_string(),if nested {"true"} else {"false"}]).output().unwrap();
    assert!(
        output.status.success(),
        "{name} Node unknown-string oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|w| w["units"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!([]),
            json!([112, 108, 97, 105, 110]),
            json!([233]),
            json!([0xd83d, 0xdc69]),
            json!([0xfeff]),
            json!([0x85]),
            json!([0xd800]),
            json!([0xdfff]),
            json!([120, 0xd800, 121])
        ],
        "{name} unknown string UTF16 inventory"
    );
    for row in rows.as_array().unwrap() {
        let raw = row["encoded"].as_str().unwrap();
        assert!(
            serde_json::from_str::<Ignoring>(raw).is_ok(),
            "{name} ordinary raw serde skips unknown surrogate string"
        );
        for (path, result) in [
            (
                "bare raw",
                serde_json::from_str::<T>(raw).map_err(|e| e.to_string()),
            ),
            (
                "shared raw",
                js_from_str::<T>(raw).map_err(|e| e.to_string()),
            ),
        ] {
            assert!(
                result.is_ok(),
                "{name} {path} unknown-string admission nested={nested}: {}",
                row["units"]
            );
            let captured = typed_utf16::retained(raw);
            let extra = if nested {
                captured.get("opts").unwrap()
            } else {
                &captured
            }
            .get("extra")
            .unwrap();
            assert_eq!(
                json!(extra.as_text().unwrap()),
                row["units"],
                "{name} unknown field raw units remain exact"
            );
            if let Ok(value) = result {
                assert_eq!(
                    &serde_json::to_value(value).unwrap(),
                    projection,
                    "{name} accepted unknown string discarded"
                );
            }
        }
        // A serde Value cannot carry lone UTF16 surrogates. Do not describe
        // raw refusal as an owned/borrowed witness on an impossible Value.
    }
}

#[path = "../../../test-support/typed_utf16.rs"]
pub(crate) mod typed_utf16;
