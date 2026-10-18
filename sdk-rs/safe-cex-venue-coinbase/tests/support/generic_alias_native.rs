use super::{CoinbaseDependencies, CoinbaseExchange, Host};
use crate::{
    cdp_signing::CdpSigningCache,
    classes::{ClassFindMethod, ClassSupportedMethod},
    market_position_reads::ReadItem,
    order_sync::{SyncedOrder, SyncedOrderRoot, SyncedOrders},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{
        CoinbaseStarbaseAuthPromise, CoinbaseStarbasePromise as Promise,
        CoinbaseStarbaseSessionError, CoinbaseStarbaseSessionFailure,
    },
    starbase_rpc::CoinbaseStarbaseRequestError,
    starbase_ws_controller::CoinbaseStarbaseActivationPromise,
};
use safe_cex_exchange::{js_runtime::client, native::native_runtime};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, Order, retained_json::RetainedJsonInput as Input};
use serde_json::json;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

#[test]
fn generic_async_aliases_retain_native_completion_and_failure_identity() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = native_runtime();
        let spawn = crate::binding::crypto::spawner(runtime);
        macro_rules! observe {
            ($alias:ty) => {
                for flag in [false, true] {
                    let (promise, resolve) = Promise::<Result<bool, PortfolioFailure>>::pending();
                    let value: $alias = Ok(promise.clone());
                    assert!(value.as_ref().unwrap().same_promise(&promise));
                    spawn(Box::pin(async move {
                        tokio::task::yield_now().await;
                        resolve.resolve(Ok(flag));
                    }));
                    let result = tokio::time::timeout(Duration::from_secs(1), value.unwrap())
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(result, flag);
                    let original = Rc::new(PortfolioThrown::from_value(None));
                    let early: $alias = Err(original.clone());
                    match early {
                        Err(error) => assert!(Rc::ptr_eq(&error, &original)),
                        Ok(_) => panic!("outer rejection changed"),
                    }
                    let late: $alias = Ok(Promise::resolved(Err(original.clone())));
                    match tokio::time::timeout(Duration::from_secs(1), late.unwrap())
                        .await
                        .unwrap()
                    {
                        Err(error) => {
                            assert!(Rc::ptr_eq(&error, &original));
                            assert!(error.value.is_none(), "undefined rejection remains absent");
                        }
                        Ok(_) => panic!("promise rejection changed"),
                    }
                }
            };
        }
        observe!(crate::cdp_signing::CdpAsync<bool>);
        observe!(crate::public_market_reads::PublicReadAsync<bool>);
        observe!(crate::exchange_lifecycle::LifecycleAsync<bool>);
        observe!(crate::exchange_tick::TickAsync<bool>);
        observe!(crate::order_sync::OrderSyncAsync<bool>);
        observe!(crate::order_edit::EditAsync<bool>);

        for flag in [false, true] {
            let original: CoinbaseStarbaseSessionFailure<bool> =
                Rc::new(CoinbaseStarbaseSessionError::Request(
                    CoinbaseStarbaseRequestError::Transport(flag),
                ));
            let cloned = original.clone();
            assert!(Rc::ptr_eq(&cloned, &original));
            assert!(matches!(&*cloned, CoinbaseStarbaseSessionError::Request(
                CoinbaseStarbaseRequestError::Transport(value)) if *value == flag));
            let auth: CoinbaseStarbaseAuthPromise<bool> = Promise::resolved(Err(original.clone()));
            let activation: CoinbaseStarbaseActivationPromise<bool> =
                Promise::resolved(Err(original.clone()));
            let auth_error = auth.await.expect_err("auth rejection");
            let activation_error = activation.await.expect_err("activation rejection");
            assert!(Rc::ptr_eq(&auth_error, &original));
            assert!(Rc::ptr_eq(&activation_error, &original));
            let success: CoinbaseStarbaseActivationPromise<bool> = Promise::resolved(Ok(()));
            success.await.unwrap();
        }
    });
}

