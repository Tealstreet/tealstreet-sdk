#[path = "support/unknown_depth.rs"]
mod unknown_depth;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_lighter::{
    builder_approval::LighterBuilderApprovalFeeCaps,
    types::{LIGHTER_DEPLOYMENT_PROFILES, LighterDeploymentProfile},
    utils::{LighterCancelTarget, LighterOrderMinimums},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;

const NUMERIC_LEXEMES: [&str; 10] = [
    "-0",
    "-0.0",
    "-0e0",
    "0",
    "1.25",
    "1e3",
    "1e400",
    "-1e400",
    "-1e-4000",
    "9007199254740993",
];
const UNICODE_LEXEMES: [&str; 8] = [
    r#""plain""#,
    r#""é""#,
    r#""\ud83d\udc69""#,
    r#""\ufeff""#,
    r#""\u0085""#,
    r#""\ud800""#,
    r#""\udfff""#,
    r#""x\ud800y""#,
];
fn corpus() -> &'static Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let mut rows=Vec::new();
        for (name,base,fields) in [
            ("LighterDeploymentProfile",json!(LIGHTER_DEPLOYMENT_PROFILES[0].1),vec!["signerChainId"]),
            ("LighterBuilderApprovalFeeCaps",json!({"maxPerpsMakerFee":1,"maxPerpsTakerFee":2,"maxSpotMakerFee":3,"maxSpotTakerFee":4}),vec!["maxPerpsMakerFee","maxPerpsTakerFee","maxSpotMakerFee","maxSpotTakerFee"]),
            ("LighterCancelTarget",json!({"orderId":"fixture","marketId":1}),vec!["marketId"]),
            ("LighterOrderMinimums",json!({"minBaseAmount":1,"minQuoteAmount":2}),vec!["minBaseAmount","minQuoteAmount"]),
        ] {
            for field in fields { for lexeme in NUMERIC_LEXEMES {
                let mut input=base.clone(); input[field]=json!("__NUMBER__");
                let raw=input.to_string().replace("\"__NUMBER__\"",lexeme);
                rows.push(json!({"kind":"numeric","name":name,"field":field,"lexeme":lexeme,"raw":raw}));
            }}
        }
        for (name,base,fields) in [
            ("LighterCancelTarget",json!({"orderId":"fixture","marketId":-0.0}),vec!["orderId"]),
            ("LighterDeploymentProfile",json!(LIGHTER_DEPLOYMENT_PROFILES[0].1),vec!["apiBaseUrl","wsUrl"]),
        ] {
            for field in fields { for encoded in UNICODE_LEXEMES {
                let mut input=base.clone(); input[field]=json!("__STRING__");
                let raw=if name=="LighterCancelTarget" { format!(r#"{{"orderId":{encoded},"marketId":-0}}"#) } else { input.to_string().replace("\"__STRING__\"",encoded) };
                rows.push(json!({"kind":"string","name":name,"field":field,"lexeme":encoded,"raw":raw}));
            }}
        }
        let output=Command::new("node").args(["-e",r#"
const rows=JSON.parse(process.argv[1]);
for(const row of rows) {
 const value=JSON.parse(row.raw)[row.field];
 if(row.kind==='numeric') { const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);row.bits=Number.isNaN(value)?'7ff8000000000000':bytes.toString('hex'); }
 else { row.valid=value.isWellFormed();row.units=Array.from({length:value.length},(_,i)=>value.charCodeAt(i)); }
}
process.stdout.write(JSON.stringify(rows));
"#,&serde_json::to_string(&rows).unwrap()]).output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let capture:Value=serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(capture.as_array().unwrap().len(),104,"numeric ingress batched inventory");
        capture
    })
}
fn witness(name: &str, field: &str, lexeme: &str) -> &'static Value {
    corpus()
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name && row["field"] == field && row["lexeme"] == lexeme)
        .unwrap_or_else(|| panic!("missing corpus {name}.{field} {lexeme}"))
}

