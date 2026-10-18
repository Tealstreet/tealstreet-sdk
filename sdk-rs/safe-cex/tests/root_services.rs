use safe_cex::{account::*, factory::*, options_patch::*};
use safe_cex_store::{Store, store::DefaultStore, virtual_clock::VirtualClock};
use safe_cex_transport::TransportRegistry;
use safe_cex_types::{ExchangeAccountConfig, ExchangeName, ExchangeOptions};
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(waker)) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture"),
    }
}
#[derive(Clone)]
struct Fixture {
    name: ExchangeName,
    options: ExchangeOptions,
    store: Rc<dyn Store>,
}
#[test]
fn factory_cache_native_options_and_store_identity() {
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let constructor: ExchangeConstructor<Fixture> = Rc::new(|name, options, store| {
        Ok(Fixture {
            name,
            options,
            store,
        })
    });
    let loader: ExchangeLoader<Fixture> = Rc::new(move || {
        count.set(count.get() + 1);
        let constructor = constructor.clone();
        Box::pin(async move { Ok(constructor) })
    });
    let factory = ExchangeFactory::new(
        [("bybit".into(), loader.clone()), ("binance".into(), loader)]
            .into_iter()
            .collect(),
        VirtualClock::default(),
        TransportRegistry::default(),
    );
    assert_eq!(factory.get_available_exchanges(), vec!["bybit", "binance"]);
    assert!(!factory.is_exchange_loaded("bybit"));
    ready(factory.preload_exchange("bybit")).unwrap();
    ready(factory.preload_exchanges(&["bybit".into(), "binance".into()])).unwrap();
    assert_eq!(calls.get(), 2);
    assert!(factory.is_exchange_loaded("bybit"));
    assert_eq!(
        ready(factory.preload_exchange("unknown")).unwrap_err(),
        "Unknown exchange: unknown"
    );
    let patch = ExchangeOptionsPatch {
        override_proxy_url: Some(Some("explicit".into())),
        tealstreet_proxy: Some(Some(true)),
        tealstreet_tunnel: Some(Some(true)),
        tealstreet_ws_route: Some(Some(true)),
        ..Default::default()
    };
    let public = factory
        .build_public_exchange_options("bybit", Some(&patch), false, None)
        .unwrap();
    assert_eq!(public.key, "");
    assert_eq!(public.secret, "");
    assert_eq!(public.public_data_only, Some(true));
    assert_eq!(public.override_proxy_url.as_deref(), Some("explicit"));
    let disabled = factory
        .build_public_exchange_options("bybit", Some(&patch), true, None)
        .unwrap();
    assert_eq!(disabled.override_proxy_url, None);
    assert_eq!(disabled.tealstreet_proxy, Some(false));
    assert_eq!(disabled.tealstreet_tunnel, Some(false));
    assert_eq!(disabled.tealstreet_ws_route, Some(false));
    let supplied: Rc<dyn Store> = Rc::new(DefaultStore::new());
    let instance =
        ready(factory.create_exchange("bybit", public.clone(), Some(supplied.clone()))).unwrap();
    assert_eq!(instance.name, ExchangeName::Bybit);
    assert_eq!(instance.options, public);
    assert!(Rc::ptr_eq(&instance.store, &supplied));
    assert_eq!(calls.get(), 2);
    let defaulted =
        ready(factory.create_public_exchange("binance", None, false, None, None)).unwrap();
    assert_eq!(defaulted.options.public_data_only, Some(true));
    assert!(!Rc::ptr_eq(&defaulted.store, &supplied));
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/root_services_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                // CI supplies an isolated compiler; this fallback only
                // serves a local run from the repo root.
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let native_disabled: ExchangeOptions =
        serde_json::from_value(oracle["disabled"].clone()).unwrap();
    assert_eq!(disabled, native_disabled);
}
#[test]
fn account_native_forwarding_overrides_and_broker_clear() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/root_services_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                // CI supplies an isolated compiler; this fallback only
                // serves a local run from the repo root.
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let accounts: Vec<ExchangeAccountConfig> =
        serde_json::from_value(oracle["accounts"].clone()).unwrap();
    assert_eq!(accounts.len(), 6);
    let registry = TransportRegistry::default();
    let env = |name: &str| match name {
        "NEXT_PUBLIC_KUCOIN_BROKER_PARTNER" => Some("partner".into()),
        "NEXT_PUBLIC_KUCOIN_BROKER_KEY" => Some("brokerkey".into()),
        "NEXT_PUBLIC_KUCOIN_BROKER_NAME" => Some("brokername".into()),
        "NEXT_PUBLIC_KUCOIN_BROKER_VERIFY" => Some("yes".into()),
        _ => None,
    };
    let host = AccountOptionsHost {
        transport: &registry,
        instance: None,
        environment: &env,
        warn: &|_| {},
    };
    let patch = ExchangeOptionsPatch {
        key: Some("override".into()),
        tealstreet_proxy: Some(Some(false)),
        extra: Some(ExchangeOptionsExtraPatch {
            tick_interval: Some(Some(25.0)),
            kucoin_broker: Some(None),
            ..Default::default()
        }),
        ..Default::default()
    };
    let options: Vec<_> = accounts
        .iter()
        .map(|account| account_to_exchange_options(account, Some(&patch), &host).unwrap())
        .collect();
    assert_eq!(options[0].key, "override");
    assert_eq!(options[0].enable_spot_markets, None);
    assert_eq!(options[1].application_id.as_deref(), Some("application"));
    assert_eq!(options[1].api_key_index.as_deref(), Some("index"));
    assert_eq!(options[2].grvt_signer_address.as_deref(), Some("signer"));
    assert_eq!(
        options[2].grvt_trading_account_id.as_deref(),
        Some("trading")
    );
    assert_eq!(options[3].extra.as_ref().unwrap().kucoin_broker, None);
    assert_eq!(options[3].extra.as_ref().unwrap().tick_interval, Some(25.0));
    assert_eq!(options[4].paradex_address.as_deref(), Some("paradex"));
    assert_eq!(options[4].subkey_address.as_deref(), Some("subkey"));
    assert_eq!(
        options[0].bybit_site_id,
        Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl)
    );
    assert_eq!(options[5].bybit_site_id, None);
    // DirectTransport has no CORS proxy; the TS oracle's injected host does. All
    // other native fields are compared before serializing either side.
    let mut expected: Vec<ExchangeOptions> =
        serde_json::from_value(oracle["options"].clone()).unwrap();
    for expected in &mut expected {
        expected.secondary_proxy_url = None;
    }
    assert_eq!(options, expected);
    let native_broker = account_to_exchange_options(&accounts[3], None, &host)
        .unwrap()
        .extra
        .unwrap()
        .kucoin_broker
        .unwrap();
    assert_eq!(native_broker.partner, "partner");
    assert_eq!(native_broker.broker_key, "brokerkey");
    assert_eq!(native_broker.broker_name, "brokername");
    assert_eq!(native_broker.verify, Some(true));
    let nonfinite = ExchangeOptionsPatch {
        extra: Some(ExchangeOptionsExtraPatch {
            tick_interval: Some(Some(f64::NAN)),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(
        account_to_exchange_options(&accounts[0], Some(&nonfinite), &host)
            .unwrap()
            .extra
            .unwrap()
            .tick_interval
            .unwrap()
            .is_nan()
    );
    let constructor: ExchangeConstructor<Fixture> = Rc::new(|name, options, store| {
        Ok(Fixture {
            name,
            options,
            store,
        })
    });
    let loader: ExchangeLoader<Fixture> = Rc::new(move || {
        let constructor = constructor.clone();
        Box::pin(async move { Ok(constructor) })
    });
    let factory = ExchangeFactory::new(
        [("bybit".into(), loader)].into_iter().collect(),
        VirtualClock::default(),
        TransportRegistry::default(),
    );
    let created = ready(create_exchange_from_account(
        &factory,
        &accounts[0],
        Some(&patch),
        &host,
    ))
    .unwrap();
    assert_eq!(created.name, ExchangeName::Bybit);
    assert_eq!(created.options, options[0]);
}

#[test]
fn facade_construction_preserves_polymarket_patch_credentials_and_native_signer_type() {
    let constructor: ExchangeConstructor<Fixture> = Rc::new(|name, options, store| {
        Ok(Fixture {
            name,
            options,
            store,
        })
    });
    let loader: ExchangeLoader<Fixture> = Rc::new(move || {
        let constructor = constructor.clone();
        Box::pin(async move { Ok(constructor) })
    });
    let factory = ExchangeFactory::new(
        [("polymarket".into(), loader)].into_iter().collect(),
        VirtualClock::default(),
        TransportRegistry::default(),
    );
    let account: ExchangeAccountConfig = serde_json::from_value(serde_json::json!({
        "exchange":"polymarket", "name":"account", "apiKey":"", "apiSecret":""
    }))
    .unwrap();
    let registry = TransportRegistry::default();
    let host = AccountOptionsHost {
        transport: &registry,
        instance: None,
        environment: &|_| None,
        warn: &|_| {},
    };
    assert!(
        factory
            .build_public_exchange_options("polymarket", None, false, None)
            .unwrap()
            .polymarket
            .is_none()
    );
    for signer_type in [0.0, 1.0, 2.0, 3.0, -0.0, f64::NAN, f64::INFINITY] {
        let patch = ExchangeOptionsPatch {
            polymarket: Some(Some(safe_cex_types::ExchangeOptionsPolymarket {
                api_key: Some("supplied-key".into()),
                api_secret: Some("supplied-secret".into()),
                api_passphrase: Some("supplied-passphrase".into()),
                signer_type: Some(signer_type),
                funder_address: Some("supplied-funder".into()),
            })),
            ..Default::default()
        };
        let account_instance = ready(create_exchange_from_account(
            &factory,
            &account,
            Some(&patch),
            &host,
        ))
        .unwrap();
        let public_instance =
            ready(factory.create_public_exchange("polymarket", Some(&patch), false, None, None))
                .unwrap();
        for instance in [account_instance, public_instance] {
            let options = instance.options.polymarket.unwrap();
            assert_eq!(options.api_key.as_deref(), Some("supplied-key"));
            assert_eq!(options.api_secret.as_deref(), Some("supplied-secret"));
            assert_eq!(
                options.api_passphrase.as_deref(),
                Some("supplied-passphrase")
            );
            assert_eq!(options.funder_address.as_deref(), Some("supplied-funder"));
            assert_eq!(
                options.signer_type.unwrap().to_bits(),
                signer_type.to_bits()
            );
        }
    }
    let clear = ExchangeOptionsPatch {
        polymarket: Some(None),
        ..Default::default()
    };
    assert!(
        ready(create_exchange_from_account(
            &factory,
            &account,
            Some(&clear),
            &host
        ))
        .unwrap()
        .options
        .polymarket
        .is_none()
    );
}

#[test]
fn option_and_inverse_toggles_reach_constructors_with_source_metadata_and_override_precedence() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/root_option_toggles_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        1764,
        "49 venues x 3 toggles x 3 account states x 4 override states"
    );
    let constructor: ExchangeConstructor<Fixture> = Rc::new(|name, options, store| {
        Ok(Fixture {
            name,
            options,
            store,
        })
    });
    let loader: ExchangeLoader<Fixture> = Rc::new(move || {
        let constructor = constructor.clone();
        Box::pin(async move { Ok(constructor) })
    });
    let factory = ExchangeFactory::new(
        rows.iter()
            .map(|row| {
                (
                    row["account"]["exchange"].as_str().unwrap().into(),
                    loader.clone(),
                )
            })
            .collect(),
        VirtualClock::default(),
        TransportRegistry::default(),
    );
    assert_eq!(factory.get_available_exchanges().len(), 49);
    let registry = TransportRegistry::default();
    let warnings = std::cell::RefCell::new(Vec::<String>::new());
    let warn = |message: &str| warnings.borrow_mut().push(message.to_owned());
    let host = AccountOptionsHost {
        transport: &registry,
        instance: None,
        environment: &|_| None,
        warn: &warn,
    };
    for row in rows {
        let account: ExchangeAccountConfig =
            serde_json::from_value(row["account"].clone()).unwrap();
        let value = match row["override"].as_str() {
            Some("absent") => None,
            Some("undefined") => Some(None),
            None => Some(Some(row["override"].as_bool().unwrap())),
            _ => panic!("unknown source override state"),
        };
        let mut patch = ExchangeOptionsPatch::default();
        match row["field"].as_str().unwrap() {
            "enableSpotMarkets" => patch.enable_spot_markets = value,
            "enableOptionMarkets" => patch.enable_option_markets = value,
            "enableInverseMarkets" => patch.enable_inverse_markets = value,
            _ => panic!("unknown source toggle"),
        }
        warnings.borrow_mut().clear();
        let instance = ready(create_exchange_from_account(
            &factory,
            &account,
            Some(&patch),
            &host,
        ))
        .unwrap();
        let native = &instance.options;
        let mut projected = serde_json::Map::new();
        for (field, value) in [
            ("enableSpotMarkets", native.enable_spot_markets),
            ("enableOptionMarkets", native.enable_option_markets),
            ("enableInverseMarkets", native.enable_inverse_markets),
        ] {
            if let Some(value) = value {
                projected.insert(field.into(), serde_json::json!(value));
            }
        }
        assert_eq!(
            serde_json::Value::Object(projected),
            row["expected"],
            "source case {row}"
        );
        assert_eq!(
            serde_json::json!(*warnings.borrow()),
            row["warnings"],
            "metadata warning case {row}"
        );
        assert_eq!(instance.name, account.exchange);
    }
    let patch = ExchangeOptionsPatch {
        enable_option_markets: Some(Some(true)),
        enable_inverse_markets: Some(Some(false)),
        ..Default::default()
    };
    let public =
        ready(factory.create_public_exchange("bybit", Some(&patch), false, None, None)).unwrap();
    assert_eq!(public.options.enable_option_markets, Some(true));
    assert_eq!(public.options.enable_inverse_markets, Some(false));
}

#[test]
fn hyro_account_mode_maps_before_presence_aware_overrides() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/root_option_toggles_source.cjs"
        ))
        .arg("--hyro-modes")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        24,
        "two venues, three account flags, four override states"
    );
    let transport = TransportRegistry::default();
    let host = AccountOptionsHost {
        transport: &transport,
        instance: None,
        environment: &|_| None,
        warn: &|_| {},
    };
    for row in rows {
        let account: ExchangeAccountConfig =
            serde_json::from_value(row["account"].clone()).unwrap();
        let mut patch = ExchangeOptionsPatch::default();
        if row["override"] != "absent" {
            patch.extra = Some(ExchangeOptionsExtraPatch {
                hyrotrader_mode: Some(if row["override"] == "undefined" {
                    None
                } else {
                    Some(serde_json::from_value(row["override"].clone()).unwrap())
                }),
                ..Default::default()
            });
        }
        let options = account_to_exchange_options(&account, Some(&patch), &host).unwrap();
        assert_eq!(
            serde_json::to_value(options.extra).unwrap(),
            row["extra"],
            "source Hyro account flag/override {}",
            row
        );
    }
}
