use super::{CoinbaseDependencies, CoinbaseExchange, options_root};
use crate::{
    api_factory::{ApiRequest, CoinbaseApiHost},
    cdp_signing::{self, CdpCryptoKey, CdpSigningCache, CdpSigningReceiver},
    exchange_lifecycle::LifecycleAccountRoot,
    exchange_ws_auth::{ExchangeWsAuthAsync, ExchangeWsAuthHost},
    order_edit::{EditUpdate, EditUpdateRoot},
    order_sync::{PrivateOrderEvent, PrivateOrderEventInput, PrivateOrderEventRoot},
    portfolios_balances::{Portfolio, PortfolioState},
    private_stream_dispatch::{PrivateOptions, PrivateStreamHost},
    public_market_reads::PublicOHLCVOptionsRoot,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use safe_cex_exchange::{
    js_runtime::{
        client,
        value::{Js, to_typed},
    },
    native::native_runtime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{
    AnyTimeframe, ExchangeName, OHLCVOptions, Timeframe, retained_json::RetainedJsonInput as Input,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    sync::Arc,
};

fn owner(name: ExchangeName) -> Rc<CoinbaseExchange> {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    CoinbaseExchange::new(
        name,
        serde_json::from_value(json!({"key":"synthetic","secret":"synthetic"})).unwrap(),
        Rc::new(DefaultStore::new()),
        CoinbaseDependencies {
            runtime,
            http,
            external_public_data: None,
            signing_cache: Rc::new(CdpSigningCache::default()),
            local_wall_time_to_utc: Rc::new(|value| value),
        },
    )
    .unwrap()
}

fn executor() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn request_private_options_and_account_aliases_come_from_real_class_adapters() {
    tokio::task::LocalSet::new().block_on(&executor(), async {
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let exchange = owner(name);
            let fields = Rc::new(RefCell::new(indexmap::IndexMap::from([(
                "native".encode_utf16().collect(),
                Frame::Array(Rc::new(RefCell::new(vec![
                    Frame::Number(-0.0),
                    Frame::Number(f64::NAN),
                    Frame::Undefined,
                    Frame::String(vec![0xd800]),
                ]))),
            )])));
            let request: ApiRequest = exchange
                .host
                .replace_url_params(None, Some(Frame::Object(fields.clone())))
                .unwrap();
            let captured = request.clone();
            assert!(Rc::ptr_eq(&request, &captured));
            assert!(matches!(
                captured
                    .borrow()
                    .get(&"url".encode_utf16().collect::<Vec<_>>()),
                Some(None)
            ));
            let Frame::Object(actual) = captured
                .borrow()
                .get(&"params".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .clone()
                .unwrap()
            else {
                panic!("native params root")
            };
            assert!(Rc::ptr_eq(&actual, &fields));
            exchange
                .host
                .mark_skip_remote_signing(request.clone())
                .unwrap();
            assert!(matches!(
                captured.borrow().get(
                    &"safeCexSkipRemoteSigning"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                ),
                Some(Some(Frame::Bool(true)))
            ));
            let Frame::Array(values) = actual
                .borrow()
                .get(&"native".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .clone()
            else {
                panic!("native params array")
            };
            {
                let values = values.borrow();
                assert!(
                    matches!(values[0], Frame::Number(value) if value.to_bits()==(-0.0_f64).to_bits())
                );
                assert!(matches!(values[1], Frame::Number(value) if value.is_nan()));
                assert!(matches!(values[2], Frame::Undefined));
                assert!(matches!(&values[3], Frame::String(units) if units==&[0xd800]));
            }
            let private = exchange.class.private_websocket.borrow().clone().unwrap();
            let options: PrivateOptions = private.receiver.host.options();
            let captured = options.clone();
            assert!(Rc::ptr_eq(&options, &captured));
            assert_eq!(captured.borrow().key, Some(json!("synthetic")));
            assert_eq!(captured.borrow().secret, Some(json!("synthetic")));
            assert!(captured.borrow().remote_signer.is_none());
            options.borrow_mut().key = Some(json!("changed"));
            assert_eq!(captured.borrow().key, Some(json!("changed")));
            let state: PortfolioState = to_typed(&Js::object(vec![
                ("uuid", Js::str("portfolio-native")),
                ("type", Js::str("INTX")),
            ]))
            .unwrap();
            let selected: Portfolio = Rc::new(RefCell::new(state));
            *exchange.class.selected_portfolio.borrow_mut() = Some(selected.clone());
            let account: LifecycleAccountRoot = exchange.class.get_account().await.unwrap();
            let captured = account.clone();
            assert!(Rc::ptr_eq(&account, &captured));
            assert_eq!(captured.borrow().user_id, "portfolio-native");
            account.borrow_mut().user_id = "local-account-root".into();
            assert_eq!(captured.borrow().user_id, "local-account-root");
            assert_eq!(
                selected.borrow().uuid,
                "portfolio-native",
                "source account is a field snapshot"
            );
        }
    });
}

#[test]
fn input_root_aliases_retain_presence_numbers_and_actual_event_array_adapter() {
    let opts: PublicOHLCVOptionsRoot = options_root(OHLCVOptions {
        symbol: "BTCUSD".into(),
        interval: AnyTimeframe::Variant1(Timeframe::M1),
        from: Some(-0.0),
        to: Some(17.0),
        limit: None,
    });
    let captured = opts.clone();
    assert!(Rc::ptr_eq(&opts, &captured));
    assert_eq!(captured.borrow().symbol, "BTCUSD");
    assert_eq!(captured.borrow().interval, "1m");
    assert_eq!(
        captured
            .borrow()
            .from
            .as_ref()
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(captured.borrow().to, Some(json!(17.0)));
    assert!(captured.borrow().limit.is_none());
    opts.borrow_mut().symbol = "changed".into();
    assert_eq!(captured.borrow().symbol, "changed");
    let update: EditUpdateRoot = Rc::new(RefCell::new(EditUpdate {
        amount: Some(f64::NAN),
        price: Some(-0.0),
        has_amount: true,
        has_price: false,
    }));
    let captured = update.clone();
    assert!(Rc::ptr_eq(&captured, &update));
    assert!(captured.borrow().amount.unwrap().is_nan());
    assert_eq!(
        captured.borrow().price.unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(captured.borrow().has_amount);
    assert!(!captured.borrow().has_price);
    update.borrow_mut().has_price = true;
    assert!(captured.borrow().has_price);
    let first = Rc::new(RefCell::new(Input::Array(vec![
        Input::Float(f64::INFINITY),
        Input::Undefined,
        Input::Text(vec![0xd800]),
    ])));
    let orders = Rc::new(RefCell::new(vec![
        first.clone(),
        Rc::new(RefCell::new(Input::Null)),
    ]));
    let event: PrivateOrderEventRoot = Rc::new(RefCell::new(PrivateOrderEvent {
        event_type: Some("snapshot".into()),
        orders: orders.clone(),
    }));
    let captured = event.clone();
    assert!(Rc::ptr_eq(&event, &captured));
    assert_eq!(captured.kind().unwrap(), Some("snapshot".into()));
    let actual = captured.orders().unwrap();
    assert!(Rc::ptr_eq(&actual, &orders));
    assert_eq!(actual.borrow().len(), 2);
    assert!(Rc::ptr_eq(&actual.borrow()[0], &first));
    event.borrow_mut().event_type = Some("update".into());
    assert_eq!(captured.kind().unwrap(), Some("update".into()));
    let item = actual.borrow()[0].clone();
    assert!(
        matches!(&*item.borrow(),Input::Array(values) if values.len()==3 && matches!(values[0],Input::Float(value) if value==f64::INFINITY) && matches!(values[1],Input::Undefined) && matches!(&values[2],Input::Text(units) if units==&[0xd800]))
    );
}

fn source_crypto(input: Value) -> Value {
    let mut process = Command::new("node")
        .arg("../safe-cex-crypto/tests/es256-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn crypto_key_and_ws_auth_aliases_use_real_import_cache_signer_and_node_verifier() {
    let source = source_crypto(json!({}));
    let der: Vec<u8> = serde_json::from_value(source["cases"][0]["pkcs8"].clone()).unwrap();
    let secret = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(der)
    );
    tokio::task::LocalSet::new().block_on(&executor(), async {
        let runtime = native_runtime();
        let receiver = Rc::new(CdpSigningReceiver {
            host: crate::binding::crypto::CryptoHost {
                runtime: runtime.clone(),
            },
            cache: Rc::new(CdpSigningCache::default()),
            spawn: crate::binding::crypto::spawner(runtime),
        });
        let key: CdpCryptoKey = cdp_signing::get_signing_key(receiver.clone(), secret.clone())
            .await
            .unwrap();
        assert!(
            key.downcast_ref::<safe_cex_crypto::es256::Es256SigningKey>()
                .is_some()
        );
        let cached: CdpCryptoKey = cdp_signing::get_signing_key(receiver.clone(), secret.clone())
            .await
            .unwrap();
        assert!(Rc::ptr_eq(&key, &cached));
        assert!(Rc::ptr_eq(
            receiver.cache.crypto_key.borrow().as_ref().unwrap(),
            &key
        ));
        assert_eq!(&*receiver.cache.key_secret.borrow(), &secret);
        let mut verify = Vec::new();
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let exchange = owner(name);
            let result: ExchangeWsAuthAsync = exchange.host.sign_local(
                Some(Frame::String(
                    "organizations/alias/apiKeys/native"
                        .encode_utf16()
                        .collect(),
                )),
                Some(Frame::String(secret.encode_utf16().collect())),
            );
            let promise = result.unwrap();
            assert!(promise.same_promise(&promise.clone()));
            let Frame::String(units) = promise.await.unwrap().unwrap() else {
                panic!("signed token string")
            };
            let token = String::from_utf16(&units).unwrap();
            let parts: Vec<_> = token.split('.').collect();
            assert_eq!(parts.len(), 3);
            let payload = format!("{}.{}", parts[0], parts[1]).into_bytes();
            let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
            assert_eq!(signature.len(), 64);
            verify.push(json!({"payload":payload,"signature":signature}));
        }
        let result = source_crypto(json!({"verify":verify}));
        assert_eq!(
            result,
            json!([{"valid":true,"changed":false},{"valid":true,"changed":false}])
        );
    });
}
