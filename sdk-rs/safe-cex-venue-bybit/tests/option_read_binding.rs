//! Option reads through the constructed venue, receiver and signed HTTP client.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Market};
use serde_json::{Value, json};
use support::{harness_as, options};

fn market(symbol: &str, underlying: &str, expiry: f64, strike: f64, right: &str) -> Market {
    serde_json::from_value(json!({
        "id":symbol,"symbol":symbol,"base":underlying,"quote":"USDT","active":true,
        "precision":{"amount":0.01,"price":0.1},
        "limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":1}},
        "option":{"underlying":underlying,"expiry":expiry,"strike":strike,"right":right,
            "style":"european","settlement":"cash","premiumAsset":"USDT","settleAsset":"USDT"}
    }))
    .unwrap()
}
fn seed(h: &support::Harness) {
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(vec![
                market("put", "BTC", 2000., 100., "put"),
                market("call", "BTC", 2000., 100., "call"),
                market("later", "BTC", 3000., 50., "call"),
                market("earlier", "BTC", 1000., 200., "call"),
                market("eth", "ETH", 500., 10., "call"),
                market("unquoted", "BTC", 1000., 50., "call"),
            ]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn quote(symbol: &str, bid: &str) -> Value {
    json!({"symbol":symbol,"bid1Price":bid,"ask1Price":"2.5suffix","markPrice":"Infinity",
        "markIv":"0.4","bid1Iv":"0.3","ask1Iv":"0.5","underlyingPrice":"60000",
        "delta":"-0","gamma":null,"vega":"0x10","theta":"-3"})
}
fn chain(name: ExchangeName, dynamic: bool) {
    let mut opts = options();
    opts.enable_option_markets = Some(true);
    let h = harness_as(name, opts, |wire| {
        wire.ok(
            "GET",
            "/v5/market/tickers",
            json!({"list":[
                quote("call","1"),quote("put","2"),quote("earlier","3"),
                quote("later","4"),quote("eth","5"),quote("call","9")
            ]}),
        );
    });
    seed(&h);
    let query = serde_json::from_value(json!({"underlying":"BTC"})).unwrap();
    let venue = h.venue();
    let rows = h
        .drive(if dynamic {
            venue.capabilities().fetch_option_chain(venue, query)
        } else {
            venue.fetch_option_chain(query)
        })
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.market.symbol.as_str())
            .collect::<Vec<_>>(),
        vec!["earlier", "call", "put", "later"]
    );
    let q = &rows[1].quote;
    assert_eq!(
        (q.bid, q.ask, q.mark, q.mark_iv, q.underlying_price),
        (9., 2.5, 0., 0.4, 60000.)
    );
    let g = q.greeks.as_ref().unwrap();
    assert_eq!(g.delta.to_bits(), (-0.0f64).to_bits());
    assert_eq!((g.gamma, g.vega, g.theta), (0., 0., -3.));
    let calls = h.wire.calls("/v5/market/tickers");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["category"], "option");
    assert_eq!(calls[0].params["baseCoin"], "BTC");
    h.venue().dispose().unwrap();
}
fn expiries(name: ExchangeName, dynamic: bool) {
    let mut opts = options();
    opts.enable_option_markets = Some(true);
    let h = harness_as(name, opts, |_| {});
    seed(&h);
    assert_eq!(
        h.drive(if dynamic {
            h.venue()
                .capabilities()
                .fetch_option_expiries(h.venue(), "BTC".into())
        } else {
            h.venue().fetch_option_expiries("BTC".into())
        })
        .unwrap(),
        vec![1000., 2000., 3000.]
    );
    assert_eq!(
        h.drive(if dynamic {
            h.venue()
                .capabilities()
                .fetch_option_expiries(h.venue(), "SOL".into())
        } else {
            h.venue().fetch_option_expiries("SOL".into())
        })
        .unwrap(),
        Vec::<f64>::new()
    );
    assert!(h.wire.calls("/v5/market/tickers").is_empty());
    h.venue().dispose().unwrap();
}
fn greeks(name: ExchangeName, dynamic: bool) {
    let mut opts = options();
    opts.enable_option_markets = Some(true);
    let h = harness_as(name, opts, |wire| {
        wire.ok("GET","/v5/asset/coin-greeks",json!({"list":[{
            "baseCoin":"BTC","totalDelta":"-0","totalGamma":"1.5x","totalVega":null,"totalTheta":"Infinity"
        }]}));
    });
    let venue = h.venue();
    let rows = h
        .drive(if dynamic {
            venue.capabilities().fetch_account_greeks(venue)
        } else {
            venue.fetch_account_greeks()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].underlying, "BTC");
    assert_eq!(rows[0].delta.to_bits(), (-0.0f64).to_bits());
    assert_eq!((rows[0].gamma, rows[0].vega, rows[0].theta), (1.5, 0., 0.));
    assert_eq!(h.wire.calls("/v5/asset/coin-greeks").len(), 1);
    h.venue().dispose().unwrap();
}
#[test]
fn primary_chain_joins_live_markets_and_quotes() {
    chain(ExchangeName::Bybit, false);
}
#[test]
fn demo_chain_joins_live_markets_and_quotes() {
    chain(ExchangeName::Bybitdemo, false);
}
#[test]
fn primary_expiries_read_live_markets() {
    expiries(ExchangeName::Bybit, false);
}
#[test]
fn demo_expiries_read_live_markets() {
    expiries(ExchangeName::Bybitdemo, false);
}
#[test]
fn primary_account_greeks_read_signed_http() {
    greeks(ExchangeName::Bybit, false);
}
#[test]
fn demo_account_greeks_read_signed_http() {
    greeks(ExchangeName::Bybitdemo, false);
}

