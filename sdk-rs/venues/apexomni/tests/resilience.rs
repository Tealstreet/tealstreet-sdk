mod support;
use safe_cex_engine::model::{OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, SocketChannel, SocketId};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::*;

const ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1"}],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const PARENT: &str = r#"{"id":"parent","symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY","size":"2","price":"100","openSlParam":{"triggerPrice":"90","triggerPriceType":"INDEX"}}"#;
const STOP: &str = r#"{"id":"stop","symbol":"BTC-USDT","status":"UNTRIGGERED","type":"STOP_MARKET","side":"SELL","size":"1","price":"80","triggerPrice":"85","triggerPriceType":"MARKET"}"#;
const FRESH: &str = r#"{"id":"fresh","symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY","size":"1","price":"95"}"#;
const ACK: &str = r#"{"code":0,"data":{}}"#;

fn list(rows: &[&str]) -> String {
    format!(r#"{{"code":0,"data":[{}]}}"#, rows.join(","))
}
fn engine(seed: &str) -> TestEngine {
    use safe_cex_crypto::signer::{
        LocalExtra, LocalExtras, LocalIdentity, LocalSetup, LocalSigner, Secret,
    };
    use safe_cex_engine::signing::{
        CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy, IdentitySetupPolicy,
        KeyEncoding,
    };
    let mut extra = LocalExtras::default();
    if safe_cex_venue_apexomni::sign::validate_seed(seed).is_ok() {
        extra.0.insert(
            ExtraKey::ZklinkSeed,
            LocalExtra {
                value: Secret::new(seed.into()),
                encoding: KeyEncoding::Hex,
            },
        );
    }
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("key".into()),
                secret: Secret::new("secret".into()),
                passphrase: Some(Secret::new("passphrase".into())),
                extra,
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base64Text),
                extra: Some(ExtraKeyPolicy {
                    key: ExtraKey::ZklinkSeed,
                    field: ExtraFieldPolicy::Optional,
                    encoding: Some(KeyEncoding::Hex),
                }),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..Default::default()
    }));
    engine.set_identities(identities).unwrap();
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    engine
}
fn seeded(e: &mut TestEngine, markets: &str) -> (Option<SocketId>, Vec<HttpRequest>, Vec<Action>) {
    e.start(0.0);
    let r = requests(e).remove(0);
    response(e, &r, 10.0, markets);
    let actions: Vec<_> = e.prepared(0.0).collect();
    let socket = actions.iter().find_map(|a| match a {
        Action::WsConnect {
            socket,
            channel: Some(SocketChannel::Private),
            ..
        } => Some(*socket),
        _ => None,
    });
    let http = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    (socket, http, actions)
}
fn answer(e: &mut TestEngine, http: Vec<HttpRequest>, at: f64, account: &str, orders: &str) {
    for r in http {
        let body = if r.url.ends_with("/v3/account") {
            account
        } else {
            orders
        };
        response(e, &r, at, body);
    }
}
fn ready(orders: &str) -> (TestEngine, SocketId) {
    let mut e = engine(&"11".repeat(65));
    let (socket, http, _) = seeded(&mut e, MARKETS);
    let socket = socket.unwrap();
    e.handle(20.0, Event::WsOpened { socket });
    e.prepared(20.0).for_each(drop);
    answer(&mut e, http, 30.0, ACCOUNT, orders);
    e.prepared(20.0).for_each(drop);
    (e, socket)
}
fn frame(body: &str) -> String {
    format!(r#"{{"topic":"ws_zk_accounts_v3","type":"delta","contents":{body}}}"#)
}
fn heal_orders(e: &mut TestEngine, socket: SocketId, at: f64, body: &str) {
    ws(e, socket, at, &frame(r#"{"orders":[]}"#));
    let timer = e.prepared(at)
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == at + 1000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(at + 1000.0, Event::TimerFired { timer });
    let request = requests(e)
        .into_iter()
        .find(|r| r.url.ends_with("/v3/open-orders"))
        .unwrap();
    response(e, &request, at + 1010.0, body);
}
fn command(e: &mut TestEngine, at: f64, command: Command) {
    e.handle(at, Event::Command(command));
    let write = prepared_requests(e, at).remove(0);
    response(e, &write, at + 10.0, ACK);
}
fn order_ids(e: &TestEngine) -> Vec<&str> {
    let mut ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
    ids.sort_unstable();
    ids
}
fn warned(actions: &[Action], text: &str) -> bool {
    actions.iter().any(|a| {
        matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. })) if message.contains(text))
    })
}
fn fill_amounts(e: &mut TestEngine) -> Vec<f64> {
    let Some(frame) = e.take_frame(100.0) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let rows = (4 + 12 * count + 7) & !7;
    (0..count)
        .map(|index| {
            let offset = rows + index * 16 + 8;
            f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap())
        })
        .collect()
}

