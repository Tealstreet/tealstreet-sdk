#![cfg(not(target_arch = "wasm32"))]

use safe_cex_exchange::{
    exchange::Exchange,
    native::{NativeHttpHost, native_runtime},
    native_store::DefaultStore,
    socket_lifecycle::SocketSlot,
    websocket::WsRuntime,
};
use safe_cex_store::{Store, StoreUpdate, virtual_clock::VIRTUAL_CLOCK};
use safe_cex_transport::rate_limit::RateLimitClock;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use safe_cex_venue_bybit::{BybitDependencies, BybitExchange};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

struct RuntimeClock(Rc<dyn WsRuntime>);
impl RateLimitClock for RuntimeClock {
    fn now_ms(&self) -> u64 {
        self.0.now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        Box::pin(async move {
            self.0
                .sleep(ms)
                .await
                .map_err(|error| safe_cex_transport::TransportError::Host(error.message))
        })
    }
}

#[allow(
    clippy::arc_with_non_send_sync,
    reason = "native rate clock runs on the LocalSet"
)]
async fn observe(name: ExchangeName) {
    let runtime = native_runtime();
    let store = Rc::new(DefaultStore::new());
    let venue = BybitExchange::create(
        name.clone(),
        serde_json::from_value(json!({"key":"", "secret":"", "tuning":{"consoleLogging":false}}))
            .unwrap(),
        store.clone(),
        BybitDependencies {
            runtime: runtime.clone(),
            remote_signer: None,
            wire: Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            http_host: Rc::new(NativeHttpHost::default()),
            retry_clock: Rc::new(RuntimeClock(runtime.clone())),
            clock: VIRTUAL_CLOCK.clone(),
            external_public_data: None,
            relay_enabled: false,
        },
    )
    .unwrap();
    let slot = if name == ExchangeName::Bybitinverse {
        SocketSlot::PublicSecondary
    } else {
        SocketSlot::Public
    };
    let socket = venue.base().get_socket(slot).unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-prototype-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut observations = 0;
    for case in cases.as_array().unwrap() {
        if case["variant"] != serde_json::to_value(&name).unwrap() {
            continue;
        }
        let tickers = serde_json::from_value(case["tickers"].clone()).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                tickers: Some(tickers),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let count = Rc::new(Cell::new(0));
        let seen = count.clone();
        let previous = RefCell::new(
            store
                .tickers()
                .unwrap()
                .iter()
                .map(|ticker| (ticker.last.to_bits(), ticker.bid.to_bits()))
                .collect::<Vec<_>>(),
        );
        let unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |state, _| {
                let current: Vec<_> = state
                    .tickers
                    .iter()
                    .map(|ticker| (ticker.last.to_bits(), ticker.bid.to_bits()))
                    .collect();
                // The socket also notifies handler statistics; count ticker commits.
                if current != *previous.borrow() {
                    seen.set(seen.get() + 1);
                    *previous.borrow_mut() = current;
                }
                Ok(())
            }),
        );
        let result = socket.on_message(&SocketFrame::Text(case["frame"].to_string()));
        assert_eq!(
            result.as_ref().err().map(|e| e.name.as_str()),
            case["error"]["name"].as_str(),
            "BYBIT-TICKER-PROTOTYPE error class: {case}"
        );
        assert_eq!(
            result.err().map(|e| e.message),
            case["error"]["message"].as_str().map(str::to_owned),
            "BYBIT-TICKER-PROTOTYPE parse precedes update: {case}"
        );
        assert_eq!(
            count.get(),
            case["notifications"].as_u64().unwrap(),
            "{case}"
        );
        let tickers = store.tickers().unwrap();
        for (ticker, expected) in tickers.iter().zip(case["result"].as_array().unwrap()) {
            assert_eq!(ticker.last, expected["last"].as_f64().unwrap(), "{case}");
            assert_eq!(ticker.bid, expected["bid"].as_f64().unwrap(), "{case}");
        }
        assert_eq!(tickers.len(), case["result"].as_array().unwrap().len());
        drop(tickers);
        unsubscribe();
        observations += 1;
    }
    assert_eq!(observations, 48, "source case census");
    venue.dispose().unwrap();
}