#[test]
fn primary_options_require_account_opt_in() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        let h = harness_as(name, options(), |_| {});
        let query = serde_json::from_value(json!({"underlying":"BTC"})).unwrap();
        assert_eq!(
            h.drive(h.venue().fetch_option_chain(query))
                .unwrap_err()
                .message,
            "Options are not enabled on this Bybit account"
        );
        assert_eq!(
            h.drive(h.venue().fetch_option_expiries("BTC".into()))
                .unwrap_err()
                .message,
            "Options are not enabled on this Bybit account"
        );
        assert_eq!(
            h.drive(h.venue().fetch_account_greeks())
                .unwrap_err()
                .message,
            "Options are not enabled on this Bybit account"
        );
        assert!(h.wire.calls("/v5/market/tickers").is_empty());
        assert!(h.wire.calls("/v5/asset/coin-greeks").is_empty());
        h.venue().dispose().unwrap();
    }
}
#[test]
fn dedicated_families_keep_source_options_refusals() {
    for (name, label) in [
        (ExchangeName::Bybitspot, "bybitspot"),
        (ExchangeName::Bybitinverse, "bybitinverse"),
    ] {
        let mut opts = options();
        opts.enable_option_markets = Some(true);
        let h = harness_as(name, opts, |_| {});
        let query = serde_json::from_value(json!({"underlying":"BTC"})).unwrap();
        assert_eq!(
            h.drive(h.venue().fetch_option_chain(query))
                .unwrap_err()
                .message,
            format!("Option chains are not supported on {label}")
        );
        assert_eq!(
            h.drive(h.venue().fetch_option_expiries("BTC".into()))
                .unwrap_err()
                .message,
            format!("Option expiries are not supported on {label}")
        );
        assert_eq!(
            h.drive(h.venue().fetch_account_greeks())
                .unwrap_err()
                .message,
            format!("Account greeks are not supported on {label}")
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn option_reads_match_maintained_methods_and_helpers() {
    use safe_cex_exchange::js_runtime::value::{Js, from_typed, to_json};
    fn encode(value: Js) -> Value {
        match value {
            Js::Number(n) => json!({"numberBits":format!("{:016x}",n.to_bits())}),
            Js::Array(_) => Value::Array(
                value
                    .array_values()
                    .unwrap()
                    .into_iter()
                    .map(encode)
                    .collect(),
            ),
            Js::Object(_) => Value::Object(
                value
                    .own_keys()
                    .into_iter()
                    .filter_map(|key| {
                        let field = value.get_key(&key).unwrap();
                        (!field.is_undefined())
                            .then(|| (String::from_utf16(&key).unwrap(), encode(field)))
                    })
                    .collect(),
            ),
            _ => to_json(&value).unwrap(),
        }
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/option-read-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 79);
    let mut failures = vec![];
    for (index, case) in cases.iter().enumerate() {
        let mut opts = options();
        opts.enable_option_markets = case["enabled"].as_bool();
        let path = if case["method"] == "fetchAccountGreeks" {
            "/v5/asset/coin-greeks"
        } else {
            "/v5/market/tickers"
        };
        let h = harness_as(ExchangeName::Bybit, opts, |wire| {
            let response=case.get("response").cloned().unwrap_or_else(||json!({"retCode":0,"result":{"list":case.get("list").cloned().unwrap_or(Value::Null)}}));
            wire.route("GET", path, 200, response);
        });
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                markets: Some(serde_json::from_value(case["markets"].clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let result = match case["method"].as_str().unwrap() {
            "fetchOptionChain" => h
                .drive(
                    h.venue()
                        .fetch_option_chain(serde_json::from_value(case["query"].clone()).unwrap()),
                )
                .map(|rows| from_typed(&rows).unwrap()),
            "fetchOptionExpiries" => h
                .drive(
                    h.venue()
                        .fetch_option_expiries(case["query"].as_str().unwrap().into()),
                )
                .map(|rows| from_typed(&rows).unwrap()),
            "fetchAccountGreeks" => h
                .drive(h.venue().fetch_account_greeks())
                .map(|rows| from_typed(&rows).unwrap()),
            _ => unreachable!(),
        };
        match result {
            Ok(result) => {
                let actual = encode(result);
                if actual != case["result"] {
                    failures.push(format!(
                        "case{index} result {actual}, expected {}",
                        case["result"]
                    ));
                }
            }
            Err(error) => {
                if error.name != case["error"]["name"].as_str().unwrap_or("")
                    || error.message != case["error"]["message"].as_str().unwrap_or("")
                {
                    failures.push(format!(
                        "case{index} {}: {}; expected {}",
                        error.name, error.message, case["error"]
                    ));
                }
            }
        }
        let calls = h.wire.calls(path);
        assert_eq!(
            calls.len(),
            case["calls"].as_array().unwrap().len(),
            "case{index} request count"
        );
        if let Some(call) = calls.first() {
            if case["method"] == "fetchOptionChain" {
                assert_eq!(call.params["category"], "option");
                assert_eq!(call.params["baseCoin"], case["query"]["underlying"]);
            } else {
                assert_eq!(
                    call.params
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    vec!["requestId"],
                    "case{index}"
                );
                assert!(
                    call.params["requestId"]
                        .as_str()
                        .is_some_and(|id| !id.is_empty())
                );
            }
        }
        h.venue().dispose().unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dynamic_option_chain_uses_live_capabilities() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        chain(name, true);
    }
}
#[test]
fn dynamic_option_expiries_use_live_capabilities() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        expiries(name, true);
    }
}
#[test]
fn dynamic_option_account_greeks_use_live_capabilities() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        greeks(name, true);
    }
}
