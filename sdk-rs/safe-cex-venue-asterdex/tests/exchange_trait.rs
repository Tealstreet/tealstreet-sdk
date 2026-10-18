//! Concrete AsterDEX bindings on the shared runtime and scripted wire.
#![cfg(not(target_arch = "wasm32"))]
#[path = "support/retained_assert.rs"]
mod retained_assert;
#[path = "support/exchange_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_transport::RestRequest;
use safe_cex_types::{PlaceOrderOpts, PositionSide, UpdateOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::drive;

#[path = "support/exchange_data.rs"]
mod exchange_data;
use exchange_data::script;

#[test]
fn constructed_aster_rejection_keeps_native_string_code_units() {
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_exchange::{
        js_runtime::{
            client::JsHttpHost,
            promise::JsFuture,
            value::{Js, Thrown, error_object},
        },
        runtime::ExchangeRuntime,
    };
    use safe_cex_venue_asterdex::native_exchange::{AsterDEXDependencies, AsterDEXExchange};
    use std::cell::Cell;
    struct RejectingHost {
        runtime: Rc<support::Runtime>,
        code: Vec<u16>,
        calls: Rc<Cell<usize>>,
    }
    impl JsHttpHost for RejectingHost {
        fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
            self.runtime.clone()
        }
        fn send(&self, config: Js) -> JsFuture<'static> {
            let code = self.code.clone();
            let calls = self.calls.clone();
            let now = self.runtime.now.get();
            Box::pin(async move {
                if config.get("url")?.to_display().contains("/time") {
                    return Ok(Js::object(vec![
                        ("status", Js::Number(200.0)),
                        ("data", Js::object(vec![("serverTime", Js::Number(now))])),
                        ("config", config),
                    ]));
                }
                assert!(config.get("url")?.to_display().contains("/openOrders"));
                calls.set(calls.get() + 1);
                let error = error_object("AxiosError", "native Aster rejection");
                error.define("code", Js::utf16(&code))?;
                error.define("isAxiosError", Js::Bool(true))?;
                error.define("config", config)?;
                Err(Thrown::new(error))
            })
        }
    }
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rejection-code-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let codes: Vec<Vec<u16>> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(codes.len(), 3);
    for code in codes {
        let runtime = support::runtime_fixture();
        let calls = Rc::new(Cell::new(0));
        let host = Rc::new(RejectingHost {
            runtime: runtime.clone(),
            code: code.clone(),
            calls: calls.clone(),
        });
        let venue = AsterDEXExchange::new(
            safe_cex_types::ExchangeName::Asterdex,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            Rc::new(DefaultStore::new()),
            AsterDEXDependencies {
                runtime: runtime.clone(),
                http: Rc::new(move |_| Ok(host.clone())),
                external_public_data: None,
            },
        )
        .unwrap();
        let error = drive(&runtime, venue.fetch_orders(None)).unwrap_err();
        assert_eq!(
            calls.get(),
            4,
            "actual Aster read reaches terminal HTTP host with its three source retries"
        );
        let Some(Js::String(actual)) = error.code.as_ref() else {
            panic!(
                "native rejection code remains a JS string: {:?}",
                error.code
            );
        };
        assert_eq!(
            actual.as_ref(),
            code,
            "real Aster rejection retains exact code units"
        );
        let origin = error
            .origin
            .as_ref()
            .unwrap()
            .error()
            .downcast_ref::<safe_cex_transport::RestFailure>()
            .unwrap();
        assert_eq!(origin.request.method, "GET");
        assert!(origin.request.url.contains("/openOrders"));
        assert!(origin.response.is_none());
        venue.dispose().unwrap();
    }
}

#[test]
fn candle_reads_follow_source_fields_without_decoding_unused_tuple_columns() {
    use std::io::Write;
    let cases = json!([
        {"name":"six-columns","rows":[[1700000000000u64,"1","3","0.5","2","4"]]},
        {"name":"numeric-columns","rows":[[1700000000001u64,1,3,0.5,2,4]]},
        {"name":"opaque-unused-columns","rows":[[1700000000002u64,"1","3","0.5","2","4",null,{},[],false,{},null,"extra"]]},
        {"name":"missing-volume","rows":[[1700000000003u64,"1tail","3","0.5","2"]]},
        {"name":"indexed-record","rows":[{"0":1700000000004u64,"1":"1","2":"3","3":"0.5","4":"2","5":null}]}
    ]);
    assert_eq!(cases.as_array().unwrap().len(), 5);
    let mut child = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ohlcv-rows-source.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").into()
            }),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 10);
    for (version, offset) in [("v1", 0), ("v3", 5)] {
        let options = if version == "v3" {
            let secret = format!("{:064x}", 1);
            let wallet =
                safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&secret).unwrap();
            json!({"key":wallet.address().to_checksum(None),"secret":secret,"applicationId":"0x1111111111111111111111111111111111111111"})
        } else {
            json!({"key":"fixture-key","secret":"fixture-secret"})
        };
        let (venue, rt, _, wire) = support::fixture(options);
        script(&wire);
        let path = format!("/fapi/{version}/klines");
        for (index, case) in cases.as_array().unwrap().iter().enumerate() {
            let expected = &source[offset + index];
            assert_eq!(expected["version"], version);
            assert_eq!(expected["name"], case["name"]);
            wire.responses
                .borrow_mut()
                .insert(path.clone(), case["rows"].clone());
            let candles = drive(&rt, venue.fetch_ohlcv(serde_json::from_value(json!({
                "symbol":"BTCUSDT","interval":"1m","limit":2,"from":123000,"to":999000
            })).unwrap())).expect("real candle binding accepts source-readable rows without validating unused columns");
            assert_eq!(candles.len(), 1);
            let candle = &candles[0];
            for (field, value) in [
                ("timestamp", candle.timestamp),
                ("open", candle.open),
                ("high", candle.high),
                ("low", candle.low),
                ("close", candle.close),
                ("volume", candle.volume),
            ] {
                let wanted = &expected["candles"][0][field];
                if wanted == "NaN" {
                    assert!(value.is_nan(), "{version} {} {field}", case["name"]);
                } else {
                    assert_eq!(
                        Some(value),
                        wanted.as_f64(),
                        "{version} {} {field}",
                        case["name"]
                    );
                }
            }
            let requests = wire.requests.borrow();
            let request = requests.last().unwrap();
            assert_eq!(request_path(request), path);
            assert_eq!(expected["requests"][0]["url"], path);
            let query = signed_query_params(request);
            assert_eq!(query["symbol"], "BTCUSDT");
            assert_eq!(query["interval"], "1m");
            assert_eq!(query["limit"], "2");
            assert_eq!(query["startTime"], "123000");
            assert!(!query.contains_key("endTime"), "source ignores to");
        }
        venue.dispose().unwrap();
    }
}

