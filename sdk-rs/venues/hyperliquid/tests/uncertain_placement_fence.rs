mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Response, Venue,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const SPOT_META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"HFUN","index":2,"szDecimals":2}],"universe":[{"name":"@1","tokens":[2,0],"index":1}]},[{"coin":"@1","midPx":"1","markPx":"1","prevDayPx":"1","dayNtlVlm":"100"}]]"#;

fn response(engine: &mut Engine<Hyperliquid>, now: f64, request: &HttpRequest, body: &str) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn boot(spot: bool) -> (Engine<Hyperliquid>, HttpRequest) {
    let options = Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    };
    let venue = if spot {
        Hyperliquid::new_spot(options)
    } else {
        Hyperliquid::new(options)
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.start(1_000.0);
    let mut old_read = None;
    for _ in 0..20 {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::WsConnect { socket, .. } => {
                    engine.handle(1_000.0, Event::WsOpened { socket });
                }
                Action::Http(request) => {
                    let kind = support::kind(&request);
                    let body = match kind.as_str() {
                        "metaAndAssetCtxs" => support::META,
                        "spotMetaAndAssetCtxs" => SPOT_META,
                        "perpDexs" => "[null]",
                        "userAbstraction" => r#""disabled""#,
                        "maxBuilderFee" => "30",
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                        "clearinghouseState" => support::STATE,
                        "spotClearinghouseState" => {
                            r#"{"balances":[{"coin":"USDC","total":"100","hold":"0"}]}"#
                        }
                        "frontendOpenOrders" => {
                            assert!(old_read.replace(request).is_none(), "single-flight orders");
                            continue;
                        }
                        other => panic!("unexpected bootstrap request {other}"),
                    };
                    response(&mut engine, 1_000.0, &request, body);
                }
                _ => {}
            }
        }
    }
    assert!(!engine.store().markets().is_empty());
    (engine, old_read.expect("real in-flight open-orders read"))
}

fn place(engine: &mut Engine<Hyperliquid>, now: f64, command: u32, spot: bool) -> Vec<Action> {
    let symbol = if spot { "HFUN/USDC" } else { "BTC-PERP" };
    let order: PlaceOrderOpts = serde_json::from_str(&format!(
        r#"{{"symbol":"{symbol}","type":"limit","side":"buy","price":1,"amount":1}}"#
    ))
    .unwrap();
    engine.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![order],
        }),
    );
    engine.drain_actions().collect()
}

fn write(actions: &[Action]) -> &HttpRequest {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/exchange") => Some(request),
            _ => None,
        })
        .expect("native signed placement")
}

fn refused(actions: &[Action], command: u32) {
    assert!(
        actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: id, error })
            if *id == command && error.outcome == Some(Outcome::NotSent)
                && error.message.contains("Placement outcome is uncertain")
        )),
        "repeat placement must be refused until a fresh complete orders read: {actions:?}"
    );
    assert!(!actions.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.ends_with("/exchange"))
    ));
}

#[derive(Clone, Copy)]
enum ReconcileReply {
    Clean,
    Duplicate,
    Malformed,
    Lost,
}

