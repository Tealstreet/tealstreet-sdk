//! Real hosted account/algorithm lifecycles across COSIGNED ownership changes.
use safe_cex_exchange_server::{
    accounts::{Account, Disposal, Registry, testing},
    algos::AlgoInput,
    envelope::InitPayload,
    threads::EngineThreads,
};
use safe_cex_remote_signer::Outbox;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::LocalSet;

struct Fixture {
    registry: Arc<Registry>,
    account: Arc<Account>,
    init: InitPayload,
    frames: Arc<Mutex<Vec<Value>>>,
}

impl Fixture {
    async fn new() -> Self {
        let registry = Arc::new(Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        ));
        let init = InitPayload {
            account_name: "acct".into(),
            exchange_name: "bitget".into(),
            key: "synthetic".into(),
            ..Default::default()
        };
        let account = testing::trading_account(&registry, "u", &init);
        let fixture = Self {
            registry,
            account,
            init,
            frames: Arc::default(),
        };
        fixture.join(7).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        fixture
    }

    fn outbox(&self) -> Outbox {
        let frames = self.frames.clone();
        Arc::new(move |text| {
            frames
                .lock()
                .unwrap()
                .push(serde_json::from_str(&text).unwrap());
            true
        })
    }

    async fn join(&self, socket: u64) {
        let joined = self
            .registry
            .obtain(
                "u",
                &self.init,
                false,
                socket,
                Some((format!("synthetic-{socket}"), u64::MAX)),
                self.outbox(),
            )
            .await
            .unwrap();
        assert!(
            Arc::ptr_eq(&joined, &self.account),
            "same account, not a rebuild"
        );
        assert_eq!(self.account.link.as_ref().unwrap().socket(), socket);
    }

    async fn wait(&self, matches: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(frame) = self
                    .frames
                    .lock()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|f| matches(f))
                    .cloned()
                {
                    return frame;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("expected hosted lifecycle event")
    }

    async fn start_twap(&self) {
        let options = json!({"accountName":"acct","activeMarket":{
            "id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
            "precision":{"amount":0.001,"price":0.1},
            "limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},
            "isPerpetual":true},"size":0.1,"duration":10,"ordersCount":10,"side":"buy",
            "chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,
            "useLimitOrders":false,"useChaserOrders":false,"origin":"web"});
        testing::algo_op(&self.account, "twap:start", &options.to_string());
        self.executed(1).await;
    }

    async fn executed(&self, count: u64) {
        self.wait(|frame| {
            frame["d"]["op"] == "twap:stateUpdate"
                && frame["d"]["data"].as_object().is_some_and(|rows| {
                    rows.values().any(|row| {
                        row["state"]["ordersExecuted"]
                            .as_u64()
                            .is_some_and(|n| n >= count)
                    })
                })
        })
        .await;
    }

    fn signing(&self) -> u64 {
        self.account.link.as_ref().unwrap().socket()
    }
}