#[test]
fn sparse_position_risk_rows_use_source_coercion_in_v1_and_v3() {
    for v3 in [false, true] {
        let options = if v3 {
            let secret = format!("{:064x}", 1);
            let wallet =
                safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&secret).unwrap();
            json!({"key":wallet.address().to_checksum(None),"secret":secret,
                "applicationId":"0x1111111111111111111111111111111111111111"})
        } else {
            json!({"key":"fixture-key","secret":"fixture-secret"})
        };
        let (venue, rt, _, wire) = support::fixture(options);
        script(&wire);
        wire.responses.borrow_mut().insert(
            "/fapi/v1/positionSide/dual".into(),
            json!({"dualSidePosition":true}),
        );
        if v3 {
            let mapped = wire
                .responses
                .borrow()
                .iter()
                .map(|(path, value)| {
                    (
                        path.replace("/v1/", "/v3/")
                            .replace("/v4/account", "/v3/account"),
                        value.clone(),
                    )
                })
                .collect::<Vec<_>>();
            wire.responses.borrow_mut().extend(mapped);
        }
        drive(&rt, venue.start()).unwrap();
        let path = if v3 {
            "/fapi/v3/positionRisk"
        } else {
            "/fapi/v1/positionRisk"
        };
        let sparse = json!({"symbol":"BTCUSDT","positionAmt":"2","entryPrice":"99",
            "unRealizedProfit":"6","leverage":20.0,"positionSide":"LONG",
            "isAutoAddMargin":false,"extraVenueField":{"retained":true}});
        wire.responses
            .borrow_mut()
            .insert(path.into(), json!([sparse.clone()]));
        let positions = drive(&rt, venue.fetch_positions(None))
            .expect("sparse positionRisk must reach the real position mapper");
        assert_eq!(positions.len(), 2);
        let long = positions
            .iter()
            .find(|p| p.side == PositionSide::Long)
            .unwrap();
        assert_eq!(long.contracts, 2.0);
        assert_eq!(long.entry_price, 99.0);
        assert!(
            long.notional.is_nan(),
            "missing notional keeps parseFloat(undefined)"
        );
        assert_eq!(long.leverage, 20.0);
        assert_eq!(long.liquidation_price, 0.0);
        // Native Float and captured JSON lexemes represent the same JS Number.
        // Check the retained fields and numeric bits before any JSON projection.
        retained_assert::assert_value(long.info.as_ref().unwrap(), &sparse);
        let short = positions
            .iter()
            .find(|p| p.side == PositionSide::Short)
            .unwrap();
        assert_eq!(short.contracts, 0.0);
        assert_eq!(
            short.leverage, 20.0,
            "numeric leverage reaches the placeholder"
        );

        // Flat and filtered rows still contribute leverage before the amount filter.
        wire.responses.borrow_mut().insert(
            path.into(),
            json!([
                {"symbol":"BTCUSDT","positionAmt":0,"leverage":7,"isAutoAddMargin":false},
                {"symbol":"OTHER","positionAmt":"1","leverage":3}
            ]),
        );
        let positions = drive(&rt, venue.fetch_positions(Some(vec!["BTCUSDT".into()]))).unwrap();
        assert_eq!(positions.len(), 2);
        assert!(
            positions
                .iter()
                .all(|p| p.contracts == 0.0 && p.leverage == 7.0)
        );
        venue.dispose().unwrap();
    }
}

fn request_path(request: &RestRequest) -> &str {
    request.url.split('?').next().unwrap()
}
fn wire_query(request: &RestRequest) -> &str {
    assert!(
        request.params.is_none(),
        "URL-consuming terminal gets rendered params"
    );
    request
        .url
        .split_once('?')
        .expect("signed query on the wire")
        .1
}
fn signed_query_params(request: &RestRequest) -> serde_json::Map<String, Value> {
    // These signature fixtures contain ASCII numbers, symbols and hex values.
    // Inspect raw wire text; refusing escapes avoids hiding a rewrite before HMAC.
    let query = wire_query(request);
    assert!(
        !query.contains('%') && !query.contains('+'),
        "unexpected escaping in signing fixture: {query}"
    );
    query
        .split('&')
        .map(|pair| {
            let (key, value) = pair.split_once('=').expect("query key/value");
            (key.into(), Value::String(value.into()))
        })
        .collect()
}
fn unsigned_wire_query(request: &RestRequest) -> (&str, &str) {
    wire_query(request)
        .rsplit_once("&signature=")
        .expect("signature appended last")
}

fn sent(wire: &support::Wire, method: &str, path: &str) -> bool {
    wire.requests
        .borrow()
        .iter()
        .any(|request| request.method.eq_ignore_ascii_case(method) && request_path(request) == path)
}
fn take(wire: &support::Wire) {
    wire.requests.borrow_mut().clear();
}

