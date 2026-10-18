use safe_cex_exchange::js_runtime::value::{Thrown, from_json, to_json};
use safe_cex_num::JsCoercionError;
use serde::Deserialize;
use serde::Serialize;
mod support;
use safe_cex_venue_lighter::account_tier::*;
use serde_json::{Value, json};
use std::process::Command;

fn number(value: Option<&Value>) -> Option<f64> {
    value.map(|v| match v.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(other) => panic!("unknown tag {other}"),
        None => v.as_f64().unwrap(),
    })
}
fn tier_name(tier: LighterAccountTier) -> &'static str {
    match tier {
        LighterAccountTier::Standard => "standard",
        LighterAccountTier::Premium => "premium",
        LighterAccountTier::Plus => "plus",
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_account_tier_matches_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/tier-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [
        ("tiers", 3),
        ("normalized", 27),
        ("eligible", 4),
        ("payloads", 958),
        ("errors", 99),
        ("fees", 81),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "{name} scalar/opaque corpus inventory"
        );
    }

    for literal in oracle["tiers"].as_array().unwrap() {
        let invalid = serde_json::json!({literal.as_str().unwrap():null});
        assert!(
            LighterAccountTier::deserialize(invalid).is_err(),
            "LighterAccountTier public qualified decoder rejects object"
        );
    }

    support::assert_string_only::<LighterAccountTier>(&oracle["tiers"], "LighterAccountTier");

    assert_eq!(
        json!(support::enum_inventory::<LighterAccountTier>()),
        oracle["tiers"],
        "LighterAccountTier accepted wire inventory"
    );
    let tiers = [
        LighterAccountTier::Standard,
        LighterAccountTier::Premium,
        LighterAccountTier::Plus,
    ];
    assert_eq!(
        serde_json::to_value(tiers).unwrap(),
        oracle["tiers"],
        "LighterAccountTier"
    );
    for tier in tiers {
        assert_eq!(serde_json::to_value(tier).unwrap(), json!(tier_name(tier)));
    }
    assert_eq!(
        json!({
            "LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE": LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE,
            "LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE": LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE,
            "LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE": LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE,
        }),
        oracle["constants"],
        "account-tier messages"
    );
    for case in oracle["normalized"].as_array().unwrap() {
        assert_eq!(
            js_outcome(
                normalize_lighter_account_tier(case.get("input").map(from_json).as_ref())
                    .map(|value| value.map(tier_name))
            ),
            case["result"],
            "normalizeLighterAccountTier {case}"
        );
        let decoded = serde_json::from_value::<LighterAccountTier>(case["input"].clone());
        if let Ok(tier) = decoded.as_ref() {
            assert_eq!(
                Some(tier_name(*tier)),
                case["input"].as_str(),
                "LighterAccountTier native decoded identity {case}"
            );
        }
        assert_eq!(
            decoded.is_ok(),
            oracle["tiers"].as_array().unwrap().contains(&case["input"]),
            "LighterAccountTier exact decoding {case}"
        );
        let input = case.get("input").map(from_json);
        let record = as_record(input.as_ref());
        if let Some(record) = record {
            assert!(
                record.same_reference(input.as_ref().unwrap()),
                "asRecord retains its input"
            );
        }
        assert_eq!(
            record
                .map(to_json)
                .transpose()
                .unwrap()
                .unwrap_or(Value::Null),
            to_json(&from_json(&case["record"])).unwrap(),
            "asRecord native value identity {case}"
        );
    }
    for case in oracle["eligible"].as_array().unwrap() {
        let tier = case
            .get("tier")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        assert_eq!(
            is_lighter_account_tier_eligible_for_partner_attribution(tier),
            case["expected"].as_bool().unwrap(),
            "isLighterAccountTierEligibleForPartnerAttribution {case}"
        );
    }
    for case in oracle["payloads"].as_array().unwrap() {
        assert_eq!(
            js_outcome(
                extract_lighter_account_tier_from_payload(
                    case.get("input").map(from_json).as_ref()
                )
                .map(|value| value.map(tier_name))
            ),
            case["result"],
            "extractLighterAccountTierFromPayload {case}"
        );
    }
    for case in oracle["errors"].as_array().unwrap() {
        assert_eq!(
            outcome(is_lighter_partner_attribution_tier_rejection(
                case.get("code"),
                case.get("message")
            )),
            case["tier"],
            "isLighterPartnerAttributionTierRejection {case}"
        );
        assert_eq!(
            outcome(is_lighter_partner_attribution_approval_missing_rejection(
                case.get("code"),
                case.get("message")
            )),
            case["approval"],
            "isLighterPartnerAttributionApprovalMissingRejection {case}"
        );
    }
    for case in oracle["fees"].as_array().unwrap() {
        assert_eq!(
            has_non_zero_lighter_integrator_fee(
                number(case.get("integratorMakerFee")),
                number(case.get("integratorTakerFee"))
            ),
            case["expected"].as_bool().unwrap(),
            "hasNonZeroLighterIntegratorFee {case}"
        );
    }
}

fn outcome<T: Serialize>(result: Result<T, JsCoercionError>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    }
}

fn js_outcome<T: Serialize>(result: Result<T, Thrown>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => {
            json!({"error":{"name":error.value.get("name").unwrap().to_display(),"message":error.value.get("message").unwrap().to_display()}})
        }
    }
}