#[tokio::test(start_paused = true)]
async fn same_owner_reconnect_inside_cosigned_window_keeps_running_twap() {
    LocalSet::new()
        .run_until(async {
            let f = Fixture::new().await;
            f.start_twap().await;
            let owner = f.account.twap_owner.snapshot();
            f.registry
                .park("r1".into(), "u", vec![(f.account.clone(), 7)]);
            tokio::time::sleep(Duration::from_secs(30)).await;
            let resumed =
                f.registry
                    .resume("r1", "u", 8, ("synthetic-8".into(), u64::MAX), &|_| {
                        f.outbox()
                    });
            assert!(Arc::ptr_eq(&resumed[0], &f.account));
            assert_eq!(f.signing(), 8);
            assert_eq!(f.account.twap_owner.snapshot(), owner);
            tokio::time::sleep(Duration::from_secs(31)).await;
            f.executed(2).await;
            assert_eq!(
                f.registry.account_count(),
                1,
                "old grace timer must not dispose the resumed job"
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn same_owner_two_client_handover_keeps_running_twap() {
    LocalSet::new()
        .run_until(async {
            let f = Fixture::new().await;
            f.start_twap().await;
            let owner = f.account.twap_owner.snapshot();
            f.join(8).await;
            f.join(7).await; // An older tab's newer init takes signing ownership.
            assert_eq!(f.signing(), 7);
            assert_eq!(f.account.twap_owner.snapshot(), owner);
            f.registry
                .park("r1".into(), "u", vec![(f.account.clone(), 7)]);
            assert_eq!(
                f.signing(),
                8,
                "the other live same-owner client keeps signing"
            );
            tokio::time::sleep(Duration::from_secs(61)).await;
            f.executed(2).await;
            assert_eq!(f.registry.account_count(), 1);
            assert_eq!(f.account.twap_owner.snapshot(), owner);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn signing_fallback_uses_latest_same_owner_init_not_largest_socket_id() {
    LocalSet::new()
        .run_until(async {
            let f = Fixture::new().await;
            f.start_twap().await;
            let owner = f.account.twap_owner.snapshot();
            f.join(9).await;
            f.join(8).await; // Newer init on an older socket.
            f.join(10).await;
            f.registry.release(&f.account, 10, Disposal::SocketClosed);
            assert_eq!(f.signing(), 8);
            assert_eq!(f.account.twap_owner.snapshot(), owner);
            tokio::time::sleep(Duration::from_secs(61)).await;
            f.executed(2).await;
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn different_owner_stops_running_twap_and_fences_queued_work() {
    LocalSet::new()
        .run_until(async {
            let f = Fixture::new().await;
            f.start_twap().await;
            let owner = f.account.twap_owner.snapshot();
            f.account
                .algos
                .as_ref()
                .unwrap()
                .send(AlgoInput::Op {
                    op: "twap:resume".into(),
                    data: "\"retired\"".into(),
                    origin: "old-u".into(),
                    twap_owner: owner.clone(),
                    action: None,
                })
                .unwrap();
            assert!(f.account.twap_owner.rebind_owner("other-user"));
            assert_ne!(f.account.twap_owner.snapshot().generation, owner.generation);
            f.wait(|frame| {
                frame
                    .to_string()
                    .contains("Algo operation cancelled-for-owner-change")
            })
            .await;
            tokio::time::sleep(Duration::from_secs(61)).await;
            f.wait(|frame| {
                frame
                    .to_string()
                    .contains("TWAPs stopped: authenticated connection binding changed")
            })
            .await;
            f.wait(|frame| {
                frame["d"]["op"] == "twap:stateUpdate" && frame["d"]["data"] == json!({})
            })
            .await;
            f.frames.lock().unwrap().clear();
            f.start_twap().await; // Fresh work under the new owner's generation still runs.
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn returning_owner_cannot_reuse_retired_generation_or_queued_work() {
    LocalSet::new()
        .run_until(async {
            let f = Fixture::new().await;
            f.start_twap().await;
            let owner = f.account.twap_owner.snapshot();
            f.account
                .algos
                .as_ref()
                .unwrap()
                .send(AlgoInput::Op {
                    op: "twap:resume".into(),
                    data: "\"retired\"".into(),
                    origin: "old-u".into(),
                    twap_owner: owner.clone(),
                    action: None,
                })
                .unwrap();
            // Both identity changes happen during the deferral, before the algo task runs.
            assert!(f.account.twap_owner.rebind_owner("other-user"));
            assert!(f.account.twap_owner.rebind_owner("u"));
            assert_ne!(f.account.twap_owner.snapshot().generation, owner.generation);
            f.wait(|frame| {
                frame
                    .to_string()
                    .contains("Algo operation cancelled-for-owner-change")
            })
            .await;
            tokio::time::sleep(Duration::from_secs(61)).await;
            f.wait(|frame| {
                frame
                    .to_string()
                    .contains("TWAPs stopped: authenticated connection binding changed")
            })
            .await;
            f.frames.lock().unwrap().clear();
            f.start_twap().await; // Fresh work under the returned owner's new generation still runs.
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn cosigned_window_expiry_stops_running_twap_and_records_reason() {
    LocalSet::new()
        .run_until(async {
            let f = Fixture::new().await;
            f.start_twap().await;
            f.registry
                .park("r1".into(), "u", vec![(f.account.clone(), 7)]);
            tokio::time::sleep(Duration::from_secs(59)).await;
            assert_eq!(f.registry.account_count(), 1);
            assert_eq!(f.account.running.counts().1, 1);
            tokio::time::sleep(Duration::from_secs(2)).await;
            assert_eq!(f.registry.account_count(), 0);
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(notice) =
                        f.registry
                            .take_notice("u", "acct", &f.account.fingerprint, Some("r1"))
                    {
                        assert!(notice.summary().contains("no browser for 60s"));
                        assert!(notice.summary().contains("TWAP"));
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("expiry records the real algorithm stop reason");
        })
        .await;
}