fn cancel_already_gone(status: u16) {
    let (venue, rt, _, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    script(&wire);
    let orders = drive(&rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    let errors = Rc::new(std::cell::RefCell::new(Vec::new()));
    let capture = errors.clone();
    venue.base().on(
        "error",
        Rc::new(move |_, args| {
            let args = safe_cex_exchange::js_runtime::value::to_json(args).unwrap();
            capture.borrow_mut().push(args.clone());
            Ok(())
        }),
    );
    wire.responses.borrow_mut().insert(
        "/fapi/v1/order".into(),
        json!({"code":-2011,"msg":"Unknown order sent."}),
    );
    wire.statuses
        .borrow_mut()
        .insert("/fapi/v1/order".into(), status);
    take(&wire);
    drive(&rt, venue.cancel_orders(orders)).unwrap();
    assert!(
        errors.borrow().is_empty(),
        "already-gone cancel must not emit an error: {:?}",
        errors.borrow()
    );
    assert!(
        rt.console
            .borrow()
            .iter()
            .any(|line| line.contains("was already gone"))
    );
    let requests = wire.requests.borrow();
    let cancel = requests
        .iter()
        .find(|r| r.method.eq_ignore_ascii_case("DELETE"))
        .unwrap();
    assert_eq!(
        cancel
            .config_extensions
            .get("__suppressAsterUnknownOrderError")
            .cloned()
            .map(|value| value.into_value().unwrap()),
        Some(json!(true))
    );
}

#[test]
fn already_gone_cancel_suppresses_success_response_errors() {
    cancel_already_gone(200);
}

#[test]
fn already_gone_cancel_suppresses_http_response_errors() {
    cancel_already_gone(400);
}

#[test]
fn all_sixteen_defaults_are_bound_through_the_constructed_exchange() {
    let (concrete, rt, store, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    let venue: Rc<dyn Exchange> = concrete.clone();
    script(&wire);
    // 1 start: actual source startup, Store writes and live socket constructors.
    drive(&rt, venue.start()).unwrap();
    assert!(
        store.loaded().unwrap().markets
            && store.loaded().unwrap().tickers
            && store.loaded().unwrap().orders
            && store.loaded().unwrap().positions
    );
    assert!(sent(&wire, "GET", "/fapi/v4/account"));
    take(&wire);
    // 2 get_account.
    assert_eq!(
        drive(&rt, venue.get_account()).unwrap().user_id,
        "aster-account"
    );
    assert!(sent(&wire, "GET", "/fapi/v4/account"));
    take(&wire);
    // 3 validate_account.
    assert_eq!(
        drive(&rt, venue.validate_account())
            .unwrap()
            .error
            .as_value(),
        &json!("")
    );
    assert!(sent(&wire, "GET", "/fapi/v4/account"));
    take(&wire);
    // 4 positions remain a read, while leverage metadata follows the source.
    let positions = drive(&rt, venue.fetch_positions(None)).unwrap();
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].side, PositionSide::Long);
    assert_eq!(positions[0].contracts.to_bits(), 2f64.to_bits());
    assert!(sent(&wire, "GET", "/fapi/v1/positionRisk"));
    take(&wire);
    // 5 orders.
    let orders = drive(&rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders[0].id, "9001");
    assert_eq!(orders[0].filled.to_bits(), 0.005f64.to_bits());
    assert!(sent(&wire, "GET", "/fapi/v1/openOrders"));
    take(&wire);
    // 6 markets: distinct price and amount precision.
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].precision.amount.to_bits(), 0.001f64.to_bits());
    assert_eq!(markets[0].precision.price.to_bits(), 0.01f64.to_bits());
    assert!(sent(&wire, "GET", "/fapi/v1/exchangeInfo"));
    assert!(sent(&wire, "GET", "/fapi/v1/leverageBracket"));
    take(&wire);
    // 7 tickers.
    let tickers = drive(&rt, venue.fetch_tickers()).unwrap();
    assert_eq!(tickers[0].last.to_bits(), 102f64.to_bits());
    assert!(sent(&wire, "GET", "/fapi/v1/ticker/24hr"));
    take(&wire);
    // 8 max leverage delegates to the actual market read.
    assert_eq!(
        drive(&rt, venue.fetch_symbol_max_leverage("BTCUSDT".into()))
            .unwrap()
            .to_bits(),
        100f64.to_bits()
    );
    assert!(sent(&wire, "GET", "/fapi/v1/exchangeInfo"));
    take(&wire);
    // 9 set leverage.
    drive(&rt, venue.set_leverage("BTCUSDT".into(), 5.4, None)).unwrap();
    assert!(sent(&wire, "POST", "/fapi/v1/leverage"));
    let request = wire
        .requests
        .borrow()
        .iter()
        .find(|r| request_path(r) == "/fapi/v1/leverage")
        .unwrap()
        .clone();
    assert_eq!(signed_query_params(&request)["leverage"], json!("5"));
    take(&wire);
    // 10 position mode.
    drive(&rt, venue.change_position_mode(true, None)).unwrap();
    assert!(store.options().unwrap().is_hedged);
    assert!(sent(&wire, "POST", "/fapi/v1/positionSide/dual"));
    take(&wire);
    // 11 margin type.
    drive(&rt, venue.change_margin_type(true, Some("BTCUSDT".into()))).unwrap();
    assert!(sent(&wire, "POST", "/fapi/v1/marginType"));
    let request = wire
        .requests
        .borrow()
        .iter()
        .find(|r| request_path(r) == "/fapi/v1/marginType")
        .unwrap()
        .clone();
    assert_eq!(
        signed_query_params(&request)["marginType"],
        json!("ISOLATED")
    );
    take(&wire);
    // 12 placement uses the first-statement client-ID contract and native payload.
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.012,"price":101.37}),
    )
    .unwrap();
    assert_eq!(
        drive(&rt, concrete.base().place_order(venue.as_ref(), opts)).unwrap(),
        ["1234567"]
    );
    let request = wire
        .requests
        .borrow()
        .iter()
        .find(|r| r.method == "post" && request_path(r) == "/fapi/v1/order")
        .unwrap()
        .clone();
    assert_eq!(signed_query_params(&request)["quantity"], json!("0.012"));
    assert_eq!(signed_query_params(&request)["price"], json!("101.25"));
    assert!(request.body.is_none());
    take(&wire);
    // 13 cancel+replace update.
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order":orders[0],"update":{"price":96.25}})).unwrap();
    assert_eq!(drive(&rt, venue.update_order(update)).unwrap(), ["1234567"]);
    assert!(sent(&wire, "DELETE", "/fapi/v1/order"));
    assert!(sent(&wire, "POST", "/fapi/v1/order"));
    take(&wire);
    // 14 cancellation.
    drive(&rt, venue.cancel_orders(orders)).unwrap();
    assert!(sent(&wire, "DELETE", "/fapi/v1/order"));
    take(&wire);
    // 15 inherited source rejection (base.ts:989).
    assert_eq!(
        drive(&rt, venue.schedule_cancel(None)).unwrap_err().message,
        "Not implemented"
    );
    assert!(wire.requests.borrow().is_empty());
    // 16 retain the already-declared closed OrderSide refusal of TS's uppercase cast.
    let error = drive(&rt, venue.fetch_public_trades("BTCUSDT".into(), Some(9.0))).unwrap_err();
    assert_eq!(error.name, "RepresentationError");
    assert!(error.message.contains("fetchPublicTrades side SELL"));
    assert!(sent(&wire, "GET", "/fapi/v1/trades"));
    venue.dispose().unwrap();
}

