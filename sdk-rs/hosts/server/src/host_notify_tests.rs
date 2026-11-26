use super::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn scope() -> Scope {
    Scope::new("auth0|u1", "bybit", "synthetic-main", "venue-account")
}

fn draft(natural_key: &str) -> Draft {
    Draft {
        kind: Kind::Fill,
        severity: Severity::Info,
        natural_key: natural_key.into(),
        fields: Fields::default()
            .symbol("BTCUSDT")
            .side(Side::Buy)
            .verb(Verb::Filled),
        deadline: None,
        collapse: None,
    }
}

fn notification(natural_key: &str) -> HostNotification {
    HostNotification::build(&scope(), draft(natural_key), false, 1_795_100_000_000)
}

/// Scripted gateway: answers in order, then accepts; records every body.
/// Every body the scripted gateway received, with its arrival time.
type Received = Arc<Mutex<Vec<(tokio::time::Instant, Value)>>>;

fn scripted(answers: Vec<Delivery>) -> (Transport, Received) {
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let answers = Arc::new(Mutex::new(VecDeque::from(answers)));
    let recorded = bodies.clone();
    let transport: Transport = Arc::new(move |body: Vec<u8>| {
        recorded.lock().unwrap().push((
            tokio::time::Instant::now(),
            serde_json::from_slice(&body).unwrap(),
        ));
        let answer = answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Delivery::Accepted);
        Box::pin(async move { answer })
    });
    (transport, bodies)
}

