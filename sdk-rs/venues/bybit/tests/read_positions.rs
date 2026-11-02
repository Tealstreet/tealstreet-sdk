//! `readPositions`: success only after a read issued after the command commits.
mod support;
use safe_cex_engine::{Command, ErrorKind, Event, Notice, Outcome, VenueAction};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::{Harness, Options};

const POSITIONS: &str = "/v5/position/list";
const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;
const REFUSED: &str = r#"{"retCode":10003,"retMsg":"API key is invalid.","result":{}}"#;

fn sized(symbol: &str, size: &str) -> String {
    format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"list":[{{"symbol":"{symbol}","positionIdx":0,"side":"Buy","size":"{size}","avgPrice":"50000","markPrice":"51000","positionValue":"51000","leverage":"10","unrealisedPnl":"100","curRealisedPnl":"0","positionIM":"10"}}],"nextPageCursor":""}}}}"#
    )
}

fn options(variant: Variant) -> Options {
    Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    }
}

fn catalogue(h: &mut Harness, inverse: bool) {
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    }
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    if h.has_request("status=PreLaunch") {
        h.respond("status=PreLaunch", 200, EMPTY);
    }
    let shape = |text: &str| {
        if inverse {
            text.replace("USDT", "USD")
                .replace("LinearPerpetual", "InversePerpetual")
        } else {
            text.to_owned()
        }
    };
    h.respond(
        "/v5/market/instruments-info",
        200,
        &shape(support::INSTRUMENTS),
    );
    h.respond("/v5/market/tickers", 200, &shape(support::TICKERS));
}

fn answer_all(h: &mut Harness, path: &str, body: &str) {
    while h.has_request(path) {
        h.respond(path, 200, body);
    }
}

/// A loaded account holding one open position, nothing in flight.
fn ready(variant: Variant) -> (Harness, &'static str) {
    let symbol = if variant.is_inverse() {
        "BTCUSD"
    } else {
        "BTCUSDT"
    };
    let mut h = Harness::started(options(variant));
    catalogue(&mut h, variant.is_inverse());
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    if variant.is_hyro() {
        ack(&mut h, "auth");
        ack(&mut h, "subscribe");
    }
    hyro_or_plain_snapshot(&mut h, &sized(symbol, "1"));
    h.requests.clear();
    h.done.clear();
    (h, symbol)
}

fn hyro_or_plain_snapshot(h: &mut Harness, positions: &str) {
    answer_all(h, "/v5/order/realtime", EMPTY);
    answer_all(h, POSITIONS, positions);
    answer_all(h, "/v5/account/wallet-balance", WALLET);
}

fn ack(h: &mut Harness, op: &str) {
    #[derive(serde::Deserialize)]
    struct Sent {
        req_id: String,
    }
    let message = h
        .sent_on("/v5/private")
        .into_iter()
        .rev()
        .find(|text| text.contains(&format!("\"op\":\"{op}\"")))
        .unwrap();
    let sent: Sent = serde_json::from_str(&message).unwrap();
    h.message(
        "/v5/private",
        &format!(
            r#"{{"op":"{op}","req_id":"{}","success":true}}"#,
            sent.req_id
        ),
    );
}

fn read(h: &mut Harness, command: u32) {
    h.command(Command::Venue {
        command,
        action: VenueAction::ReadPositions,
    });
}

fn contracts(h: &Harness, symbol: &str) -> f64 {
    h.engine
        .store()
        .positions()
        .iter()
        .find(|p| p.symbol == symbol)
        .map_or(0.0, |p| p.contracts)
}

fn failure(h: &Harness, id: u32) -> (Option<ErrorKind>, Option<Outcome>) {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            Notice::CommandFailed { command, error } if *command == id => {
                Some((error.kind, error.outcome))
            }
            _ => None,
        })
        .expect("a failure notice")
}