#[test]
fn secondary_reads_keep_their_key_secret_and_transport_options() {
    let (venue, rt, _, wire) = support::fixture(
        json!({"key":"write-key","secret":"write-secret","secondary_key":"read-key","secondary_secret":"read-secret","secondaryProxyUrl":"https://secondary.test","overrideProxyUrl":"https://primary.test"}),
    );
    script(&wire);
    drive(&rt, venue.get_account()).unwrap();
    let read = wire
        .requests
        .borrow()
        .iter()
        .find(|request| request_path(request) == "/fapi/v4/account")
        .unwrap()
        .clone();
    assert_eq!(
        read.headers.get("X-MBX-APIKEY").map(String::as_str),
        Some("read-key"),
        "GET uses secondary key"
    );
    let mut params = signed_query_params(&read);
    let signature = params.remove("signature").unwrap();
    let (query, wire_signature) = unsigned_wire_query(&read);
    assert_eq!(signature, json!(wire_signature));
    let expected = drive(
        &rt,
        safe_cex_venue_asterdex::api_helpers::generate_v1_signature(query, "read-secret"),
    )
    .unwrap();
    assert_eq!(
        signature,
        json!(expected),
        "GET signs with the secondary secret"
    );
    assert!(
        rt.http_options
            .borrow()
            .iter()
            .any(|options| options.key == "write-key"
                && options.override_proxy_url.as_deref() == Some("https://primary.test")),
        "primary API retains its account routing"
    );
    let module_options = rt.http_options.borrow()[0].clone();
    assert!(
        module_options.key.is_empty()
            && module_options.secret.is_empty()
            && module_options.override_proxy_url.is_none(),
        "module axios clock is independent of account credentials and private proxy routing"
    );
    assert!(
        rt.http_options
            .borrow()
            .iter()
            .any(|options| options.key == "read-key"
                && options.override_proxy_url.as_deref() == Some("https://secondary.test")),
        "secondary transport options reach the HTTP factory"
    );
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    venue
        .base()
        .store
        .update(
            safe_cex_store::StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    take(&wire);
    drive(&rt, venue.set_leverage("BTCUSDT".into(), 7.0, None)).unwrap();
    assert_eq!(
        wire.requests
            .borrow()
            .iter()
            .find(|r| request_path(r) == "/fapi/v1/leverage")
            .unwrap()
            .headers
            .get("X-MBX-APIKEY")
            .map(String::as_str),
        Some("write-key"),
        "POST keeps the primary key"
    );
    venue.dispose().unwrap();
}

#[test]
fn v3_clients_sign_both_the_abi_reads_and_exact_code_query_order_writes() {
    let secret = format!("{:064x}", 1);
    let wallet = safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&secret).unwrap();
    let signer = wallet.address().to_checksum(None);
    let user = "0x1111111111111111111111111111111111111111";
    let (venue, rt, _, wire) =
        support::fixture(json!({"key":signer,"secret":secret,"applicationId":user}));
    script(&wire);
    let mapped = wire
        .responses
        .borrow()
        .iter()
        .map(|(path, value)| {
            (
                path.replace("/v1/", "/v3/")
                    .replace("/v4/account", "/v3/account"),
                value.clone(),
            )
        })
        .collect::<Vec<_>>();
    wire.responses.borrow_mut().extend(mapped);
    drive(&rt, venue.start()).unwrap();
    let account = wire
        .requests
        .borrow()
        .iter()
        .find(|r| request_path(r) == "/fapi/v3/account")
        .unwrap()
        .clone();
    let params = signed_query_params(&account);
    assert_eq!(params["user"], json!(user));
    assert_eq!(params["signer"], json!(signer));
    assert_eq!(
        params["signature"].as_str().unwrap().len(),
        132,
        "ABI request gets a full recoverable signature"
    );
    let mut nonce = [0; 32];
    let observed_nonce = params["nonce"].as_str().unwrap().parse::<u128>().unwrap();
    nonce[16..].copy_from_slice(&observed_nonce.to_be_bytes());
    let unsigned=json!({"recvWindow":params["recvWindow"].as_str().unwrap(),"timestamp":params["timestamp"].as_str().unwrap()}).to_string();
    let encoded = safe_cex_crypto::ethereum::encode_string_address_address_uint256(
        &unsigned, user, &signer, &nonce,
    )
    .unwrap();
    let hash = safe_cex_crypto::ethereum::keccak256(&encoded);
    let digest = safe_cex_crypto::ethereum::ethereum_message_hash(&hash);
    assert_eq!(
        params["signature"],
        json!(safe_cex_crypto::ethereum::sign_digest(&digest, &secret).unwrap()),
        "ABI signing binds the exact timestamp/window payload, user, signer and observed nonce"
    );
    take(&wire);
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.012,"price":101.37}),
    )
    .unwrap();
    assert_eq!(
        drive(&rt, venue.base().place_order(venue.as_ref(), opts)).unwrap(),
        ["1234567"]
    );
    let request = wire
        .requests
        .borrow()
        .iter()
        .find(|r| r.method == "post" && r.url.starts_with("/fapi/v3/order?"))
        .unwrap()
        .clone();
    let (_, query) = request.url.split_once('?').unwrap();
    let (message, signature) = query.rsplit_once("&signature=").unwrap();
    assert!(message.starts_with("symbol=BTCUSDT&side=BUY&type=LIMIT&builder=0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca&feeRate=0.0001&quantity=0.012&timeInForce=GTC&price=101.25&asterChain=Mainnet&user="),"exact final Aster Code parameter order: {message}");
    assert_eq!(
        signature,
        safe_cex_venue_asterdex::api_helpers::generate_aster_dex_code_message_signature(
            message,
            &secret,
            Some(false)
        )
        .unwrap(),
        "signature covers the exact unencoded sent query"
    );
    assert!(
        request.body.is_none() && request.params.is_none(),
        "Code signing leaves no adapter query/body rewrite"
    );
    venue.dispose().unwrap();
}

#[test]
fn rejected_start_never_opens_sockets_and_caller_order_ids_fail_before_http() {
    let (venue, rt, _, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    script(&wire);
    wire.statuses
        .borrow_mut()
        .insert("/fapi/v4/account".into(), 401);
    wire.responses.borrow_mut().insert(
        "/fapi/v4/account".into(),
        json!({"msg":"credential refused"}),
    );
    let error = drive(&rt, venue.start()).unwrap_err();
    assert!(error.message.contains("credential refused"));
    assert!(
        rt.urls.borrow().is_empty(),
        "failed authentication sends no WebSocket subscriptions"
    );
    take(&wire);
    let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.012,"price":101.37,"clientOrderId":"caller-id"})).unwrap();
    assert!(drive(&rt, venue.base().place_order(venue.as_ref(), opts)).is_err());
    assert!(
        wire.requests.borrow().is_empty(),
        "client-order-ID contract precedes every placement effect"
    );
    venue.dispose().unwrap();
}

#[test]
fn constructed_exchange_starts_with_real_signed_clients_and_native_sockets() {
    let (venue, rt, store, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    assert!(
        store.loaded().unwrap().markets
            && store.loaded().unwrap().tickers
            && store.loaded().unwrap().positions
            && store.loaded().unwrap().orders
    );
    assert_eq!(store.markets().unwrap()[0].symbol, "BTCUSDT");
    assert_eq!(store.tickers().unwrap()[0].last.to_bits(), 102f64.to_bits());
    let account = drive(&rt, venue.get_account()).unwrap();
    assert_eq!(account.user_id, "aster-account");
    assert!(wire.requests.borrow().iter().any(|request| {
        request_path(request) == "/fapi/v4/account"
            && request
                .headers
                .get("X-MBX-APIKEY")
                .is_some_and(|key| key == "fixture-key")
            && signed_query_params(request).get("signature").is_some()
    }));
    rt.pump();
    assert!(
        rt.urls
            .borrow()
            .iter()
            .any(|url| url.ends_with("/ws/fixture-listen-key"))
    );
    venue.dispose().unwrap();
}

#[test]
fn v1_signature_rejection_resends_once_from_unsigned_parameters() {
    for status in [200, 400] {
        for repeated in [false, true] {
            let (venue, rt, _, wire) =
                support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
            script(&wire);
            let markets = drive(&rt, venue.fetch_markets()).unwrap();
            venue
                .base()
                .store
                .update(
                    safe_cex_store::StoreUpdate {
                        markets: Some(markets),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            take(&wire);
            let rejection = json!({"code":-1022,"msg":"Signature for this request is not valid"});
            wire.sequence.borrow_mut().insert(
                "/fapi/v1/order".into(),
                [
                    (status, rejection.clone()),
                    if repeated {
                        (status, rejection)
                    } else {
                        (200, json!({"orderId":1234567}))
                    },
                ]
                .into(),
            );
            let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.012,"price":101.37})).unwrap();
            let result = drive(&rt, venue.base().place_order(venue.as_ref(), opts));
            if repeated {
                assert!(result.is_err(), "second signature rejection surfaces");
            } else {
                assert_eq!(result.unwrap(), ["1234567"]);
            }
            let requests = wire
                .requests
                .borrow()
                .iter()
                .filter(|r| request_path(r) == "/fapi/v1/order")
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                requests.len(),
                2,
                "one resend for status {status}, repeated={repeated}"
            );
            for request in requests {
                let mut params = signed_query_params(&request);
                let signature = params.remove("signature").unwrap();
                assert_eq!(params["symbol"], json!("BTCUSDT"));
                assert_eq!(params["quantity"], json!("0.012"));
                assert_eq!(
                    params.len(),
                    8,
                    "no old signature is retained among unsigned order fields"
                );
                let (query, wire_signature) = unsigned_wire_query(&request);
                assert_eq!(signature, json!(wire_signature));
                let expected = drive(
                    &rt,
                    safe_cex_venue_asterdex::api_helpers::generate_v1_signature(
                        query,
                        "fixture-secret",
                    ),
                )
                .unwrap();
                assert_eq!(
                    signature,
                    json!(expected),
                    "each resend signs exactly the sent params"
                );
            }
            venue.dispose().unwrap();
        }
    }
}