#[test]
fn native_account_tier_keeps_live_values_and_throw_identity() {
    use safe_cex_exchange::js_runtime::value::Js;
    use std::{cell::RefCell, rc::Rc};
    let source = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(["-e", r#"
const fs=require('node:fs'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync('packages/safe-cex/src/exchanges/lighter/lighter.account-tier.ts','utf8'),
 {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
const marker={sentinel:true};
const cases={nan:{user_tier_name:NaN,user_tier:'standard'},infinity:{user_tier_name:Infinity,user_tier:'standard'},
 undefined:{user_tier_name:undefined,user_tier:'premium'},null:{user_tier_name:null,user_tier:'plus'},
 surrogate:{user_tier_name:'\ud800',user_tier:'standard'},unrelated:{user_tier_name:'premium',extra:'\ud800'},
 symbol:{user_tier_name:Symbol.iterator,user_tier:'standard'},
 coercion:{user_tier_name:{toString(){return ' PREMIUM ';}}},
 'coercion-throw':{user_tier_name:{toString(){throw marker;}}},
 'getter-throw':{get data(){throw marker;}},cycle:{user_tier_name:'plus'}};
cases.cycle.extra=cases.cycle;
const trace=[];cases.trace={get data(){trace.push('data');return {get account(){trace.push('data.account');return null;}}},
 get account(){trace.push('account');return null;},get user_tier_name(){trace.push('alias');return NaN;},
 get user_tier(){throw new Error('later alias must not run');}};
const rows=Object.entries(cases).map(([label,input])=>{
 trace.length=0;
 try{return {label,expected:{tier:exports.extractLighterAccountTierFromPayload(input)??null,trace:[...trace]}};}
 catch(error){if(error!==marker)throw error;return {label,expected:{sameError:true,trace:[...trace]}};}
});process.stdout.write(JSON.stringify(rows));
"#]).output().expect("live account-tier source oracle");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let marker = Js::object(vec![("sentinel", Js::Bool(true))]);
        let mut payload = Js::object(vec![("user_tier", Js::str("standard"))]);
        let first = match row["label"].as_str().unwrap() {
            "nan" => Js::Number(f64::NAN),
            "infinity" => Js::Number(f64::INFINITY),
            "undefined" => {
                payload.set("user_tier", Js::str("premium")).unwrap();
                Js::Undefined
            }
            "null" => {
                payload.set("user_tier", Js::str("plus")).unwrap();
                Js::Null
            }
            "surrogate" => Js::String(vec![0xd800].into()),
            "unrelated" => {
                payload
                    .define("extra", Js::String(vec![0xd800].into()))
                    .unwrap();
                Js::str("premium")
            }
            "symbol" => Js::SymbolIterator,
            "coercion" => Js::object(vec![(
                "toString",
                Js::function("toString", |_, _| Ok(Js::str(" PREMIUM "))),
            )]),
            "coercion-throw" => {
                let marker = marker.clone();
                Js::object(vec![(
                    "toString",
                    Js::function("toString", move |_, _| Err(Thrown::new(marker.clone()))),
                )])
            }
            "getter-throw" => {
                let marker = marker.clone();
                payload
                    .define_accessor(
                        "data",
                        Some(Rc::new(move |_| Err(Thrown::new(marker.clone())))),
                        None,
                    )
                    .expect("fresh test accessor target");
                Js::Undefined
            }
            "cycle" => {
                payload.define("extra", payload.clone()).unwrap();
                Js::str("plus")
            }
            "trace" => {
                payload = Js::object(vec![]);
                let data = Js::object(vec![]);
                let seen = trace.clone();
                data.define_accessor(
                    "account",
                    Some(Rc::new(move |_| {
                        seen.borrow_mut().push("data.account".into());
                        Ok(Js::Null)
                    })),
                    None,
                )
                .expect("fresh test accessor target");
                let seen = trace.clone();
                payload
                    .define_accessor(
                        "data",
                        Some(Rc::new(move |_| {
                            seen.borrow_mut().push("data".into());
                            Ok(data.clone())
                        })),
                        None,
                    )
                    .expect("fresh test accessor target");
                let seen = trace.clone();
                payload
                    .define_accessor(
                        "account",
                        Some(Rc::new(move |_| {
                            seen.borrow_mut().push("account".into());
                            Ok(Js::Null)
                        })),
                        None,
                    )
                    .expect("fresh test accessor target");
                let seen = trace.clone();
                payload
                    .define_accessor(
                        "user_tier_name",
                        Some(Rc::new(move |_| {
                            seen.borrow_mut().push("alias".into());
                            Ok(Js::Number(f64::NAN))
                        })),
                        None,
                    )
                    .expect("fresh test accessor target");
                payload
                    .define_accessor(
                        "user_tier",
                        Some(Rc::new(|_| panic!("later alias must not run"))),
                        None,
                    )
                    .expect("fresh test accessor target");
                Js::Undefined
            }
            label => panic!("unknown source case {label}"),
        };
        if row["label"] != "trace" {
            payload.define("user_tier_name", first).unwrap();
        }
        assert!(as_record(Some(&payload)).unwrap().same_reference(&payload));
        let actual = match extract_lighter_account_tier_from_payload(Some(&payload)) {
            Ok(tier) => json!({"tier":tier.map(tier_name),"trace":*trace.borrow()}),
            Err(error) => {
                json!({"sameError":error.value.same_reference(&marker),"trace":*trace.borrow()})
            }
        };
        assert_eq!(actual, row["expected"], "{row}");
        payload.delete(&"extra".encode_utf16().collect::<Vec<_>>());
    }
}