fn unknown_then_stale_then_fresh(spot: bool, reply: ReconcileReply) {
    let (mut engine, old_read) = boot(spot);
    let first = place(&mut engine, 2_000.0, 101, spot);
    let first = write(&first);
    engine.handle(
        2_400.0,
        Event::HttpFailed {
            request: first.id,
            error: "placement reply lost after dispatch",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 101, error })
        if error.outcome == Some(Outcome::Unknown)
    )));
    assert!(!actions.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.ends_with("/exchange"))
    ));

    refused(&place(&mut engine, 2_800.0, 102, spot), 102);
    let symbol = if spot { "HFUN/USDC" } else { "BTC-PERP" };
    engine.handle(
        2_850.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 106,
            symbol: symbol.into(),
        }),
    );
    refused(&engine.drain_actions().collect::<Vec<_>>(), 106);
    response(&mut engine, 2_900.0, &old_read, "[]");
    let actions: Vec<_> = engine.drain_actions().collect();
    let fresh = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if support::kind(&request) == "frontendOpenOrders" => {
                Some(request)
            }
            _ => None,
        })
        .expect("one owed fresh read after the pre-write snapshot completes");
    assert_ne!(old_read.id, fresh.id);
    let retry = place(&mut engine, 3_800.0, 103, spot);
    refused(&retry, 103);
    assert!(!retry.iter().any(|action|
        matches!(action, Action::Http(request) if support::kind(request) == "frontendOpenOrders")
    ), "a backoff re-ask must join the running fresh read");

    let coin = if spot { "@1" } else { "BTC" };
    let row = format!(
        r#"{{"coin":"{coin}","oid":7,"side":"B","limitPx":"1","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}}"#
    );
    let (fresh, clean_at) = if !matches!(reply, ReconcileReply::Clean) {
        match reply {
            ReconcileReply::Duplicate => {
                response(&mut engine, 4_000.0, &fresh, &format!("[{row},{row}]"));
            }
            ReconcileReply::Malformed => response(&mut engine, 4_000.0, &fresh, r#"[{"oid":7}]"#),
            ReconcileReply::Lost => engine.handle(
                4_000.0,
                Event::HttpFailed {
                    request: fresh.id,
                    error: "reconcile response lost",
                },
            ),
            ReconcileReply::Clean => unreachable!(),
        }
        let mut actions: Vec<_> = engine.drain_actions().collect();
        let retry = place(&mut engine, 6_500.0, 105, spot);
        refused(&retry, 105);
        actions.extend(retry);
        let next = actions
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if support::kind(&request) == "frontendOpenOrders" => {
                    Some(request)
                }
                _ => None,
            })
            .expect("engine re-asks after an unproven reconciliation");
        assert_ne!(fresh.id, next.id);
        (next, 6_600.0)
    } else {
        (fresh, 4_000.0)
    };
    response(&mut engine, clean_at, &fresh, &format!("[{row}]"));
    assert!(
        engine.store().order("7").is_some(),
        "publish before releasing"
    );
    assert!(!engine.drain_actions().any(|action|
        matches!(action, Action::Http(request) if support::kind(&request) == "frontendOpenOrders")
    ), "the successful fresh read must not inherit an extra owed read from a re-ask");
    engine.handle(
        clean_at + 400.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 107,
            symbol: symbol.into(),
        }),
    );
    let cancel: Vec<_> = engine.drain_actions().collect();
    let cancel = write(&cancel);
    let payload: serde_json::Value = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["action"]["type"], "cancel");
    response(
        &mut engine,
        clean_at + 401.0,
        cancel,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    engine.drain_actions().for_each(drop);
    let retry = place(&mut engine, clean_at + 800.0, 104, spot);
    write(&retry);
    assert!(!retry.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { command: 104, .. })
    )));
}

#[test]
fn repeated_reconcile_hooks_join_one_real_open_orders_read() {
    for spot in [false, true] {
        let options = Options {
            application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
            ..Options::default()
        };
        let venue = if spot {
            Hyperliquid::new_spot(options)
        } else {
            Hyperliquid::new(options)
        };
        let venue = safe_cex_engine::test_support::ProbeVenue::new(venue, |venue, cx| {
            for _ in 0..3 {
                venue.reconcile_uncertain(cx, None);
            }
        });
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine.start(1_000.0);
        let requests: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(support::kind(&requests[0]), "frontendOpenOrders");
        assert!(requests[0].url.ends_with("/info"));
    }
}

#[test]
fn perps_unknown_placement_is_fenced_until_a_fresh_complete_read() {
    unknown_then_stale_then_fresh(false, ReconcileReply::Clean);
}

#[test]
fn spot_unknown_placement_is_fenced_until_a_fresh_complete_read() {
    unknown_then_stale_then_fresh(true, ReconcileReply::Clean);
}

#[test]
fn duplicate_rows_cannot_release_perps_or_spot_uncertainty() {
    for spot in [false, true] {
        unknown_then_stale_then_fresh(spot, ReconcileReply::Duplicate);
    }
}

#[test]
fn malformed_rows_cannot_release_perps_or_spot_uncertainty() {
    for spot in [false, true] {
        unknown_then_stale_then_fresh(spot, ReconcileReply::Malformed);
    }
}

#[test]
fn failed_reconciliation_is_reasked_without_replaying_the_write() {
    for spot in [false, true] {
        unknown_then_stale_then_fresh(spot, ReconcileReply::Lost);
    }
}