#[test]
fn amendment_omits_undefined_side_on_the_signed_wire() {
    let (concrete, rt, _, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    let venue: Rc<dyn Exchange> = concrete;
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    let order = drive(&rt, venue.fetch_orders(None)).unwrap().remove(0);
    let mut row = serde_json::to_value(order).unwrap();
    row.as_object_mut().unwrap().remove("side");
    let opts = serde_json::from_value(json!({"order":row,"update":{"price":96}})).unwrap();
    take(&wire);
    assert_eq!(drive(&rt, venue.update_order(opts)).unwrap(), ["1234567"]);
    let requests = wire.requests.borrow();
    let placed = requests
        .iter()
        .find(|r| r.method.eq_ignore_ascii_case("POST") && request_path(r) == "/fapi/v1/order")
        .unwrap();
    assert!(!signed_query_params(placed).contains_key("side"));
    drop(requests);
    venue.dispose().unwrap();
}

#[test]
fn private_reopen_refreshes_orders_missed_during_disconnect() {
    let (venue, rt, store, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    assert_eq!(store.orders().unwrap()[0].id, "9001");
    let private = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/ws/fixture-listen-key"))
        .expect("production private socket")
        .1
        .clone();
    let mut changed = wire.responses.borrow()["/fapi/v1/openOrders"][0].clone();
    changed["orderId"] = json!(777);
    changed["clientOrderId"] = json!("after-reopen");
    wire.responses
        .borrow_mut()
        .insert("/fapi/v1/openOrders".into(), json!([changed]));
    let before = wire
        .requests
        .borrow()
        .iter()
        .filter(|r| request_path(r) == "/fapi/v1/openOrders")
        .count();
    let dials = rt.urls.borrow().len();
    private.closed.set(true);
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Close {
            code: Some(1006),
            reason: "connection lost".into(),
        });
    for _ in 0..60 {
        rt.now.set(rt.now.get() + 100.0);
        rt.pump();
    }
    assert!(
        rt.urls.borrow().len() > dials,
        "native private socket redialed"
    );
    let after = wire
        .requests
        .borrow()
        .iter()
        .filter(|r| request_path(r) == "/fapi/v1/openOrders")
        .count();
    assert!(
        after > before,
        "private reopen must perform the source snapshot refresh"
    );
    let rows = store.orders().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].id, "777",
        "snapshot replaces orders missed during disconnect"
    );
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    drop(rows);
    venue.dispose().unwrap();
}

fn stream_fixture() -> (
    Rc<safe_cex_venue_asterdex::native_exchange::AsterDEXExchange>,
    Rc<support::Runtime>,
    Rc<safe_cex_exchange::native_store::DefaultStore>,
    std::sync::Arc<support::Wire>,
) {
    let (venue, rt, store, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    (venue, rt, store, wire)
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_store::{Store, StoreUpdate};
    let corpus = ticker_coercion::source("asterdex");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 252);
    ticker_coercion::assert_case_counts(cases, 372, 12);
    let (venue, rt, store, _) = stream_fixture();
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.ends_with("/ws/fixture-listen-key"))
        .unwrap()
        .1
        .clone();
    for case in cases {
        Store::update(
            store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("asterdex")),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        rt.console.borrow_mut().clear();
        rt.errors.borrow_mut().clear();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .events
                .borrow_mut()
                .push_back(safe_cex_exchange::websocket::WsEvent::Message(
                    safe_cex_transport::socket::SocketFrame::Text(frame.as_str().unwrap().into()),
                ));
            for _ in 0..8 {
                rt.pump();
            }
        }
        ticker_coercion::assert_native(&store.tickers().unwrap(), case);
        ticker_coercion::assert_refusals(&rt.console.borrow().join("\n"), case);
        assert!(
            rt.errors.borrow().is_empty(),
            "native ticker {:?}",
            rt.errors.borrow()
        );
    }
    venue.dispose().unwrap();
}
#[test]
fn public_frames_preserve_partial_depth_and_candle_semantics() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::cell::RefCell;
    let (venue, rt, store, _) = stream_fixture();
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.ends_with("/ws/fixture-listen-key"))
        .unwrap()
        .1
        .clone();
    let books = Rc::new(RefCell::new(Vec::new()));
    let saved = books.clone();
    let close_book = venue
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |book| {
                saved.borrow_mut().push(book);
            }),
        )
        .unwrap();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let saved = candles.clone();
    let close_candle = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(move |candle| saved.borrow_mut().push(candle)),
        )
        .unwrap();
    let frame = |value: Value| {
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(value.to_string())));
        for _ in 0..8 {
            rt.pump();
        }
        assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    };
    assert!(
        rt.sent
            .borrow()
            .iter()
            .any(|v| v.contains("btcusdt@depth20@100ms"))
    );
    frame(json!({"stream":"btcusdt@depth@100ms","data":{"b":[["99","2"]],"a":[["101","4"]]}}));
    assert!(
        books.borrow().is_empty(),
        "incremental depth is not an authoritative snapshot"
    );
    frame(
        json!({"stream":"btcusdt@depth20@100ms","data":{"b":[["98","3"],["99","2"]],"a":[["102","5"],["101","4"]]}}),
    );
    assert_eq!(books.borrow().len(), 1);
    {
        let rows = books.borrow();
        assert_eq!(
            (
                rows[0].bids[0].price,
                rows[0].bids[1].total,
                rows[0].asks[0].price,
                rows[0].asks[1].total
            ),
            (99.0, 5.0, 101.0, 9.0)
        );
    }
    for bids in [
        json!([[99, "2"]]),
        json!([["99junk", "2"]]),
        json!([["99", "0"]]),
        json!([["102", "2"]]),
    ] {
        frame(json!({"stream":"btcusdt@depth20@100ms","data":{"b":bids,"a":[["101","4"]]}}));
    }
    assert_eq!(
        books.borrow().len(),
        1,
        "malformed or crossed snapshot keeps the last good book"
    );
    frame(
        json!({"stream":"!ticker@arr","data":[{"s":"BTCUSDT","c":"100.5","P":"2.5","v":"8","q":"900"}]}),
    );
    frame(
        json!({"stream":"!markPrice@arr@1s","data":[{"s":"BTCUSDT","p":"100.75","i":"100.25","r":"0","T":1700000100000u64}]}),
    );
    {
        let rows = store.tickers().unwrap();
        let ticker = rows.iter().find(|v| v.symbol == "BTCUSDT").unwrap();
        assert_eq!(
            (
                ticker.bid,
                ticker.ask,
                ticker.last,
                ticker.percentage,
                ticker.volume,
                ticker.quote_volume
            ),
            (99.0, 101.0, 100.5, 2.5, 8.0, 900.0)
        );
        assert_eq!(
            (
                ticker.mark,
                ticker.index.unwrap(),
                ticker.funding_rate,
                ticker.funding_time
            ),
            (100.75, 100.25, 0.0, 1700000100000.0)
        );
    }
    let candle = json!({"stream":"btcusdt@kline_1m","data":{"k":{"i":"1m","t":1700000000000u64,"o":"100","h":"104","l":"99","c":"103","v":"7","q":"999"}}});
    frame(candle.clone());
    assert_eq!(candles.borrow().len(), 1);
    assert_eq!(
        (
            candles.borrow()[0].timestamp,
            candles.borrow()[0].close,
            candles.borrow()[0].volume
        ),
        (1700000000000.0, 103.0, 7.0)
    );
    close_book().unwrap();
    close_candle().unwrap();
    frame(candle);
    frame(json!({"stream":"btcusdt@depth20@100ms","data":{"b":[["97","1"]],"a":[["103","2"]]}}));
    assert_eq!((books.borrow().len(), candles.borrow().len()), (1, 1));
    assert!(
        rt.sent
            .borrow()
            .iter()
            .any(|v| v.contains("UNSUBSCRIBE") && v.contains("btcusdt@depth20@100ms"))
    );
    venue.dispose().unwrap();
}