#[test]
#[ignore = "requires Node numeric oracle"]
fn existing_map_decoder_numeric_ingress_audit() {
    audit_repaired::<LighterDeploymentProfile>(
        "LighterDeploymentProfile",
        json!(LIGHTER_DEPLOYMENT_PROFILES[0].1),
        &["signerChainId"],
        |v, _| v.signer_chain_id,
    );
    audit_repaired::<LighterBuilderApprovalFeeCaps>(
        "LighterBuilderApprovalFeeCaps",
        json!({"maxPerpsMakerFee":1,"maxPerpsTakerFee":2,"maxSpotMakerFee":3,"maxSpotTakerFee":4}),
        &[
            "maxPerpsMakerFee",
            "maxPerpsTakerFee",
            "maxSpotMakerFee",
            "maxSpotTakerFee",
        ],
        |v, f| match f {
            "maxPerpsMakerFee" => v.max_perps_maker_fee,
            "maxPerpsTakerFee" => v.max_perps_taker_fee,
            "maxSpotMakerFee" => v.max_spot_maker_fee,
            "maxSpotTakerFee" => v.max_spot_taker_fee,
            _ => unreachable!(),
        },
    );
    audit_repaired::<LighterCancelTarget>(
        "LighterCancelTarget",
        json!({"orderId":"fixture","marketId":1}),
        &["marketId"],
        |v, _| v.market_id,
    );
    audit_repaired::<LighterOrderMinimums>(
        "LighterOrderMinimums",
        json!({"minBaseAmount":1,"minQuoteAmount":2}),
        &["minBaseAmount", "minQuoteAmount"],
        |v, f| match f {
            "minBaseAmount" => v.min_base_amount,
            "minQuoteAmount" => v.min_quote_amount,
            _ => unreachable!(),
        },
    );
}

fn audit_repaired<T: DeserializeOwned + Serialize + JsNumberOutput>(
    name: &str,
    base: Value,
    fields: &[&str],
    read: impl Fn(&T, &str) -> f64,
) {
    unknown_depth::audit::<T>(name, base.clone(), false);
    for field in fields {
        for lexeme in NUMERIC_LEXEMES {
            let mut raw = base.clone();
            raw[*field] = json!("__NUMBER__");
            let raw = serde_json::to_string(&raw)
                .unwrap()
                .replace("\"__NUMBER__\"", lexeme);
            let source = witness(name, field, lexeme);
            assert_eq!(source["raw"], raw, "{name}.{field} batched input identity");
            let bits = source["bits"].as_str().unwrap();
            let value: Value = serde_json::from_str(&raw).unwrap();
            for (path, decoded) in [
                ("bare raw", serde_json::from_str::<T>(&raw)),
                ("bare owned", serde_json::from_value::<T>(value.clone())),
                ("bare borrowed", T::deserialize(&value)),
                ("raw", js_from_str::<T>(&raw)),
                ("owned", js_from_value::<T>(value.clone())),
                ("borrowed", js_from_value_ref::<T>(&value)),
            ] {
                let decoded = decoded
                    .unwrap_or_else(|error| panic!("{name}.{field} {path} {lexeme}: {error}"));
                assert_eq!(
                    format!("{:016x}", read(&decoded, field).to_bits()),
                    bits,
                    "{name}.{field} {path} repaired bits {lexeme}"
                );
            }
            let bare_nested: Vec<T> = serde_json::from_str(&format!("[{raw}]")).unwrap();
            assert_eq!(
                format!("{:016x}", read(&bare_nested[0], field).to_bits()),
                bits,
                "{name}.{field} bare nested raw repaired bits {lexeme}"
            );
            let nested: Vec<T> = js_from_str(&format!("[{raw}]")).unwrap();
            assert_eq!(
                format!("{:016x}", read(&nested[0], field).to_bits()),
                bits,
                "{name}.{field} nested raw repaired bits {lexeme}"
            );
        }
        let mut native = base.clone();
        native[*field] = json!(-0.0);
        let decoded: T = js_from_value(native).unwrap();
        assert_eq!(
            read(&decoded, field).to_bits(),
            (-0.0f64).to_bits(),
            "{name}.{field} native signed zero"
        );
    }
    for bad in [
        json!([]),
        json!([1, 2, 3, 4]),
        json!(null),
        json!(false),
        json!(1),
        json!("record"),
    ] {
        assert!(
            js_from_value::<T>(bad).is_err(),
            "{name} canonical ingress rejects non-record"
        );
    }
}