#[test]
fn constructed_public_socket_uses_source_ticker_prototype_lookup() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitinverse,
            ExchangeName::Bybitspot,
        ] {
            observe(name).await;
        }
    });
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn constructed_public_reads_do_not_strand_idle_limiter_replay_deadlines() {
    use safe_cex_exchange::{native, replay_clock};
    use safe_cex_transport::{
        RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody,
    };
    let source = std::process::Command::new("node")
        .args([
            "-e",
            r#"
const assert = require('node:assert/strict');
const axios = require('axios');
const rateLimit = require('axios-rate-limit');
let now = 0, next = 0;
const timers = new Map();
global.setTimeout = (callback, delay) => {
  const handle = {id: ++next, ref() {}, unref() {}};
  timers.set(handle, {at: now + delay, callback});
  return handle;
};
global.clearTimeout = handle => timers.delete(handle);
Date.now = () => 0;
async function advance(time) {
  now = time;
  for (let i = 0; i < 100; i++) {
    await Promise.resolve();
    for (const [handle, timer] of timers) {
      if (timer.at <= now) { timers.delete(handle); timer.callback(); }
    }
  }
}
(async () => {
  let sent = 0;
  const client = rateLimit(axios.create({adapter: async config => {
    sent++;
    return {status:200, statusText:'OK', headers:{}, data:{}, config};
  }}), {maxRequests:5, perMilliseconds:1000});
  const first = Promise.all(Array.from({length:3}, () => client.get('/ticker')));
  await advance(0); await first;
  assert.equal(sent, 3);
  await advance(2341.45725);
  const second = Promise.all(Array.from({length:5}, () => client.get('/ticker')));
  await advance(2341.45725); await second;
  assert.equal(sent, 8);
  assert.equal(Date.now(), 0);
  console.log('independent rate reset PASS');
})().catch(error => { console.error(error); process.exitCode = 1; });
"#,
        ])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    assert!(String::from_utf8_lossy(&source.stdout).contains("independent rate reset PASS"));
    struct Wire;
    impl RestAdapter for Wire {
        fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    headers: Default::default(),
                    reason: None,
                    body: RestResponseBody::Encoded(
                        br#"{"retCode":0,"retMsg":"OK","result":{"list":[]}}"#.to_vec(),
                    ),
                })
            })
        }
    }
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = native::native_replay_runtime(1_700_000_000_000.);
        let venue = BybitExchange::create(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"", "secret":"", "publicDataOnly":true})).unwrap(),
            Rc::new(DefaultStore::new()),
            BybitDependencies {
                runtime: runtime.clone(),
                remote_signer: None,
                wire: Arc::new(Wire),
                http_host: Rc::new(NativeHttpHost::default()),
                retry_clock: Rc::new(RuntimeClock(runtime.clone())),
                clock: VIRTUAL_CLOCK.clone(),
                external_public_data: None,
                relay_enabled: false,
            },
        )
        .unwrap();
        assert!(venue.fetch_tickers().await.unwrap().is_empty());
        let clock = runtime.replay_clock.as_ref().unwrap();
        replay_clock::activate(clock).unwrap();
        let advance = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            replay_clock::advance(clock, 2341.45725),
        )
        .await;
        assert!(
            advance.is_ok(),
            "BYBIT_IDLE_LIMITER_ADVANCE: completed HTTP leaves no parked reset sleep"
        );
        advance.unwrap().unwrap();
        assert!(venue.fetch_tickers().await.unwrap().is_empty());
        venue.dispose().unwrap();
        drop(venue);
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            native::replay_raw_tasks(&runtime)["waitingTimers"],
            0,
            "BYBIT_RATE_DISPOSE cancels pending window reset"
        );
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            replay_clock::advance(clock, 5000.),
        )
        .await
        .unwrap()
        .unwrap();
        safe_cex_store::virtual_clock::set_thread_host_time(None);
    });
}

#[test]
fn dropping_native_rate_limiter_cancels_its_independent_reset() {
    use safe_cex_exchange::{
        exchange::{BaseExchange, ExchangeSettings},
        native,
        runtime::rate_limit_scheduler,
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = native::native_replay_runtime(1_700_000_000_000.);
        let base = BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"", "secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let limiter = safe_cex_transport::rate_limit::RateLimiter::new(
            rate_limit_scheduler(&base),
            1,
            60_000,
        );
        limiter.acquire().await.unwrap();
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            native::replay_raw_tasks(&runtime)["waitingTimers"],
            1,
            "reset is a real owned timer"
        );
        drop(limiter);
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            native::replay_raw_tasks(&runtime)["waitingTimers"],
            0,
            "BYBIT_RATE_DROP cancels the reset on limiter drop"
        );
        safe_cex_store::virtual_clock::set_thread_host_time(None);
    });
}
