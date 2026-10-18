//! Own-entry effects and post-await work are observed separately.
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::PositionSide;
use safe_cex_venue_bitrue::{
    account_helpers::AccountPositionSnapshot, account_reads, leverage_reads, position_metadata,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut f = std::pin::pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ENTRY-LOCAL-READY"),
    }
}
fn request<'a>(
    calls: &'a Cell<usize>,
    polls: &'a Cell<usize>,
    data: Option<Value>,
) -> impl Future<Output = Result<Option<Value>, ExchangeError>> + 'a {
    calls.set(calls.get() + 1);
    async move {
        polls.set(polls.get() + 1);
        Ok(data)
    }
}
#[test]
fn native_entry_phases_match_actual_typescript() {
    let store = DefaultStore::default();
    let mut rows = Vec::new();
    macro_rules! leverage {
  ($method:ident,$name:literal,$expected:expr,$value:expr) => {{
   let calls=Cell::new(0);let polls=Cell::new(0);
   let future=leverage_reads::$method(&store,"BTC",false,|_,_|Ok(request(&calls,&polls,None)));
   assert_eq!(calls.get(),1,concat!("ENTRY-",$name));assert_eq!(polls.get(),0,"ENTRY-NO-EAGER-POLL");
   let result=($value)(ready(future));assert_eq!(result,$expected,concat!("ENTRY-RESULT-",$name));assert_eq!(polls.get(),1);
   let observed=serde_json::to_value(result).unwrap();
            let observed=observed.as_f64().map(|v|json!(format!("{:016x}",v.to_bits()))).unwrap_or(observed);
            rows.push(json!({"name":$name,"callsAtReturn":1,"result":observed}));
  }};
 }
    leverage!(
        fetch_and_sync_symbol_max_leverage,
        "sync",
        None::<f64>,
        |value: Option<f64>| value
    );
    leverage!(
        fetch_symbol_max_leverage,
        "leverage",
        1.0,
        |value: Result<f64, ExchangeError>| value.unwrap()
    );
    leverage!(update_symbol_metadata, "update", (), |value: ()| value);
    let calls = Cell::new(0);
    let polls = Cell::new(0);
    let emits = Cell::new(0);
    let future = account_reads::fetch_balance_and_positions_raw(
        |_| Ok(request(&calls, &polls, Some(json!({"code":0,"tag":17})))),
        |_| {
            emits.set(emits.get() + 1);
            Ok(())
        },
    );
    assert_eq!(calls.get(), 1, "ENTRY-raw");
    assert_eq!(polls.get(), 0, "ENTRY-RAW-NO-POLL");
    assert_eq!(emits.get(), 0);
    assert_eq!(ready(future).unwrap(), Some(json!({"code":0,"tag":17})));
    assert_eq!(polls.get(), 1);
    assert_eq!(emits.get(), 0);
    rows.push(json!({"name":"raw","callsAtReturn":1,"result":{"code":0,"tag":17}}));
    let calls = Cell::new(0);
    let polls = Cell::new(0);
    let before = store.balance_handle().unwrap();
    let future = account_reads::fetch_balance(
        &store,
        || Ok(request(&calls, &polls, None)),
        |warning| panic!("unexpected refusal: {warning}"),
    );
    assert_eq!(calls.get(), 1, "ENTRY-balance");
    assert_eq!(polls.get(), 0, "ENTRY-BALANCE-NO-POLL");
    let balance = ready(future).unwrap();
    assert!(
        before
            .native_value()
            .downcast::<safe_cex_exchange::js_runtime::value::Js>()
            .unwrap()
            .same_reference(
                &balance
                    .native_value()
                    .downcast::<safe_cex_exchange::js_runtime::value::Js>()
                    .unwrap()
            ),
        "ENTRY-BALANCE-OLD-HANDLE"
    );
    assert_eq!(polls.get(), 1);
    rows.push(json!({"name":"balance","callsAtReturn":1,"result":"retained"}));
    let cache = RefCell::new(Vec::new());
    let symbols = vec!["BTC".to_owned()];
    let calls = Cell::new(0);
    let polls = Cell::new(0);
    let disposed = Cell::new(0);
    let future = position_metadata::fetch_position_metadata(
        &cache,
        &symbols,
        None,
        || Ok(request(&calls, &polls, None)),
        || {
            disposed.set(disposed.get() + 1);
            false
        },
    );
    assert_eq!(calls.get(), 1, "ENTRY-position");
    assert_eq!(polls.get(), 0, "ENTRY-POSITION-NO-POLL");
    assert_eq!(disposed.get(), 0, "ENTRY-POSITION-DISPOSED-AFTER-AWAIT");
    assert!(ready(future).unwrap().is_empty());
    assert_eq!(polls.get(), 1);
    assert_eq!(disposed.get(), 1);
    rows.push(json!({"name":"position","callsAtReturn":1,"result":[]}));
    // No-await metadata must mutate and select before returning, retaining selected handles.
    let leverage = json!(17);
    let kind = json!(2);
    let supplied = [AccountPositionSnapshot {
        symbol: "BTC",
        side: PositionSide::Long,
        leverage_level: &leverage,
        position_type: &kind,
    }];
    let future = position_metadata::fetch_position_metadata(
        &cache,
        &symbols,
        Some(&supplied),
        || Ok(std::future::ready(Ok(None))),
        || false,
    );
    assert_eq!(cache.borrow().len(), 1, "ENTRY-CACHE-WRITE");
    assert_eq!(
        cache.borrow()[0].1.borrow().leverage.to_bits(),
        17.0f64.to_bits(),
        "ENTRY-CACHE-VALUE"
    );
    let selected = cache.borrow()[0].1.clone();
    cache.borrow_mut().clear();
    let result = ready(future).unwrap();
    assert_eq!(result.len(), 1, "ENTRY-CACHE-SELECTED");
    assert!(std::rc::Rc::ptr_eq(&selected, &result[0]));
    // An explicit empty supplied array clears eagerly, even when disposed.
    let future = position_metadata::fetch_position_metadata(
        &cache,
        &symbols,
        Some(&supplied),
        || Ok(std::future::ready(Ok(None))),
        || true,
    );
    assert_eq!(cache.borrow().len(), 1, "ENTRY-DISPOSED-REBUILD-FIRST");
    assert!(ready(future).unwrap().is_empty());
    let future = position_metadata::fetch_position_metadata(
        &cache,
        &symbols,
        Some(&[]),
        || Ok(std::future::ready(Ok(None))),
        || false,
    );
    assert!(cache.borrow().is_empty(), "ENTRY-EMPTY-CLEARS");
    assert!(ready(future).unwrap().is_empty());
    // Synchronous request-start errors emit now; emitter errors remain future errors.
    let mut begin_errors = Vec::new();
    for fail_emit in [false, true] {
        let emits = Cell::new(0);
        let emitted = RefCell::new(None);
        let future = account_reads::fetch_balance_and_positions_raw(
            |_| {
                let mut error = ExchangeError::new("start rejected");
                error.response = Some(json!({"data":{"msg":"source-detail"}}));
                Err::<std::future::Ready<Result<Option<Value>, ExchangeError>>, _>(error)
            },
            |message| {
                emits.set(emits.get() + 1);
                *emitted.borrow_mut() = message.cloned();
                if fail_emit {
                    {
                        let mut error = ExchangeError::new("emit rejected");
                        error.name = "SubscriberError".into();
                        error.store_subscriber_ids = vec![9, 3];
                        Err(error)
                    }
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(emits.get(), 1, "ENTRY-SYNC-ERROR-EMIT");
        assert_eq!(*emitted.borrow(), Some(json!("source-detail")));
        let result = ready(future);
        if fail_emit {
            let err = result.unwrap_err();
            assert_eq!(err.name, "SubscriberError");
            assert_eq!(err.message, "emit rejected");
            assert_eq!(err.store_subscriber_ids, vec![9, 3]);
        } else {
            assert_eq!(result.unwrap(), None);
        }
        begin_errors.push(json!({"emitFails":fail_emit,"emitsAtReturn":1,"message":"source-detail","outcome":if fail_emit {"SubscriberError"} else {"undefined"}}));
    }
    // Only after all native assertions, execute source and compare captured observations.
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/entry_timing_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ENTRY-SOURCE:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["rows"], json!(rows));
    assert_eq!(
        source["cache"],
        json!({"beforeReturn":17,"selectedAfterClear":17,"emptyCleared":true,"disposedRebuilt":true})
    );
    assert_eq!(source["beginErrors"], json!(begin_errors));
    assert_eq!(
        source["rows"].as_array().unwrap().len(),
        6,
        "ENTRY-SOURCE-SIX-ROWS"
    );
}
