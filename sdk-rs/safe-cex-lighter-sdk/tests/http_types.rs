use safe_cex_lighter_sdk::{http_types::*, referral_types::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/http_types_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "SDK-HTTP maintained declarations: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn units(v: &Value) -> Vec<u16> {
    v["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_u64().unwrap() as u16)
        .collect()
}
fn optional_units(v: &Value) -> Option<Vec<u16>> {
    if v.get("undefined") == Some(&json!(true)) {
        None
    } else {
        Some(units(v))
    }
}
fn number(v: &Value) -> f64 {
    let s = v["number"].as_str().unwrap();
    if s == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap())
    }
}
fn optional_number(v: &Value) -> Option<f64> {
    if v.get("undefined") == Some(&json!(true)) {
        None
    } else {
        Some(number(v))
    }
}
fn equal_number(actual: f64, expected: &Value, label: &str) {
    let n = number(expected);
    assert!(
        if n.is_nan() {
            actual.is_nan()
        } else {
            actual.to_bits() == n.to_bits()
        },
        "SDK-HTTP native number {label}"
    );
}
fn settled<T>(mut f: std::pin::Pin<Box<dyn std::future::Future<Output = T>>>) -> T {
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture future did not settle"),
    }
}
#[test]
fn callable_declarations_keep_values_identity_and_optional_lists() {
    let c = corpus();
    for (name, param, result) in [
        ("RequestInterceptor", "config", "any | Promise<any>"),
        ("ResponseInterceptor", "response", "any | Promise<any>"),
        ("ErrorInterceptor", "error", "any"),
    ] {
        assert_eq!(
            c["schemas"][name],
            json!({"params":[[param,"any"]],"returns":result}),
            "SDK-HTTP source callable schema {name}"
        );
    }

    assert_eq!(
        c["callbacks"].as_array().unwrap().len(),
        12,
        "SDK-HTTP callback floor"
    );
    for case in c["callbacks"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        let variant = case["variant"].as_u64().unwrap();
        let value = Rc::new(RefCell::new(false));
        let failure = Rc::new(RefCell::new(17));
        let thrown = failure.clone();
        let function: RequestInterceptor<Rc<RefCell<bool>>, Rc<RefCell<i32>>> = Rc::new(move |v| {
            if variant == 3 {
                return Err(thrown.clone());
            }
            if variant == 2 {
                *v.borrow_mut() = true;
            }
            if variant == 1 && kind != "error" {
                Ok(CallbackReturn::Promise(Box::pin(async move { Ok(v) })))
            } else {
                Ok(CallbackReturn::Value(v))
            }
        });
        let other: RequestInterceptor<Rc<RefCell<bool>>, Rc<RefCell<i32>>> =
            Rc::new(|_| Ok(CallbackReturn::Value(Rc::new(RefCell::new(false)))));
        let config = if kind == "request" {
            Interceptors::new(
                Some(Rc::new(RefCell::new(vec![function.clone(), other.clone()]))),
                None,
                None,
            )
        } else if kind == "response" {
            Interceptors::new(
                None,
                Some(Rc::new(RefCell::new(vec![function.clone(), other.clone()]))),
                None,
            )
        } else {
            let delegated = function.clone();
            let first: ErrorInterceptor<_, _> =
                Rc::new(move |v| delegated(v)?.value().map_err(|_| Rc::new(RefCell::new(99))));
            let second: ErrorInterceptor<_, _> = Rc::new(|_| Ok(Rc::new(RefCell::new(false))));
            let array = Rc::new(RefCell::new(vec![first.clone(), second]));
            let cfg = Interceptors::new(None, None, Some(array.clone()));
            let picked = cfg.first_error().unwrap();
            assert!(
                Rc::ptr_eq(&picked, &first),
                "SDK-HTTP callback identity error {variant}"
            );
            assert!(
                Rc::ptr_eq(cfg.error.as_ref().unwrap(), &array),
                "SDK-HTTP callback array error {variant}"
            );
            let outcome = picked(value.clone());
            check_callback(case, outcome.map(CallbackReturn::Value), &value, &failure);
            continue;
        };
        let array = if kind == "request" {
            config.request.as_ref().unwrap()
        } else {
            config.response.as_ref().unwrap()
        };
        let alias = array.clone();
        let picked = if kind == "request" {
            config.first_request().unwrap()
        } else {
            config.first_response().unwrap()
        };
        assert!(
            Rc::ptr_eq(&picked, &function),
            "SDK-HTTP callback identity {kind} {variant}"
        );
        alias.borrow_mut().push(function.clone());
        assert_eq!(
            array.borrow().len(),
            3,
            "SDK-HTTP live callback array {kind}"
        );
        check_callback(case, picked(value.clone()), &value, &failure);
    }
}
#[test]
fn interceptor_configuration_preserves_optional_field_arrays() {
    let c = corpus();
    assert_eq!(
        c["schemas"]["Interceptors"],
        json!([["request",true,{"array":"RequestInterceptor"}],["response",true,{"array":"ResponseInterceptor"}],["error",true,{"array":"ErrorInterceptor"}]]),
        "SDK-HTTP source optional lists schema"
    );
    assert_eq!(
        c["configs"].as_array().unwrap().len(),
        4,
        "SDK-HTTP config floor"
    );
    for (i, source) in c["configs"].as_array().unwrap().iter().enumerate() {
        let make = |name: &str| {
            source.get(name).map(|n| {
                Rc::new(RefCell::new(
                    (0..n.as_u64().unwrap())
                        .map(|_| {
                            let f: RequestInterceptor<i32, i32> =
                                Rc::new(|v| Ok(CallbackReturn::Value(v)));
                            f
                        })
                        .collect::<Vec<_>>(),
                ))
            })
        };
        let error = source.get("error").map(|n| {
            Rc::new(RefCell::new(
                (0..n.as_u64().unwrap())
                    .map(|_| {
                        let f: ErrorInterceptor<i32, i32> = Rc::new(Ok);
                        f
                    })
                    .collect::<Vec<_>>(),
            ))
        });
        let cfg = Interceptors::new(make("request"), make("response"), error);
        for (name, count) in [
            ("request", cfg.request.as_ref().map(|v| v.borrow().len())),
            ("response", cfg.response.as_ref().map(|v| v.borrow().len())),
            ("error", cfg.error.as_ref().map(|v| v.borrow().len())),
        ] {
            assert_eq!(
                count,
                source.get(name).map(|v| v.as_u64().unwrap() as usize),
                "SDK-HTTP config count {name} {i}"
            );
        }
    }
}
fn check_callback(
    case: &Value,
    outcome: Result<CallbackReturn<Rc<RefCell<bool>>, Rc<RefCell<i32>>>, Rc<RefCell<i32>>>,
    value: &Rc<RefCell<bool>>,
    failure: &Rc<RefCell<i32>>,
) {
    let kind = case["kind"].as_str().unwrap();
    let variant = case["variant"].as_u64().unwrap();
    assert_eq!(case["sameCallback"], true);
    assert_eq!(case["sameList"], true);
    if variant == 3 {
        assert!(
            Rc::ptr_eq(&outcome.err().unwrap(), failure),
            "SDK-HTTP original callback throw {kind}"
        );
        assert_eq!(case["output"]["thrownSame"], true);
    } else {
        let (returned, promise) = match outcome.ok().unwrap() {
            CallbackReturn::Value(v) => (v, false),
            CallbackReturn::Promise(f) => (settled(f).ok().unwrap(), true),
        };
        assert!(
            Rc::ptr_eq(&returned, value),
            "SDK-HTTP original callback value {kind}"
        );
        assert_eq!(case["output"]["same"], true);
        assert_eq!(
            promise,
            case["output"]["promise"].as_bool().unwrap(),
            "SDK-HTTP callback promise {kind}"
        );
        assert_eq!(
            *value.borrow(),
            case["output"]["changed"].as_bool().unwrap(),
            "SDK-HTTP callback mutation {kind}"
        );
    }
}
#[test]
fn api_error_fields_and_original_details_match_actual_constructor() {
    let c = corpus();
    assert_eq!(
        c["errors"].as_array().unwrap().len(),
        4,
        "SDK-HTTP constructor floor"
    );
    for case in c["errors"].as_array().unwrap() {
        let input = &case["input"];
        let source = &case["output"];
        let details = Rc::new(RefCell::new(7));
        let mut native = LighterApiError::new(
            units(&input["message"]),
            optional_units(&input["code"]),
            if input["detailsPresent"] == true {
                Some(details.clone())
            } else {
                None
            },
            optional_units(&input["requestId"]),
            optional_number(&input["statusCode"]),
        );
        assert_eq!(native.name, "LighterApiError", "SDK-HTTP native error name");
        assert_eq!(
            source["name"]["units"],
            json!(native.name.encode_utf16().collect::<Vec<_>>())
        );
        assert_eq!(
            native.message,
            units(&source["message"]),
            "SDK-HTTP native message"
        );
        assert_eq!(native.code, units(&source["code"]), "SDK-HTTP native code");
        assert_eq!(
            native.request_id,
            optional_units(&source["requestId"]),
            "SDK-HTTP native request ID"
        );
        match native.status_code {
            Some(n) => equal_number(n, &source["statusCode"], "statusCode"),
            None => assert_eq!(source["statusCode"]["undefined"], true),
        }
        if input["detailsPresent"] == true {
            assert!(
                Rc::ptr_eq(native.details.as_ref().unwrap(), &details),
                "SDK-HTTP original details"
            );
            *details.borrow_mut() = 8;
            assert_eq!(
                *native.details.take().unwrap().borrow(),
                source["liveCount"]["number"]
                    .as_str()
                    .map(|_| number(&source["liveCount"]) as i32)
                    .unwrap(),
                "SDK-HTTP live details"
            );
        } else {
            assert!(native.details.is_none(), "SDK-HTTP omitted details");
        }
        assert_eq!(source["detailsSame"], true);
        assert_eq!(
            case["isError"], true,
            "SDK-HTTP source inheritance boundary"
        );
        assert_eq!(
            case["messageEnumerable"], false,
            "SDK-HTTP source descriptor boundary"
        );
    }
    assert_eq!(c["errors"][0]["trace"].as_array().unwrap().len(), 0);
    for case in &c["errors"].as_array().unwrap()[1..] {
        assert_eq!(
            case["trace"].as_array().unwrap().len(),
            1,
            "SDK-HTTP source capture call"
        );
        assert_eq!(
            case["trace"][0]["constructorName"]["units"],
            json!("LighterApiError".encode_utf16().collect::<Vec<_>>())
        );
    }
    assert_eq!(
        c["failures"][0]["marker"], 17,
        "SDK-HTTP original source capture fault"
    );
    assert_eq!(
        c["failures"][1]["name"], "TypeError",
        "SDK-HTTP truthy noncallable capture boundary"
    );
    let native =
        LighterApiError::<()>::new("message".encode_utf16().collect(), None, None, None, None);
    assert_eq!(
        native.name, "LighterApiError",
        "SDK-HTTP native constructor lacks V8 hook"
    );
}
#[test]
fn referral_records_retain_native_values_and_shared_nested_objects() {
    let c = corpus();
    assert_eq!(
        c["schemas"]["ReferralPoints"],
        json!([["accountIndex",false,"string"],["totalPoints",false,"number"],["availablePoints",true,"number"],["pendingPoints",true,"number"],["referralCount",true,"number"],["tier",true,"string"],["rewards",true,[["totalUsd",true,"string"],["byToken",true,{"array":[["token",false,"string"],["amount",false,"string"]]}]]]]),
        "SDK-HTTP source points schema"
    );
    assert_eq!(
        c["schemas"]["ReferralPointsResponse"],
        json!([["referralPoints", false, "ReferralPoints"]]),
        "SDK-HTTP source response schema"
    );
    assert_eq!(
        c["points"].as_array().unwrap().len(),
        4,
        "SDK-HTTP points floor"
    );
    for (i, case) in c["points"].as_array().unwrap().iter().enumerate() {
        let p = &case["point"];
        let mut native = ReferralPoints::new(units(&p["accountIndex"]), number(&p["totalPoints"]));
        assert_eq!(
            native.account_index,
            units(&p["accountIndex"]),
            "SDK-HTTP native account {i}"
        );
        equal_number(native.total_points, &p["totalPoints"], "totalPoints");
        native.available_points = p.get("availablePoints").map(number);
        native.pending_points = p.get("pendingPoints").map(number);
        native.referral_count = p.get("referralCount").map(number);
        native.tier = p.get("tier").map(units);
        for (name, n) in [
            ("availablePoints", native.available_points),
            ("pendingPoints", native.pending_points),
            ("referralCount", native.referral_count),
        ] {
            if let Some(n) = n {
                equal_number(n, &p[name], name)
            } else {
                assert!(p.get(name).is_none())
            }
        }
        let mut original_rewards = None;
        let mut original_tokens = None;
        if let Some(r) = p.get("rewards") {
            let token_array = r.get("byToken").map(|a| {
                Rc::new(RefCell::new(
                    a.as_array()
                        .unwrap()
                        .iter()
                        .map(|t| ReferralRewardToken {
                            token: units(&t["token"]),
                            amount: units(&t["amount"]),
                        })
                        .collect::<Vec<_>>(),
                ))
            });
            let rewards = Rc::new(RefCell::new(ReferralRewards {
                total_usd: optional_units(&r["totalUsd"]),
                by_token: token_array.clone(),
            }));
            native.rewards = Some(rewards.clone());
            original_rewards = Some(rewards);
            original_tokens = token_array;
        }
        let original = Rc::new(RefCell::new(native));
        let response = ReferralPointsResponse::new(original.clone());
        assert!(
            Rc::ptr_eq(&response.referral_points, &original),
            "SDK-HTTP response original identity {i}"
        );
        assert_eq!(case["same"], true);
        equal_number(
            response.referral_points.borrow().total_points,
            &p["totalPoints"],
            "response totalPoints",
        );
        if let Some(rewards) = original_rewards {
            assert!(
                Rc::ptr_eq(
                    response.referral_points.borrow().rewards.as_ref().unwrap(),
                    &rewards
                ),
                "SDK-HTTP rewards identity"
            );
            assert_eq!(
                rewards.borrow().total_usd,
                optional_units(&p["rewards"]["totalUsd"]),
                "SDK-HTTP total USD"
            );
            if let Some(tokens) = original_tokens {
                assert!(
                    Rc::ptr_eq(rewards.borrow().by_token.as_ref().unwrap(), &tokens),
                    "SDK-HTTP tokens identity"
                );
                let source = p["rewards"]["byToken"].as_array().unwrap();
                assert_eq!(
                    tokens.borrow().len(),
                    source.len(),
                    "SDK-HTTP token cardinality"
                );
                for (j, t) in tokens.borrow().iter().enumerate() {
                    assert_eq!(
                        t.token,
                        units(&source[j]["token"]),
                        "SDK-HTTP native token {j}"
                    );
                    assert_eq!(
                        t.amount,
                        units(&source[j]["amount"]),
                        "SDK-HTTP native token amount {j}"
                    );
                }
                tokens.borrow_mut().push(ReferralRewardToken {
                    token: vec![0xd800],
                    amount: vec![49],
                });
                assert_eq!(
                    rewards.borrow().by_token.as_ref().unwrap().borrow().len(),
                    source.len() + 1,
                    "SDK-HTTP live token alias"
                );
            }
        } else {
            assert!(
                response.referral_points.borrow().rewards.is_none(),
                "SDK-HTTP omitted rewards"
            );
        }
        original.borrow_mut().account_index.push(0xdfff);
        assert_eq!(
            response.referral_points.borrow().account_index.last(),
            Some(&0xdfff),
            "SDK-HTTP live point alias"
        );
    }
}