#[test]
fn synchronized_order_aliases_keep_info_roots_and_native_number_bits() {
    let fields: Order = serde_json::from_value(json!({"id":"first","symbol":"BTCUSD",
        "status":"open","type":"limit","side":"buy","price":12,"amount":3,
        "filled":0,"remaining":3,"reduceOnly":false,"lastUpdate":0}))
    .unwrap();
    let info: ReadItem = Rc::new(RefCell::new(Input::Array(vec![
        Input::Float(-0.0),
        Input::Float(f64::NAN),
        Input::Float(f64::INFINITY),
        Input::Undefined,
        Input::Text(vec![0xd800]),
    ])));
    let first: SyncedOrderRoot<ReadItem> = Rc::new(RefCell::new(SyncedOrder {
        fields: fields.clone(),
        info: info.clone(),
    }));
    let second: SyncedOrderRoot<ReadItem> = Rc::new(RefCell::new(SyncedOrder {
        fields,
        info: Rc::new(RefCell::new(Input::Null)),
    }));
    second.borrow_mut().fields.id = "second".into();
    let orders: SyncedOrders<ReadItem> = Rc::new(RefCell::new(vec![first.clone(), second.clone()]));
    let captured = orders.clone();
    assert!(Rc::ptr_eq(&orders, &captured));
    assert_eq!(captured.borrow().len(), 2);
    assert!(Rc::ptr_eq(&captured.borrow()[0], &first));
    assert!(Rc::ptr_eq(&captured.borrow()[1], &second));
    assert!(Rc::ptr_eq(&captured.borrow()[0].borrow().info, &info));
    first.borrow_mut().fields.price = safe_cex_types::ModelField::Value(-0.0);
    second.borrow_mut().fields.price = safe_cex_types::ModelField::Value(f64::NAN);
    assert_eq!(
        captured.borrow()[0]
            .borrow()
            .fields
            .price
            .value()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(
        captured.borrow()[1]
            .borrow()
            .fields
            .price
            .value()
            .unwrap()
            .is_nan()
    );
    let retained = info.borrow();
    let Input::Array(values) = &*retained else {
        panic!("retained info array changed");
    };
    assert_eq!(values.len(), 5);
    assert!(matches!(values[0], Input::Float(value) if value.to_bits() == (-0.0_f64).to_bits()));
    assert!(matches!(values[1], Input::Float(value) if value.is_nan()));
    assert!(matches!(values[2], Input::Float(value) if value == f64::INFINITY));
    assert!(matches!(values[3], Input::Undefined));
    assert!(matches!(&values[4], Input::Text(units) if units.as_slice() == [0xd800]));
}

#[test]
fn bound_method_aliases_capture_the_real_production_host() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = native_runtime();
        let mut owners = Vec::new();
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let http = client::rest_http_host(
                runtime.clone(),
                Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            );
            let options =
                serde_json::from_value(json!({"key":"synthetic","secret":"synthetic"})).unwrap();
            owners.push(
                CoinbaseExchange::new(
                    name,
                    options,
                    Rc::new(DefaultStore::new()),
                    CoinbaseDependencies {
                        runtime: runtime.clone(),
                        http,
                        external_public_data: None,
                        signing_cache: Rc::new(CdpSigningCache::default()),
                        local_wall_time_to_utc: Rc::new(|value| value),
                    },
                )
                .unwrap(),
            );
        }
        let first: ClassFindMethod<Host> = owners[0].class.capture_finder();
        let second: ClassFindMethod<Host> = owners[1].class.capture_finder();
        assert!(first.same_function(&first.clone()));
        assert!(!first.same_function(&second));
        let first: ClassSupportedMethod<Host> = owners[0].class.capture_supported();
        let second: ClassSupportedMethod<Host> = owners[1].class.capture_supported();
        assert!(first.same_function(&first.clone()));
        assert!(!first.same_function(&second));
    });
}