fn deliver_private(rt: &support::Runtime, frame: Value) {
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/ws/fixture-listen-key"))
        .unwrap()
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(frame.to_string()),
        ));
    for _ in 0..8 {
        rt.pump();
    }
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
}
fn order_frame(id: u64) -> Value {
    json!({"e":"ORDER_TRADE_UPDATE","o":{"i":id,"s":"BTCUSDT","S":"BUY","o":"LIMIT","X":"NEW","p":"95","sp":"0","q":"3","z":"0","R":false,"cp":false}})
}
#[test]
fn production_private_order_frame_updates_store() {
    let (venue, rt, store, _) = stream_fixture();
    deliver_private(&rt, order_frame(9050));
    let rows = store.orders().unwrap();
    let order = rows
        .iter()
        .find(|o| o.id == "9050")
        .expect("private order in real Store");
    assert_eq!(order.symbol, "BTCUSDT");
    assert_eq!(
        (
            order.price.to_number(),
            order.amount,
            order.filled,
            order.remaining
        ),
        (95.0, 3.0, 0.0, 3.0)
    );
    drop(rows);
    venue.dispose().unwrap();
}
#[test]
fn production_private_fill_frame_updates_store_and_emits_fill() {
    let (venue, rt, store, _) = stream_fixture();
    let fills = Rc::new(std::cell::RefCell::new(vec![]));
    let saved = fills.clone();
    venue.base().on(
        "fill",
        Rc::new(move |_, data| {
            let data = safe_cex_exchange::js_runtime::value::to_json(data).unwrap();
            saved.borrow_mut().push(data.clone());
            Ok(())
        }),
    );
    deliver_private(&rt, order_frame(9051));
    let mut frame = order_frame(9051);
    frame["o"]["X"] = json!("PARTIALLY_FILLED");
    frame["o"]["z"] = json!("0.25");
    frame["o"]["l"] = json!("0.25");
    frame["o"]["L"] = json!("96");
    frame["o"]["ap"] = json!("96");
    deliver_private(&rt, frame);
    let rows = store.orders().unwrap();
    let order = rows.iter().find(|o| o.id == "9051").unwrap();
    assert_eq!((order.filled, order.remaining), (0.25, 2.75));
    let fills = fills.borrow();
    assert_eq!(fills.len(), 1);
    let fill = &fills[0];
    assert_eq!(fill["id"], "9051");
    assert_eq!(fill["orderId"], "9051");
    assert_eq!(fill["symbol"], "BTCUSDT");
    assert_eq!(fill["side"], "buy");
    assert_eq!(fill["price"].as_f64(), Some(96.0));
    assert_eq!(fill["amount"].as_f64(), Some(0.25));
    drop(rows);
    venue.dispose().unwrap();
}
#[test]
fn production_private_positions_frame_updates_store() {
    let (venue, rt, store, _) = stream_fixture();
    deliver_private(
        &rt,
        json!({"e":"ACCOUNT_UPDATE","E":1700000001000u64,"a":{"P":[{"s":"BTCUSDT","pa":"3","ep":"97","up":"8","ps":"BOTH","mt":"cross","iw":"0"}]}}),
    );
    let rows = store.positions().unwrap();
    let position = rows
        .iter()
        .find(|p| p.symbol == "BTCUSDT" && p.side == PositionSide::Long)
        .unwrap();
    assert_eq!(
        (
            position.contracts,
            position.entry_price,
            position.unrealized_pnl
        ),
        (3.0, 97.0, 8.0)
    );
    drop(rows);
    venue.dispose().unwrap();
}
#[test]
fn production_private_balance_frame_updates_store() {
    let (venue, rt, store, _) = stream_fixture();
    deliver_private(
        &rt,
        json!({"e":"ACCOUNT_UPDATE","E":1700000001000u64,"a":{"B":[{"a":"USDT","wb":"125","cw":"110"}],"P":[]}}),
    );
    let balance = store.balance().unwrap();
    assert_eq!(
        (balance.total, balance.free, balance.used),
        (125.0, 110.0, 15.0)
    );
    drop(balance);
    venue.dispose().unwrap();
}