fn ids(body: &Value) -> Vec<String> {
    body["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[test]
fn unset_url_is_off_and_a_set_url_requires_token_and_box() {
    assert!(
        Config::parse(None, Some("t"), Some("exchange-01"), None)
            .unwrap()
            .is_none()
    );
    let config = Config::parse(
        Some("https://news.tealstreet.io/internal/host-notifications"),
        Some("secret-token"),
        None,
        Some("01"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        config.box_id, "01",
        "router box id is the fallback identity"
    );
    let shown = format!("{config:?}");
    assert!(!shown.contains("secret-token") && shown.contains("<redacted>"));
    let labelled = Config::parse(
        Some("https://g/x"),
        Some("t"),
        Some("exchange-01"),
        Some("01"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(labelled.box_id, "exchange-01");
    for (url, token, label) in [
        ("http://gateway.example/x", Some("t"), Some("exchange-01")),
        ("https://user:pw@g/x", Some("t"), Some("exchange-01")),
        ("not a url", Some("t"), Some("exchange-01")),
        ("https://g/x", None, Some("exchange-01")),
        ("https://g/x", Some("has space"), Some("exchange-01")),
        ("https://g/x", Some("t"), None),
        ("https://g/x", Some("t"), Some("bad box")),
    ] {
        assert!(
            Config::parse(Some(url), token, label, None).is_err(),
            "{url} {token:?} {label:?}"
        );
    }
    assert!(Config::parse(Some("http://127.0.0.1:9/x"), Some("t"), Some("b"), None).is_ok());
}

#[test]
fn ids_follow_the_contract_hash_and_are_deterministic() {
    assert_eq!(
        notification_id("auth0|u1", &"ab".repeat(32), Kind::Fill, "order-1\0filled"),
        "cb6db82fb69385b8ae8c728d7925b827f64bd1edc6e1fc4deb78a89de070f0db"
    );
    assert_eq!(notification("k").id, notification("k").id);
    assert_ne!(notification("k").id, notification("k2").id);
    let other_user = HostNotification::build(
        &Scope::new("auth0|u2", "bybit", "synthetic-main", "venue-account"),
        draft("k"),
        false,
        1,
    );
    assert_ne!(other_user.id, notification("k").id);
    assert_ne!(other_user.account_digest, notification("k").account_digest);
    // Retries resend the identical object, so the presence/clock never change the id.
    assert_eq!(
        HostNotification::build(&scope(), draft("k"), true, 2).id,
        notification("k").id
    );
}

#[test]
fn only_allowlisted_fields_serialize_and_labels_never_leave_the_box() {
    let built = HostNotification::build(
        &scope(),
        Draft {
            kind: Kind::ShutdownWarning,
            severity: Severity::Warning,
            natural_key: "1795103600000".into(),
            fields: Fields::default()
                .symbol("BTCUSDT")
                .side(Side::Sell)
                .verb(Verb::PartiallyFilled)
                .algo(Algo::Twap)
                .cause(Cause::NoClient48h),
            deadline: Some(1_795_103_600_000),
            collapse: None,
        },
        true,
        1_795_100_000_000,
    );
    let value = serde_json::to_value(&built).unwrap();
    let keys: BTreeSet<_> = value.as_object().unwrap().keys().cloned().collect();
    let allowed: BTreeSet<String> = [
        "id",
        "user",
        "accountDigest",
        "venue",
        "kind",
        "severity",
        "occurredAt",
        "clientPresent",
        "collapseKey",
        "deadline",
        "fields",
    ]
    .map(String::from)
    .into();
    assert_eq!(keys, allowed);
    assert_eq!(
        value["fields"],
        json!({"symbol":"BTCUSDT","side":"sell","verb":"partiallyFilled","algo":"twap","cause":"noClient48h"})
    );
    assert_eq!(value["kind"], "delegated.shutdownWarning");
    assert_eq!(value["clientPresent"], true);
    let text = value.to_string();
    assert!(
        !text.contains("synthetic-main"),
        "account label must never serialize"
    );
    assert_eq!(value["accountDigest"].as_str().unwrap().len(), 64);
    assert_eq!(
        value["collapseKey"],
        format!(
            "delegated.shutdownWarning:{}",
            &value["accountDigest"].as_str().unwrap()[..16]
        )
    );

    // Optional fields disappear rather than serializing null.
    let bare = serde_json::to_value(HostNotification::build(
        &scope(),
        Draft {
            kind: Kind::Shutdown,
            severity: Severity::Critical,
            natural_key: "x".into(),
            fields: Fields::default(),
            deadline: None,
            collapse: Some("opaque".into()),
        },
        false,
        1,
    ))
    .unwrap();
    assert_eq!(bare["fields"], json!({}));
    assert!(bare.get("deadline").is_none());
    assert_eq!(bare["collapseKey"], "delegated.shutdown:opaque");
}

#[test]
fn causes_and_verbs_use_the_contract_spelling() {
    fn text<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }
    assert_eq!(text(&Cause::NoClient48h), "\"noClient48h\"");
    assert_eq!(text(&Cause::ProxyTokenExpired), "\"proxyTokenExpired\"");
    assert_eq!(text(&Cause::ServerRestart), "\"serverRestart\"");
    assert_eq!(text(&Cause::AlgoError), "\"algoError\"");
    assert_eq!(text(&Cause::AlgoFinished), "\"algoFinished\"");
    assert_eq!(text(&Cause::VenueRejected), "\"venueRejected\"");
    assert_eq!(text(&Verb::Filled), "\"filled\"");
    assert_eq!(text(&Verb::Rejected), "\"rejected\"");
    assert_eq!(text(&Verb::Stopped), "\"stopped\"");
    assert_eq!(text(&Verb::Finished), "\"finished\"");
    assert_eq!(text(&Algo::Chase), "\"chase\"");
    assert_eq!(text(&Algo::Exec), "\"exec\"");
    assert_eq!(text(&Kind::OrderRejected), "\"orderRejected\"");
    assert_eq!(text(&Kind::AlgoStopped), "\"algoStopped\"");
    assert_eq!(text(&Kind::Shutdown), "\"delegated.shutdown\"");
    assert_eq!(text(&Severity::Critical), "\"critical\"");
}

#[test]
fn symbols_and_venues_that_are_not_plain_names_are_omitted() {
    for bad in [
        "",
        "BTC USDT",
        "BTCUSDT\n",
        "{\"key\":1}",
        &"A".repeat(41),
        "ключ",
    ] {
        assert_eq!(Fields::default().symbol(bad), Fields::default(), "{bad:?}");
    }
    for good in ["BTCUSDT", "BTC-USDT-SWAP", "BTC/USDT:USDT", "1000PEPE_USDT"] {
        assert_ne!(Fields::default().symbol(good), Fields::default(), "{good}");
    }
    assert_eq!(Scope::new("u", "by bit", "a", "v").venue, "unknown");
    assert_eq!(
        Scope::new("u", "lighter-robinhood", "a", "v").venue,
        "lighter-robinhood"
    );
}

#[test]
fn outbox_drops_oldest_at_capacity() {
    let notifier = HostNotifier::unstarted();
    for index in 0..MAX_OUTBOX + 5 {
        notifier.push(notification(&index.to_string()));
    }
    let queued = notifier.queued();
    assert_eq!(queued.len(), MAX_OUTBOX);
    assert_eq!(queued[0].id, notification("5").id);
    assert_eq!(notifier.shared.queue.lock().unwrap().dropped, 5);
    let mut queue = Queue::default();
    for index in 0..MAX_OUTBOX {
        queue.push(notification(&index.to_string()));
    }
    let batch = queue.take_batch();
    assert_eq!(batch.len(), MAX_BATCH);
    queue.push(notification("late"));
    queue.requeue(batch);
    assert_eq!(queue.items.len(), MAX_OUTBOX);
    assert_eq!(queue.dropped, 1, "a requeue past the cap drops the oldest");
    assert_eq!(queue.items.back().unwrap().id, notification("late").id);
}

fn of_kind(kind: Kind, natural_key: &str) -> HostNotification {
    HostNotification::build(
        &scope(),
        Draft {
            kind,
            ..draft(natural_key)
        },
        false,
        1,
    )
}

#[test]
fn a_full_outbox_evicts_fills_then_rejections_never_shutdown_notices() {
    let mut queue = Queue::default();
    queue.push(of_kind(Kind::Shutdown, "shutdown"));
    queue.push(of_kind(Kind::ShutdownWarning, "warning"));
    queue.push(of_kind(Kind::AlgoStopped, "stopped"));
    queue.push(of_kind(Kind::OrderRejected, "rejected"));
    for index in 0..MAX_OUTBOX - 4 {
        queue.push(of_kind(Kind::Fill, &index.to_string()));
    }
    for index in 0..MAX_OUTBOX {
        queue.push(of_kind(Kind::Fill, &format!("late-{index}")));
    }
    let kinds = |queue: &Queue| queue.items.iter().map(|item| item.kind).collect::<Vec<_>>();
    assert_eq!(queue.items.len(), MAX_OUTBOX);
    assert_eq!(
        kinds(&queue)[..4],
        [
            Kind::Shutdown,
            Kind::ShutdownWarning,
            Kind::AlgoStopped,
            Kind::OrderRejected
        ],
        "fills go first, oldest first"
    );
    assert_eq!(queue.items[4].id, of_kind(Kind::Fill, "late-4").id);
    // No fills left to evict for a priority notice: the rejection goes next.
    let mut queue = Queue::default();
    queue.push(of_kind(Kind::OrderRejected, "rejected"));
    for index in 0..MAX_OUTBOX - 1 {
        queue.push(of_kind(Kind::AlgoStopped, &index.to_string()));
    }
    queue.push(of_kind(Kind::Shutdown, "shutdown"));
    assert!(kinds(&queue).iter().all(|kind| kind.priority()));
    // All priority: an incoming fill is dropped, an incoming shutdown evicts the oldest.
    queue.push(of_kind(Kind::Fill, "dropped"));
    assert!(queue.items.iter().all(|item| item.kind != Kind::Fill));
    queue.push(of_kind(Kind::Shutdown, "newest"));
    assert_eq!(queue.items.len(), MAX_OUTBOX);
    assert_eq!(queue.items[0].id, of_kind(Kind::AlgoStopped, "1").id);
    assert_eq!(
        queue.items.back().unwrap().id,
        of_kind(Kind::Shutdown, "newest").id
    );
    assert_eq!(queue.dropped, 3);
}

#[tokio::test(start_paused = true)]
async fn batches_hold_at_most_fifty() {
    let (transport, bodies) = scripted(Vec::new());
    let notifier = HostNotifier::with_transport(transport);
    for index in 0..120 {
        notifier.push(notification(&index.to_string()));
    }
    assert!(notifier.flush(Duration::from_secs(1)).await);
    let bodies = bodies.lock().unwrap();
    let sizes: Vec<_> = bodies.iter().map(|(_, body)| ids(body).len()).collect();
    assert_eq!(sizes, [50, 50, 20]);
    assert!(bodies.iter().all(|(_, body)| body["v"] == 1));
    let sent: Vec<_> = bodies.iter().flat_map(|(_, body)| ids(body)).collect();
    let expected: Vec<_> = (0..120)
        .map(|index| notification(&index.to_string()).id)
        .collect();
    assert_eq!(sent, expected, "delivery keeps order");
}

#[tokio::test(start_paused = true)]
async fn failures_retry_the_same_batch_with_backoff_and_refusals_drop_it() {
    let (transport, bodies) = scripted(vec![
        Delivery::Retry(None),
        Delivery::Retry(Some(503)),
        Delivery::Accepted,
        Delivery::Refused(400),
    ]);
    let notifier = HostNotifier::with_transport(transport);
    notifier.push(notification("a"));
    settle().await;
    tokio::time::advance(Duration::from_millis(999)).await;
    settle().await;
    assert_eq!(
        bodies.lock().unwrap().len(),
        1,
        "first retry waits one second"
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    settle().await;
    assert_eq!(bodies.lock().unwrap().len(), 2);
    tokio::time::advance(Duration::from_millis(1999)).await;
    settle().await;
    assert_eq!(bodies.lock().unwrap().len(), 2, "backoff doubles");
    tokio::time::advance(Duration::from_millis(1)).await;
    settle().await;
    {
        let bodies = bodies.lock().unwrap();
        assert_eq!(bodies.len(), 3);
        assert!(
            bodies
                .iter()
                .all(|(_, body)| ids(body) == [notification("a").id])
        );
    }
    assert!(notifier.queued().is_empty());
    notifier.push(notification("b"));
    settle().await;
    assert_eq!(bodies.lock().unwrap().len(), 4);
    assert!(
        notifier.flush(Duration::ZERO).await,
        "a refused batch is dropped"
    );
    assert!(notifier.queued().is_empty());
}

#[tokio::test(start_paused = true)]
async fn backoff_caps_at_ten_minutes_and_flush_is_bounded() {
    let (transport, bodies) = scripted(vec![Delivery::Retry(Some(502)); 100]);
    let notifier = HostNotifier::with_transport(transport);
    notifier.push(notification("a"));
    settle().await;
    for _ in 0..15 {
        tokio::time::advance(MAX_BACKOFF).await;
        settle().await;
    }
    let bodies_now = bodies.lock().unwrap().clone();
    let gaps: Vec<_> = bodies_now
        .windows(2)
        .map(|pair| pair[1].0 - pair[0].0)
        .collect();
    assert!(gaps.iter().all(|gap| *gap <= MAX_BACKOFF));
    assert_eq!(gaps.last(), Some(&MAX_BACKOFF));
    let before = bodies.lock().unwrap().len();
    let started = tokio::time::Instant::now();
    assert!(!notifier.flush(RESTART_FLUSH).await);
    assert_eq!(
        started.elapsed(),
        RESTART_FLUSH,
        "flush gives up at its bound"
    );
    assert_eq!(
        bodies.lock().unwrap().len(),
        before + 1,
        "a flush cuts one backoff short, never loops on a failing gateway"
    );
}

#[tokio::test]
async fn http_delivery_sends_the_contract_headers_and_body() {
    use axum::http::HeaderMap;
    let (seen_tx, mut seen_rx) = tokio::sync::mpsc::unbounded_channel();
    let app = axum::Router::new().route(
        "/internal/host-notifications",
        axum::routing::post(move |headers: HeaderMap, body: String| {
            let seen_tx = seen_tx.clone();
            async move {
                let header = |name: &str| {
                    headers
                        .get(name)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_owned()
                };
                let _ = seen_tx.send((
                    header("authorization"),
                    header("x-tealstreet-box"),
                    header("content-type"),
                    body,
                ));
                "{\"accepted\":1,\"duplicate\":0}"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let config = Config::parse(
        Some(&format!(
            "http://127.0.0.1:{}/internal/host-notifications",
            address.port()
        )),
        Some("synthetic-notify-token"),
        Some("exchange-01"),
        None,
    )
    .unwrap()
    .unwrap();
    let notifier = HostNotifier::start(&config).unwrap();
    notifier.push(notification("a"));
    assert!(notifier.flush(Duration::from_secs(5)).await);
    let (authorization, box_id, content_type, body) = seen_rx.recv().await.unwrap();
    assert_eq!(authorization, "Bearer synthetic-notify-token");
    assert_eq!(box_id, "exchange-01");
    assert_eq!(content_type, "application/json");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["v"], 1);
    assert_eq!(ids(&body), [notification("a").id]);
}