#[test]
fn accepted_cancel_fences_stale_reads_but_an_order_the_venue_kept_reappears() {
    let (mut e, socket) = ready(&list(&[PARENT]));
    assert_eq!(order_ids(&e), ["parent", "parent__stop_loss"]);
    // A read already in flight when the cancel lands cannot reopen it.
    ws(&mut e, socket, 100.0, &frame(r#"{"orders":[]}"#));
    let timer = e.prepared(100.0)
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = requests(&mut e, 1100.0).remove(0);
    command(
        &mut e,
        1200.0,
        Command::CancelOrders {
            command: 1,
            ids: vec!["parent".into()],
        },
    );
    assert!(order_ids(&e).is_empty());
    response(&mut e, &stale, 1300.0, &list(&[PARENT]));
    assert!(order_ids(&e).is_empty());
    // An echo before the venue settles the cancel stays hidden.
    heal_orders(&mut e, socket, 2000.0, &list(&[PARENT]));
    assert!(order_ids(&e).is_empty());
    // Well after the accepted cancel the venue's open list is authoritative again.
    heal_orders(&mut e, socket, 8000.0, &list(&[PARENT]));
    assert_eq!(order_ids(&e), ["parent", "parent__stop_loss"]);
    let actions: Vec<_> = e.prepared(1300.0).collect();
    assert!(warned(
        &actions,
        "order parent is still open after its cancel"
    ));
}

#[test]
fn stale_read_landing_after_the_grace_window_cannot_reopen_a_cancelled_order() {
    let (mut e, socket) = ready(&list(&[PARENT]));
    ws(&mut e, socket, 100.0, &frame(r#"{"orders":[]}"#));
    let timer = e.prepared(100.0)
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = requests(&mut e, 1100.0).remove(0);
    command(
        &mut e,
        1200.0,
        Command::CancelOrders {
            command: 5,
            ids: vec!["parent".into()],
        },
    );
    response(&mut e, &stale, 7400.0, &list(&[PARENT]));
    assert!(order_ids(&e).is_empty());
    let actions: Vec<_> = e.prepared(7400.0).collect();
    assert!(!warned(&actions, "still open after its cancel"));
    // The stale read does not consume the check; a fresh read still reports it.
    heal_orders(&mut e, socket, 8000.0, &list(&[PARENT]));
    assert_eq!(order_ids(&e), ["parent", "parent__stop_loss"]);
    let actions: Vec<_> = e.prepared(7400.0).collect();
    assert!(warned(
        &actions,
        "order parent is still open after its cancel"
    ));
}

#[test]
fn cancel_all_does_not_tombstone_an_untriggered_stop_the_venue_kept() {
    let (mut e, socket) = ready(&list(&[PARENT, STOP]));
    assert_eq!(order_ids(&e), ["parent", "parent__stop_loss", "stop"]);
    command(
        &mut e,
        100.0,
        Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(order_ids(&e).is_empty());
    heal_orders(&mut e, socket, 8000.0, &list(&[STOP]));
    assert_eq!(order_ids(&e), ["stop"]);
    assert_eq!(e.store().order("stop").unwrap().status, OrderStatus::Open);
}

#[test]
fn venue_confirmed_cancel_stays_hidden_after_the_grace_window() {
    let (mut e, socket) = ready(&list(&[PARENT]));
    command(
        &mut e,
        100.0,
        Command::CancelOrders {
            command: 3,
            ids: vec!["parent".into()],
        },
    );
    let canceled = PARENT.replace(r#""status":"OPEN""#, r#""status":"CANCELED""#);
    ws(
        &mut e,
        socket,
        200.0,
        &frame(&format!(r#"{{"orders":[{canceled}]}}"#)),
    );
    heal_orders(&mut e, socket, 8000.0, &list(&[PARENT]));
    assert!(order_ids(&e).is_empty());
}

#[test]
fn malformed_startup_rows_still_initialise_the_account_and_signing_state() {
    let mut e = engine(&"11".repeat(65));
    let (_, http, _) = seeded(&mut e, MARKETS);
    let account = ACCOUNT.replace(
        r#""positions":["#,
        r#""positions":[{"symbol":"BTC-USDT","side":"SHORT","size":"not-a-number"},"#,
    );
    let broken = r#"{"id":"broken","symbol":"BTC-USDT","status":"MYSTERY","type":"LIMIT","side":"BUY","size":"1","price":"1"}"#;
    answer(&mut e, http, 30.0, &account, &list(&[broken, PARENT]));
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(e.store().loaded().positions && e.store().loaded().orders);
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().contracts, 2.0);
    assert_eq!(order_ids(&e), ["parent", "parent__stop_loss"]);
    assert!(warned(&actions, "malformed order row broken;"));
    assert!(warned(&actions, "malformed position row BTCUSDT;"));
    // Committing the snapshot initialises signing state, which requests balance.
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/account-balance")))
    );
}

#[test]
fn malformed_heal_row_keeps_its_known_order_while_the_rest_commits() {
    let unidentified = r#"{"symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY"}"#;
    let broken_parent = PARENT.replace(r#""side":"BUY""#, r#""side":"SIDEWAYS""#);
    // A wrongly typed symbol must not cost the row its valid ID.
    let odd_symbol = PARENT.replace(r#""symbol":"BTC-USDT""#, r#""symbol":{"name":7}"#);
    for (malformed, expected) in [
        (
            broken_parent.as_str(),
            vec!["fresh", "parent", "parent__stop_loss"],
        ),
        (
            odd_symbol.as_str(),
            vec!["fresh", "parent", "parent__stop_loss"],
        ),
        (
            unidentified,
            vec!["fresh", "parent", "parent__stop_loss", "stop"],
        ),
    ] {
        let (mut e, socket) = ready(&list(&[PARENT, STOP]));
        heal_orders(&mut e, socket, 100.0, &list(&[malformed, FRESH]));
        assert_eq!(order_ids(&e), expected, "{malformed}");
    }
}

#[test]
fn null_fields_and_one_bad_row_never_drop_sibling_fills() {
    let (mut e, socket) = ready(&list(&[]));
    e.take_frame(50.0);
    let nulls = r#"{"id":"f1","matchFillId":null,"orderId":"o1","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","fee":null,"token":null,"feeCurrency":null,"createdAt":1700000000123}"#;
    let order = r#"{"id":"o1","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"OPEN","size":"3","price":"100","clientOrderId":null,"clientId":null,"cancelReason":null,"timeInForce":null,"reduceOnly":null,"postOnly":null,"triggerPriceType":null,"openSlParam":null}"#;
    let bad = r#"{"id":"f2","orderId":"o1","symbol":"BTC-USDT","side":"BUY","size":"1","price":"oops","createdAt":1700000000124}"#;
    let good = r#"{"id":"f3","orderId":"o1","symbol":"BTC-USDT","side":"BUY","size":"0.5","price":"102","createdAt":1700000000125}"#;
    ws(
        &mut e,
        socket,
        100.0,
        &frame(&format!(
            r#"{{"fills":[{nulls},{bad},{good}],"orders":[{order}],"positions":null}}"#
        )),
    );
    let actions: Vec<_> = e.prepared(100.0).collect();
    assert!(warned(&actions, "skipped malformed fill rows (f2)"));
    assert_eq!(fill_amounts(&mut e), [1.0, 0.5]);
    assert_eq!(e.store().order("o1").unwrap().amount, 3.0);
}

#[test]
fn one_malformed_market_row_does_not_block_markets_or_private_start() {
    let odd = r#"{"symbol":"ETH-USDT","crossSymbolName":"ETHUSDT","baseTokenId":"ETH","settleAssetId":"USDT","enableTrade":true,"stepSize":null,"tickSize":"0.01","minOrderSize":"0.01","maxOrderSize":"100","displayMinLeverage":"1","displayMaxLeverage":"50","l2PairId":"50001"}"#;
    let markets = MARKETS.replace(
        r#""perpetualContract":["#,
        &format!(r#""perpetualContract":[{odd},"#),
    );
    let mut e = engine(&"11".repeat(65));
    let (socket, http, actions) = seeded(&mut e, &markets);
    assert_eq!(e.store().markets().len(), 1);
    assert!(socket.is_some());
    assert!(http.iter().any(|r| r.url.ends_with("/v3/open-orders")));
    assert!(warned(&actions, "malformed market rows (ETH-USDT)"));
}

#[test]
fn account_rows_on_a_skipped_market_are_reported_once_as_hidden() {
    let odd =
        r#"{"symbol":"ETH-USDT","crossSymbolName":"ETHUSDT","enableTrade":true,"stepSize":null}"#;
    let markets = MARKETS.replace(
        r#""perpetualContract":["#,
        &format!(r#""perpetualContract":[{odd},"#),
    );
    let mut e = engine(&"11".repeat(65));
    let (socket, http, _) = seeded(&mut e, &markets);
    let socket = socket.unwrap();
    e.handle(20.0, Event::WsOpened { socket });
    let account = ACCOUNT.replace(
        r#""positions":["#,
        r#""positions":[{"symbol":"ETH-USDT","side":"LONG","entryPrice":"3000","size":"1"},"#,
    );
    answer(&mut e, http, 30.0, &account, &list(&[]));
    let eth =
        r#"{"positions":[{"symbol":"ETH-USDT","side":"LONG","entryPrice":"3000","size":"2"}]}"#;
    ws(&mut e, socket, 100.0, &frame(eth));
    let hidden = e.prepared(100.0)
        .filter(|a| {
            matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
                if message.contains("position on ETH-USDT is hidden"))
        })
        .count();
    assert_eq!(hidden, 1);
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().contracts, 2.0);
}

#[test]
fn fill_without_order_id_is_still_emitted() {
    let (mut e, socket) = ready(&list(&[]));
    e.take_frame(50.0);
    let orphan = r#"{"id":"f9","orderId":null,"symbol":"BTC-USDT","side":"SELL","size":"0.25","price":"101","createdAt":1700000000200}"#;
    ws(
        &mut e,
        socket,
        100.0,
        &frame(&format!(r#"{{"fills":[{orphan}]}}"#)),
    );
    assert_eq!(fill_amounts(&mut e), [0.25]);
}

#[test]
fn invalid_omni_seed_keeps_read_only_monitoring_and_rejects_placement() {
    let mut e = engine("bad");
    let (socket, http, actions) = seeded(&mut e, MARKETS);
    assert_eq!(e.store().markets().len(), 1);
    assert!(socket.is_some());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
            if message.contains("Invalid OmniKey seed") && message.contains("placement is disabled")
    )));
    answer(&mut e, http, 30.0, ACCOUNT, &list(&[PARENT]));
    assert!(e.store().loaded().positions && e.store().loaded().orders);
    assert_eq!(order_ids(&e), ["parent", "parent__stop_loss"]);
    e.drain_actions().for_each(drop);
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":100}"#,
    )
    .unwrap();
    e.handle(
        40.0,
        Event::Command(Command::PlaceOrders {
            command: 4,
            orders: vec![order],
        }),
    );
    let actions: Vec<_> = e.prepared(40.0).collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 4, error })
            if error.kind == Some(safe_cex_engine::ErrorKind::InvalidInput) && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
    )));
}

#[test]
fn one_malformed_order_keeps_its_stored_copy_while_valid_peers_commit() {
    let (mut e, socket) = ready(&list(&[PARENT, FRESH]));
    assert_eq!(order_ids(&e), ["fresh", "parent", "parent__stop_loss"]);
    let changed = FRESH.replace(r#""price":"95""#, r#""price":"96""#);
    let broken = PARENT.replace(r#""side":"BUY""#, r#""side":"SIDEWAYS""#);
    heal_orders(&mut e, socket, 100.0, &list(&[&changed, &broken]));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(e.store().order("fresh").unwrap().price, 96.0);
    assert_eq!(order_ids(&e), ["fresh", "parent", "parent__stop_loss"]);
    assert!(warned(
        &actions,
        "ApeX skipped malformed order row parent; keeping its last known state"
    ));
    assert!(!warned(&actions, "SIDEWAYS"));
    heal_orders(&mut e, socket, 3000.0, &list(&[&changed]));
    assert_eq!(order_ids(&e), ["fresh"]);
}