#[test]
#[ignore = "requires Node string oracle"]
fn cancel_target_retains_scalar_unicode_and_refuses_lone_surrogates() {
    for encoded in UNICODE_LEXEMES {
        let source = witness("LighterCancelTarget", "orderId", encoded);
        let raw = source["raw"].as_str().unwrap();
        let parsed = js_from_str::<LighterCancelTarget>(raw);
        if source["valid"] == true {
            let value = parsed.unwrap();
            assert_eq!(
                json!(value.order_id.encode_utf16().collect::<Vec<_>>()),
                source["units"],
                "CancelTarget scalar Unicode retention"
            );
            assert_eq!(value.market_id.to_bits(), (-0.0f64).to_bits());
        } else {
            let value = parsed.unwrap();
            typed_utf16::assert_projection(raw, &value);
            let expected: Vec<u16> = serde_json::from_value(source["units"].clone()).unwrap();
            assert_eq!(value.order_id, String::from_utf16_lossy(&expected));
            assert_eq!(value.market_id.to_bits(), (-0.0f64).to_bits());
        }
    }
}

#[test]
#[ignore = "requires Node string oracle"]
fn deployment_profile_keeps_cow_urls_and_the_shared_string_boundary() {
    for (_, profile) in &LIGHTER_DEPLOYMENT_PROFILES {
        profile.check_number_output();
        assert!(matches!(
            profile.api_base_url,
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(matches!(profile.ws_url, std::borrow::Cow::Borrowed(_)));
        let decoded: LighterDeploymentProfile = js_from_value(json!(profile)).unwrap();
        assert_eq!(
            &decoded, profile,
            "borrowed constant and owned profile agree"
        );
        assert!(matches!(decoded.api_base_url, std::borrow::Cow::Owned(_)));
        assert!(matches!(decoded.ws_url, std::borrow::Cow::Owned(_)));
    }
    for encoded in UNICODE_LEXEMES {
        for field in ["apiBaseUrl", "wsUrl"] {
            let mut value = json!(LIGHTER_DEPLOYMENT_PROFILES[0].1);
            value[field] = json!("__STRING__");
            let raw = serde_json::to_string(&value)
                .unwrap()
                .replace("\"__STRING__\"", encoded);
            let source = witness("LighterDeploymentProfile", field, encoded);
            assert_eq!(
                source["raw"], raw,
                "DeploymentProfile.{field} batched input identity"
            );
            let parsed = js_from_str::<LighterDeploymentProfile>(&raw);
            if source["valid"] == true {
                let value = parsed.unwrap();
                let text = if field == "apiBaseUrl" {
                    value.api_base_url
                } else {
                    value.ws_url
                };
                assert_eq!(
                    json!(text.encode_utf16().collect::<Vec<_>>()),
                    source["units"],
                    "DeploymentProfile.{field} scalar Unicode retention"
                );
            } else {
                let value = parsed.unwrap();
                typed_utf16::assert_projection(&raw, &value);
                let text = if field == "apiBaseUrl" {
                    value.api_base_url
                } else {
                    value.ws_url
                };
                let expected: Vec<u16> = serde_json::from_value(source["units"].clone()).unwrap();
                assert_eq!(text, String::from_utf16_lossy(&expected));
            }
        }
    }
}

use unknown_depth::typed_utf16;
