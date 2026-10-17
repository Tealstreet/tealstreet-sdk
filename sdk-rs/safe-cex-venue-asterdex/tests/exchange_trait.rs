//! Concrete AsterDEX bindings on the shared runtime and scripted wire.
#![cfg(not(target_arch = "wasm32"))]
#[path = "support/exchange_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_transport::RestRequest;
use safe_cex_types::{PlaceOrderOpts, PositionSide, UpdateOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::drive;

fn script(wire: &support::Wire) {
    let mut responses = wire.responses.borrow_mut();
    responses.insert(
        "/fapi/v1/time".into(),
        json!({"serverTime":1700000000000u64}),
    );
    responses.insert("/fapi/v4/account".into(),json!({"accountAlias":"aster-account","totalWalletBalance":"100","availableBalance":"70","totalUnrealizedProfit":"5","totalInitialMargin":"12","totalOpenOrderInitialMargin":"3"}));
    responses.insert(
        "/fapi/v1/positionSide/dual".into(),
        json!({"dualSidePosition":false}),
    );
    responses.insert(
        "/fapi/v1/listenKey".into(),
        json!({"listenKey":"fixture-listen-key"}),
    );
    responses.insert("/fapi/v1/exchangeInfo".into(),json!({"symbols":[{
        "symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","contractType":"PERPETUAL","status":"TRADING",
        "maintMarginPercent":"2.5","requiredMarginPercent":"5","pricePrecision":2,"quantityPrecision":3,"baseAssetPrecision":8,"quotePrecision":8,
        "triggerProtect":"0.05","liquidationFee":"0.01","marketTakeBound":"0.3","filters":[{"filterType":"PRICE_FILTER","minPrice":"0.01","maxPrice":"1000000","tickSize":"0.25"},{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"100","stepSize":"0.001"}],"orderTypes":["LIMIT","MARKET"],"timeInForce":["GTC","IOC"]
    }]}));
    responses.insert(
        "/fapi/v1/leverageBracket".into(),
        json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":100}]}]),
    );
    responses.insert("/fapi/v1/ticker/24hr".into(),json!([{"symbol":"BTCUSDT","priceChange":"2","priceChangePercent":"1","weightedAvgPrice":"101","lastPrice":"102","lastQty":"0.2","openPrice":"100","highPrice":"110","lowPrice":"90","volume":"50","quoteVolume":"5000","openTime":1699990000000u64,"closeTime":1700000000000u64,"firstId":1,"lastId":10,"count":10}]));
    responses.insert(
        "/fapi/v1/openInterest".into(),
        json!({"symbol":"BTCUSDT","openInterest":"7"}),
    );
    responses.insert("/fapi/v1/positionRisk".into(),json!([{"symbol":"BTCUSDT","positionAmt":"2","entryPrice":"99","breakEvenPrice":"99.5","markPrice":"102","unRealizedProfit":"6","liquidationPrice":"50","leverage":"10","maxNotionalValue":"10000","marginType":"cross","isolatedMargin":"0","isAutoAddMargin":"false","positionSide":"BOTH","notional":"204","isolatedWallet":"0","updateTime":1700000000000u64}]));
    responses.insert("/fapi/v1/openOrders".into(),json!([{"symbol":"BTCUSDT","orderId":9001,"clientOrderId":"resting","price":"95","origQty":"0.02","executedQty":"0.005","cummulativeQuoteQty":"0.475","status":"NEW","timeInForce":"GTC","type":"LIMIT","side":"BUY","time":1699990000000u64,"updateTime":1700000000000u64,"isWorking":true,"workingTime":1699990000000u64,"origQuoteOrderQty":"1.9"}]));
    responses.insert("/fapi/v1/order".into(), json!({"orderId":1234567}));
    responses.insert("/fapi/v1/trades".into(),json!([{"id":4,"price":"101","qty":"0.03","quoteQty":"3.03","time":1700000000000u64,"isBuyerMaker":true}]));
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
            .get("__suppressAsterUnknownOrderError"),
        Some(&json!(true))
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
        store.loaded().markets
            && store.loaded().tickers
            && store.loaded().orders
            && store.loaded().positions
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
    assert!(store.options().is_hedged);
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
        store.loaded().markets
            && store.loaded().tickers
            && store.loaded().positions
            && store.loaded().orders
    );
    assert_eq!(store.markets()[0].symbol, "BTCUSDT");
    assert_eq!(store.tickers()[0].last.to_bits(), 102f64.to_bits());
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
    assert_eq!(store.orders()[0].id, "9001");
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
    let rows = store.orders();
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
    Rc<safe_cex_store::store::DefaultStore>,
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
    let rows = store.orders();
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
    let rows = store.orders();
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
    let rows = store.positions();
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
    let balance = store.balance();
    assert_eq!(
        (balance.total, balance.free, balance.used),
        (125.0, 110.0, 15.0)
    );
    drop(balance);
    venue.dispose().unwrap();
}