#[test]
fn market_load_ignores_unmatched_bracket_keys_and_defaults_falsy_leverage() {
    use std::io::Write;
    let row = |symbol: Value, value: Value| json!({"symbol":symbol,"brackets":[{"initialLeverage":value}]});
    let good = row(json!("BTCUSDT"), json!(125));
    let missing = json!({"symbol":"BTCUSDT","brackets":[{}]});
    let cases = vec![
        (
            "missing-symbol",
            json!([good,{"brackets":[{"initialLeverage":20}]}]),
            125.0,
        ),
        (
            "null-symbol",
            json!([good, row(Value::Null, json!(20))]),
            125.0,
        ),
        (
            "number-symbol",
            json!([good, row(json!(42), json!(20))]),
            125.0,
        ),
        (
            "object-symbol",
            json!([good, row(json!({"market":"BTCUSDT"}), json!(20))]),
            125.0,
        ),
        (
            "array-symbol",
            json!([good, row(json!(["BTCUSDT"]), json!(20))]),
            125.0,
        ),
        (
            "null-leverage",
            json!([row(json!("BTCUSDT"), Value::Null)]),
            10.0,
        ),
        ("missing-leverage", json!([missing]), 10.0),
        (
            "false-leverage",
            json!([row(json!("BTCUSDT"), json!(false))]),
            10.0,
        ),
        (
            "empty-leverage",
            json!([row(json!("BTCUSDT"), json!(""))]),
            10.0,
        ),
        (
            "zero-leverage",
            json!([row(json!("BTCUSDT"), json!(0))]),
            10.0,
        ),
        (
            "duplicate-last-null",
            json!([good, row(json!("BTCUSDT"), Value::Null)]),
            10.0,
        ),
        ("duplicate-last-missing", json!([good, missing]), 10.0),
        (
            "partial-before-null",
            json!([row(json!("BTCUSDT"), json!(35)), null]),
            35.0,
        ),
        (
            "single-null-leverage",
            row(json!("BTCUSDT"), Value::Null),
            10.0,
        ),
        (
            "unmatched-string-null",
            json!([good, row(json!("ETHUSDT"), Value::Null)]),
            125.0,
        ),
    ];
    assert_eq!(cases.len(), 15);
    let (_, _, _, wire) = support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    script(&wire);
    let market = wire.responses.borrow()["/fapi/v1/exchangeInfo"]["symbols"][0].clone();
    let compiler = std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").into()
    });
    let mut child = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/market-bracket-source.cjs"
        ))
        .env("SAFE_CEX_TYPESCRIPT_PATH", compiler)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let input = json!({"market":market,"cases":cases.iter().map(|(name,data,_)|json!({"name":name,"data":data})).collect::<Vec<_>>()});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 30);
    for (version, index) in [("v1", 0), ("v3", 15)] {
        let options = if version == "v3" {
            let secret = format!("{:064x}", 1);
            let wallet =
                safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&secret).unwrap();
            json!({"key":wallet.address().to_checksum(None),"secret":secret,"applicationId":"0x1111111111111111111111111111111111111111"})
        } else {
            json!({"key":"fixture-key","secret":"fixture-secret"})
        };
        for (offset, (name, data, expected)) in cases.iter().enumerate() {
            let source = &source[index + offset];
            assert_eq!(source["version"], version);
            assert_eq!(source["name"], *name);
            assert_eq!(
                source["max"].as_f64(),
                Some(*expected),
                "source default floor"
            );
            let (venue, rt, _, wire) = support::fixture(options.clone());
            script(&wire);
            wire.responses.borrow_mut().insert(
                format!("/fapi/{version}/exchangeInfo"),
                json!({"symbols":[market]}),
            );
            wire.responses
                .borrow_mut()
                .insert(format!("/fapi/{version}/leverageBracket"), data.clone());
            let markets = drive(&rt, venue.fetch_markets()).unwrap_or_else(|e| {
                panic!(
                    "real market bracket admission {version}/{name}: {}",
                    e.message
                )
            });
            assert_eq!(markets.len(), 1);
            assert_eq!(
                markets[0].limits.leverage.max.to_bits(),
                expected.to_bits(),
                "real market bracket leverage {version}/{name}"
            );
            assert!(sent(
                &wire,
                "GET",
                &format!("/fapi/{version}/leverageBracket")
            ));
            assert_eq!(
                source["requests"],
                json!([
                    format!("/fapi/{version}/exchangeInfo"),
                    format!("/fapi/{version}/leverageBracket")
                ])
            );
        }
    }
}

#[test]
fn algo_capacity_counts_raw_stop_info_when_normalized_type_is_undefined() {
    use safe_cex_store::{Store, StoreUpdate};
    let (venue, rt, store, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    let venue: Rc<dyn Exchange> = venue;
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    let mut markets = store.markets().unwrap().to_vec();
    let safe_cex_types::retained_json::RetainedJsonInput::Array(filters) = markets[0]
        .info
        .as_mut()
        .unwrap()
        .get_mut("filters")
        .unwrap()
    else {
        panic!("native filters array")
    };
    filters.push(json!({"filterType":"MAX_NUM_ALGO_ORDERS","limit":1}).into());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let existing = serde_json::from_value(json!({
        "id":"cached-stop","symbol":"BTCUSDT","status":"open","side":"sell",
        "price":90,"amount":1,"filled":0,"remaining":1,"reduceOnly":true,
        "lastUpdate":1700000000000u64,"info":{"type":"STOP"}
    }))
    .expect("source models admit an omitted normalized type");
    store.add_or_update_orders(vec![existing]).unwrap();
    take(&wire);
    let opts: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0.01,"price":90,"reduceOnly":true})).unwrap();
    let error = drive(&rt, venue.base().place_order(venue.as_ref(), opts))
        .expect_err("raw STOP info still consumes the source algo capacity");
    assert_eq!(
        error.message,
        "AsterDEX max algo order limit reached for BTCUSDT: 1/1 open, 1 new requested"
    );
    assert!(
        !wire
            .requests
            .borrow()
            .iter()
            .any(|r| r.method.eq_ignore_ascii_case("POST") && request_path(r) == "/fapi/v1/order")
    );
    venue.dispose().unwrap();
}

#[test]
fn market_leverage_applies_approved_js_number_after_source_fallback() {
    let cases = [
        (json!("125"), 125.),
        (json!("0"), 0.),
        (json!("-0"), -0.),
        (json!(" 20 "), 20.),
        (json!("0x20"), 32.),
        (json!("not-number"), f64::NAN),
        (json!("Infinity"), f64::INFINITY),
        (json!(true), 1.),
        (json!([]), 0.),
        (json!([20]), 20.),
        (json!({}), f64::NAN),
        (json!(0), 10.),
    ];
    for version in ["v1", "v3"] {
        let options = if version == "v3" {
            let secret = format!("{:064x}", 1);
            let wallet =
                safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&secret).unwrap();
            json!({"key":wallet.address().to_checksum(None),"secret":secret,
                "applicationId":"0x1111111111111111111111111111111111111111"})
        } else {
            json!({"key":"fixture-key","secret":"fixture-secret"})
        };
        for (raw, expected) in &cases {
            let (venue, rt, _, wire) = support::fixture(options.clone());
            script(&wire);
            let market = wire.responses.borrow()["/fapi/v1/exchangeInfo"]["symbols"][0].clone();
            wire.responses.borrow_mut().insert(
                format!("/fapi/{version}/exchangeInfo"),
                json!({"symbols":[market]}),
            );
            wire.responses.borrow_mut().insert(
                format!("/fapi/{version}/leverageBracket"),
                json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":raw}]}]),
            );
            let markets = drive(&rt, venue.fetch_markets())
                .unwrap_or_else(|error| panic!("approved Number at {version}/{raw}: {error:?}"));
            assert_eq!(markets.len(), 1);
            let actual = markets[0].limits.leverage.max;
            if expected.is_nan() {
                assert!(actual.is_nan(), "{version}/{raw} retains NaN");
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits(), "{version}/{raw}");
            }
            assert!(sent(
                &wire,
                "GET",
                &format!("/fapi/{version}/leverageBracket")
            ));
            venue.dispose().unwrap();
        }
    }
}