const PLAIN: [Variant; 2] = [Variant::Linear, Variant::Inverse];

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    for variant in PLAIN {
        let (mut h, symbol) = ready(variant);
        let before = contracts(&h, symbol);
        assert!(before > 0.0);
        read(&mut h, 40);
        assert!(h.has_request(POSITIONS), "{variant:?}: a read is issued");
        assert_eq!(h.result(40), None);
        answer_all(&mut h, POSITIONS, EMPTY);
        assert_eq!(h.result(40), Some(&Ok("[]".to_owned())), "{variant:?}");
        assert_eq!(contracts(&h, symbol), 0.0, "{variant:?}: the store is flat");
    }
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    for variant in PLAIN {
        let (mut h, symbol) = ready(variant);
        h.advance(5_100.0);
        assert!(h.has_request(POSITIONS), "{variant:?}: a tick read is out");
        let stale: Vec<_> = h
            .requests
            .iter()
            .filter(|r| r.url.contains(POSITIONS))
            .map(|r| r.id)
            .collect();
        read(&mut h, 41);
        let issued = h
            .requests
            .iter()
            .filter(|r| r.url.contains(POSITIONS))
            .count();
        assert_eq!(issued, stale.len(), "{variant:?}: no second read yet");
        let body = sized(symbol, "1");
        for id in &stale {
            h.reply(*id, 200, &body);
        }
        h.requests.retain(|r| !stale.contains(&r.id));
        assert_eq!(
            h.result(41),
            None,
            "{variant:?}: the older read cannot answer"
        );
        assert!(h.has_request(POSITIONS), "{variant:?}: a new read follows");
        answer_all(&mut h, POSITIONS, EMPTY);
        assert_eq!(h.result(41), Some(&Ok("[]".to_owned())), "{variant:?}");
        assert_eq!(contracts(&h, symbol), 0.0);
    }
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    for variant in PLAIN {
        let (mut h, symbol) = ready(variant);
        let before = contracts(&h, symbol);
        read(&mut h, 42);
        h.respond(POSITIONS, 200, REFUSED);
        assert!(
            matches!(h.result(42), Some(Err(message)) if message.contains("API key is invalid")),
            "{:?}",
            h.result(42)
        );
        let (kind, outcome) = failure(&h, 42);
        assert_eq!(kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(outcome, Some(Outcome::NotSent));
        assert_eq!(contracts(&h, symbol), before);
    }
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let (mut h, _) = ready(Variant::Inverse);
    read(&mut h, 43);
    h.respond(POSITIONS, 200, r#"{"retCode":0,"retMsg":"OK","result":{}}"#);
    assert!(matches!(h.result(43), Some(Err(_))), "{:?}", h.result(43));
    assert_eq!(failure(&h, 43).1, Some(Outcome::NotSent));

    let (mut h, _) = ready(Variant::Inverse);
    read(&mut h, 44);
    h.respond(POSITIONS, 200, &sized("BTCUSD", "not-a-number"));
    assert!(
        matches!(h.result(44), Some(Err(message)) if message.contains("could not be mapped")),
        "{:?}",
        h.result(44)
    );
    assert_eq!(failure(&h, 44).1, Some(Outcome::NotSent));
}

#[test]
fn a_read_a_stream_frame_overrode_is_not_a_commit() {
    let (mut h, symbol) = ready(Variant::Inverse);
    let before = contracts(&h, symbol);
    read(&mut h, 45);
    // The frame lands while the read is out: the read is discarded unread.
    let frame = format!(
        r#"{{"topic":"position.inverse","data":[{{"category":"inverse","symbol":"{symbol}","positionIdx":0,"side":"Buy","size":"7","entryPrice":"50000","markPrice":"51000","positionValue":"51000","leverage":"10","unrealisedPnl":"1","positionIM":"10"}}]}}"#
    );
    h.message("/v5/private", &frame);
    let framed = contracts(&h, symbol);
    assert!(framed > 0.0 && framed != before);
    h.respond(POSITIONS, 200, EMPTY);
    assert_eq!(h.result(45), None, "a discarded read answers nothing");
    assert_eq!(contracts(&h, symbol), framed);
    // Next read: still inside the frame's guard, so it commits around the row.
    h.advance(500.0);
    h.respond(POSITIONS, 200, EMPTY);
    assert_eq!(h.result(45), None, "a guarded read answers nothing");
    assert_eq!(contracts(&h, symbol), framed);
    // Past the guard the read commits and answers.
    h.advance(600.0);
    h.respond(POSITIONS, 200, EMPTY);
    assert_eq!(h.result(45), Some(&Ok("[]".to_owned())));
    assert_eq!(contracts(&h, symbol), 0.0);
}

#[test]
fn a_private_socket_close_drops_the_read_and_issues_another() {
    let (mut h, symbol) = ready(Variant::Inverse);
    read(&mut h, 46);
    let dropped = h.take(POSITIONS);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.reply(dropped.id, 200, EMPTY);
    assert_eq!(h.result(46), None);
    assert!(h.has_request(POSITIONS), "{:?}", h.paths());
    h.respond(POSITIONS, 200, EMPTY);
    assert_eq!(h.result(46), Some(&Ok("[]".to_owned())));
    assert_eq!(contracts(&h, symbol), 0.0);
}

#[test]
fn every_enabled_product_must_commit() {
    let mut h = Harness::started(Options {
        enable_inverse_markets: true,
        ..options(Variant::Linear)
    });
    for _ in 0..2 {
        if h.has_request("/v5/account/fee-rate") {
            h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
        }
        if h.has_request("/v5/account/info") {
            h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
        }
    }
    answer_all(&mut h, "status=PreLaunch", EMPTY);
    answer_all(&mut h, "/v5/market/instruments-info", support::INSTRUMENTS);
    answer_all(&mut h, "/v5/market/tickers", support::TICKERS);
    answer_all(&mut h, "/v5/account/fee-rate", support::FEE_RATES);
    hyro_or_plain_snapshot(&mut h, EMPTY);
    h.requests.clear();
    read(&mut h, 47);
    let inverse = "category=inverse";
    assert!(h.has_request(inverse), "{:?}", h.paths());
    answer_all(&mut h, "category=linear", EMPTY);
    assert_eq!(h.result(47), None, "inverse is still out");
    answer_all(&mut h, inverse, EMPTY);
    assert_eq!(h.result(47), Some(&Ok("[]".to_owned())));
}

#[test]
fn hyro_answers_only_from_a_committed_snapshot_issued_after_the_command() {
    let (mut h, symbol) = ready(Variant::Hyrotrader);
    read(&mut h, 50);
    assert!(h.has_request(POSITIONS));
    answer_all(&mut h, "/v5/order/realtime", EMPTY);
    answer_all(&mut h, POSITIONS, EMPTY);
    assert_eq!(
        h.result(50),
        None,
        "the wallet leg is out: nothing committed"
    );
    answer_all(&mut h, "/v5/account/wallet-balance", WALLET);
    assert_eq!(h.result(50), Some(&Ok("[]".to_owned())));
    assert_eq!(contracts(&h, symbol), 0.0);
}

#[test]
fn hyro_abandons_a_snapshot_in_flight_for_a_new_one() {
    let (mut h, symbol) = ready(Variant::Hyrotrader);
    h.advance(5_100.0);
    assert!(h.has_request(POSITIONS), "a tick snapshot is out");
    let stale: Vec<_> = h.requests.drain(..).collect();
    read(&mut h, 51);
    assert!(h.has_request(POSITIONS), "a new snapshot is issued");
    let body = sized(symbol, "1");
    for request in &stale {
        let reply = if request.url.contains(POSITIONS) {
            body.as_str()
        } else if request.url.contains("wallet") {
            WALLET
        } else {
            EMPTY
        };
        h.reply(request.id, 200, reply);
    }
    assert_eq!(h.result(51), None, "the old snapshot cannot answer");
    hyro_or_plain_snapshot(&mut h, EMPTY);
    assert_eq!(h.result(51), Some(&Ok("[]".to_owned())));
    assert_eq!(contracts(&h, symbol), 0.0);
}

#[test]
fn hyro_any_failed_leg_fails_the_command() {
    let (mut h, symbol) = ready(Variant::Hyrotrader);
    read(&mut h, 52);
    h.respond(POSITIONS, 200, REFUSED);
    assert!(
        matches!(h.result(52), Some(Err(message)) if message.contains("API key is invalid")),
        "{:?}",
        h.result(52)
    );
    assert_eq!(failure(&h, 52).1, Some(Outcome::NotSent));
    assert_eq!(contracts(&h, symbol), 1.0);

    let (mut h, _) = ready(Variant::Hyrotrader);
    read(&mut h, 53);
    h.respond("/v5/account/wallet-balance", 500, "bad gateway");
    assert!(matches!(h.result(53), Some(Err(_))), "{:?}", h.result(53));
    assert_eq!(failure(&h, 53).1, Some(Outcome::NotSent));
}

#[test]
fn spot_and_a_public_engine_send_nothing() {
    let mut h = Harness::started(options(Variant::Spot));
    read(&mut h, 60);
    assert_eq!(
        failure(&h, 60),
        (Some(ErrorKind::Unsupported), Some(Outcome::NotSent))
    );
    assert!(!h.has_request(POSITIONS));

    let mut h = Harness::new();
    read(&mut h, 61);
    let (kind, outcome) = failure(&h, 61);
    assert_ne!(
        kind,
        Some(ErrorKind::Unsupported),
        "advertised, so a precondition"
    );
    assert_eq!(outcome, Some(Outcome::NotSent));
    assert!(!h.has_request(POSITIONS));
}