fn mark_frame_refuses_bad_rows(wrapped: bool) {
    use safe_cex_store::{Store, StoreUpdate};
    let (venue, rt, store, _) = stream_fixture();
    let original = store.tickers().unwrap()[0].clone();
    let tickers = ["BTCUSDT", "ETHUSDT", "LTCUSDT", "ADAUSDT"]
        .into_iter()
        .map(|symbol| {
            let mut ticker = original.clone();
            ticker.id = symbol.into();
            ticker.symbol = symbol.into();
            ticker
        })
        .collect::<Vec<_>>();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let before = store.tickers().unwrap().to_vec();
    let emitted = Rc::new(std::cell::Cell::new(0));
    let saved = emitted.clone();
    venue.base().on(
        "markPrice",
        Rc::new(move |_, _| {
            saved.set(saved.get() + 1);
            Ok(())
        }),
    );
    let rows = json!([
        {"e":"markPriceUpdate","s":"BTCUSDT","p":"201","i":"202","r":"0.01","T":"123"},
        {"e":"markPriceUpdate","s":"ETHUSDT","p":"301","i":"302","r":"0.02","T":1700000100000u64},
        {"e":"markPriceUpdate","s":"LTCUSDT","p":"401","i":"402","r":"0.03","T":true},
        {"e":"markPriceUpdate","s":"ADAUSDT","p":"501","i":"502","r":"0.04","T":1700000200000u64}
    ]);
    let frame = if wrapped {
        json!({"stream":"!markPrice@arr@1s","data":rows})
    } else {
        rows
    };
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.ends_with("/ws/fixture-listen-key"))
        .unwrap()
        .1
        .clone();
    rt.console.borrow_mut().clear();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(frame.to_string()),
        ));
    for _ in 0..16 {
        rt.pump();
    }
    let tickers = store.tickers().unwrap();
    let get = |symbol: &str| {
        tickers
            .iter()
            .find(|ticker| ticker.symbol == symbol)
            .unwrap()
    };
    assert_eq!(
        get("ETHUSDT").mark,
        301.,
        "valid ETH sibling survives a malformed fundingTime"
    );
    assert_eq!(
        get("ADAUSDT").mark,
        501.,
        "later valid ADA sibling survives"
    );
    assert_eq!(get("ETHUSDT").funding_time, 1700000100000.);
    assert_eq!(get("ADAUSDT").funding_time, 1700000200000.);
    for symbol in ["BTCUSDT", "LTCUSDT"] {
        let previous = before
            .iter()
            .find(|ticker| ticker.symbol == symbol)
            .unwrap();
        assert_eq!(
            serde_json::to_value(get(symbol)).unwrap(),
            serde_json::to_value(previous).unwrap(),
            "refuse the entire invalid row"
        );
    }
    assert_eq!(
        emitted.get(),
        2,
        "only admitted rows emit native ticker events"
    );
    let reports = rt.console.borrow().join("\n");
    assert!(
        reports.contains("BTCUSDT")
            && reports.contains("LTCUSDT")
            && reports.contains("fundingTime")
            && reports.contains("\"123\"")
            && reports.contains("true"),
        "{reports}"
    );
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    drop(tickers);
    venue.dispose().unwrap();
}
#[test]
fn wrapped_mark_price_frame_retains_valid_siblings() {
    mark_frame_refuses_bad_rows(true);
}
#[test]
fn bare_mark_price_frame_retains_valid_siblings() {
    mark_frame_refuses_bad_rows(false);
}

#[test]
fn mark_price_falsey_time_and_unknown_targets_keep_source_behavior() {
    let (venue, rt, store, _) = stream_fixture();
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.ends_with("/ws/fixture-listen-key"))
        .unwrap()
        .1
        .clone();
    let rows = json!([
        {"e":"markPriceUpdate","s":"BTCUSDT","p":"200","T":1700000100000u64},
        {"e":"markPriceUpdate","s":"BTCUSDT","p":"201","T":null},
        {"e":"markPriceUpdate","s":"BTCUSDT","p":"202","T":false},
        {"e":"markPriceUpdate","s":"BTCUSDT","p":"203","T":0},
        {"e":"markPriceUpdate","s":"BTCUSDT","p":"204","T":""},
        {"e":"markPriceUpdate","s":"MISSING","p":"999","T":"123"},
        {"e":"markPriceUpdate","s":"constructor","p":"999","T":true}
    ]);
    rt.console.borrow_mut().clear();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(rows.to_string()),
        ));
    for _ in 0..16 {
        rt.pump();
    }
    let tickers = store.tickers().unwrap();
    assert_eq!(tickers[0].mark, 204.);
    assert_eq!(tickers[0].funding_time, 1700000100000.);
    assert!(
        !rt.console
            .borrow()
            .iter()
            .any(|message| message.contains("typed field")),
        "{:?}",
        rt.console.borrow()
    );
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    drop(tickers);
    venue.dispose().unwrap();
}

#[test]
fn alternate_typed_preparation_keeps_all_native_fields_and_presence() {
    use safe_cex_types::{
        ModelField, OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType,
    };
    use safe_cex_venue_asterdex::{
        order_execution::{ExecutionOrder, PreparedExecutionOrder},
        types::{AsterDEXPlaceOrderOpts, AsterDEXStpMode},
    };
    for (amount, reduce_only, post_only) in [
        (1.25, Some(false), Some(true)),
        (-0.0, Some(true), Some(false)),
        (2.5, None, None),
    ] {
        let order = PlaceOrderOpts {
            symbol: "BTCUSDT".into(),
            r#type: OrderType::Limit,
            side: OrderSide::Sell,
            amount,
            price: Some(-0.0),
            trigger_price: Some(3.25),
            stop_loss: Some(4.5),
            take_profit: Some(5.75),
            reduce_only,
            time_in_force: Some(OrderTimeInForce::PostOnly),
            trigger_type: Some(OrderStopTriggerType::MarkPrice),
            tp_trigger_type: Some(OrderStopTriggerType::LastPrice),
            sl_trigger_type: Some(OrderStopTriggerType::IndexPrice),
            client_order_id: Some("native-λ".into()),
            max_slippage_pct: Some(0.125),
            close: Some(false),
            implied_volatility: Some(0.25),
        };
        let venue: AsterDEXPlaceOrderOpts = order.clone().into();
        assert_eq!(venue.order, order);
        assert!(
            venue.stp_mode.is_none(),
            "shared-to-venue conversion does not invent STP"
        );
        let input = ExecutionOrder {
            opts: AsterDEXPlaceOrderOpts {
                order: order.clone(),
                stp_mode: Some(AsterDEXStpMode::V2),
            },
            post_only,
        };
        let borrowed = PreparedExecutionOrder::from(&input);
        let owned = PreparedExecutionOrder::from(input.clone());
        let copied = PreparedExecutionOrder::from(&borrowed);
        for prepared in [&borrowed, &owned, &copied] {
            let got = &prepared.opts.order;
            assert_eq!(got.symbol, order.symbol);
            assert_eq!(got.r#type, ModelField::Value(order.r#type));
            assert_eq!(got.side, ModelField::Value(order.side));
            assert_eq!(got.amount.to_bits(), amount.to_bits());
            assert_eq!(got.price.unwrap().to_bits(), (-0.0_f64).to_bits());
            assert_eq!(got.trigger_price, order.trigger_price);
            assert_eq!(got.stop_loss, order.stop_loss);
            assert_eq!(got.take_profit, order.take_profit);
            assert_eq!(got.reduce_only, ModelField::from(reduce_only));
            assert_eq!(got.time_in_force, order.time_in_force);
            assert_eq!(got.trigger_type, order.trigger_type);
            assert_eq!(got.tp_trigger_type, order.tp_trigger_type);
            assert_eq!(got.sl_trigger_type, order.sl_trigger_type);
            assert_eq!(got.client_order_id, order.client_order_id);
            assert_eq!(got.max_slippage_pct, order.max_slippage_pct);
            assert_eq!(got.close, order.close);
            assert_eq!(got.implied_volatility, order.implied_volatility);
            assert_eq!(prepared.opts.stp_mode, Some(AsterDEXStpMode::V2));
            assert_eq!(prepared.post_only, post_only);
        }
    }
}
