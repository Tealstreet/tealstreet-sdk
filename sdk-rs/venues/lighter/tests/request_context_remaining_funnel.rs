//! Remaining private reads and writes retain genuine admission and DATA custody.
mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, TimerId, VenueAction,
};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::{ACCOUNT, META, NOW, options, ready_with, reply_at};

const COMMAND: u32 = 901;

fn ready(testnet: bool) -> Engine<Lighter> {
    let (mut engine, _) = ready_with(safe_cex_venue_lighter::Options {
        testnet,
        ..options()
    });
    engine.drain_actions().count();
    engine
}

fn issue(engine: &mut Engine<Lighter>, command: Command, wait: u32) -> Vec<Action> {
    engine.handle_command(NOW, command, Some(wait)).unwrap();
    engine.drain_actions().collect()
}

fn place(engine: &mut Engine<Lighter>) -> Vec<Action> {
    let order: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"ETH", "type":"limit", "side":"buy", "amount":0.2, "price":90
    }))
    .unwrap();
    issue(
        engine,
        Command::PlaceOrders {
            command: COMMAND,
            orders: vec![order],
        },
        30_000,
    )
}

fn fills() -> Command {
    Command::Query {
        command: COMMAND,
        query: Query::Fills {
            symbol: Some("ETH".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    }
}

fn candidate(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(packet) if packet.url.contains(path) => Some(packet.clone()),
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { request, .. } if request.url.contains(path) => {
                    Some(request.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}: {actions:?}"))
}

fn prepared(actions: &[Action], path: &str) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(packet) if packet.url.contains(path))),
        "{path}: physical HTTP must await genuine route preparation"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, Action::ManagedHttp(action)
                if matches!(action.as_ref(), HttpAction::Prepare { request, .. }
                    if request.url.contains(path))))
            .count(),
        1,
        "exactly one preparation for {path}"
    );
}

fn release(engine: &mut Engine<Lighter>, actions: &[Action], path: &str) -> HttpRequest {
    release_at(engine, actions, path, NOW)
}

fn release_at(
    engine: &mut Engine<Lighter>,
    actions: &[Action],
    path: &str,
    now: f64,
) -> HttpRequest {
    if let Some(packet) = actions.iter().find_map(|action| match action {
        Action::Http(packet) if packet.url.contains(path) => Some(packet.clone()),
        _ => None,
    }) {
        // Baseline controls also execute before the legacy path is replaced.
        return packet;
    }
    let preparation = actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains(path) => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("genuine preparation");
    engine.http_prepared(
        now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, packet) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } if request.url.contains(path) => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("matching bind");
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    packet
}

fn after_reply(engine: &mut Engine<Lighter>, packet: &HttpRequest, body: &str) -> Vec<Action> {
    reply_at(engine, NOW, packet, 200, body);
    engine.drain_actions().collect()
}

fn lookup() -> (Engine<Lighter>, Vec<Action>) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(options()));
    engine.start(NOW);
    let catalog = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(packet) if packet.url.contains("orderBookDetails") => Some(packet),
            _ => None,
        })
        .expect("released catalogue");
    let actions = after_reply(&mut engine, &catalog, META);
    (engine, actions)
}

fn nonce_then_submit(engine: &mut Engine<Lighter>) -> Vec<Action> {
    let actions = place(engine);
    let nonce = release(engine, &actions, "nextNonce");
    after_reply(engine, &nonce, r#"{"code":200,"nonce":10}"#)
}

fn terminal(actions: &[Action]) -> Vec<&Notice> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(notice @ Notice::CommandDone { command, .. })
            | Action::Notice(notice @ Notice::CommandFailed { command, .. })
                if *command == COMMAND =>
            {
                Some(notice)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn startup_lookup_waits_for_preparation() {
    let (_, actions) = lookup();
    prepared(&actions, "/account?by=index");
}

#[test]
fn account_snapshot_waits_for_preparation() {
    let (mut engine, actions) = lookup();
    let packet = release(&mut engine, &actions, "/account?by=index");
    let actions = after_reply(&mut engine, &packet, ACCOUNT);
    prepared(&actions, "/account?by=index");
}

#[test]
fn open_orders_wait_for_preparation() {
    let (mut engine, actions) = lookup();
    let packet = release(&mut engine, &actions, "/account?by=index");
    let actions = after_reply(&mut engine, &packet, ACCOUNT);
    prepared(&actions, "accountActiveOrders");
}

#[test]
fn private_history_waits_for_preparation() {
    let mut engine = ready(true);
    let actions = issue(&mut engine, fills(), 30_000);
    prepared(&actions, "/trades?");
}

#[test]
fn limits_wait_for_preparation_before_partner_attributed_order() {
    let mut engine = ready(false);
    let actions = place(&mut engine);
    prepared(&actions, "accountLimits");
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(packet)
        if packet.url.contains("sendTx")))
    );
}

#[test]
fn approval_read_waits_for_preparation() {
    let mut engine = ready(false);
    let actions = place(&mut engine);
    let limits = release(&mut engine, &actions, "accountLimits");
    let actions = after_reply(
        &mut engine,
        &limits,
        r#"{"code":200,"user_tier_name":"premium"}"#,
    );
    prepared(&actions, "/account?by=index");
}

#[test]
fn nonce_read_waits_for_preparation() {
    let mut engine = ready(true);
    let actions = place(&mut engine);
    prepared(&actions, "nextNonce");
}

#[test]
fn submission_waits_for_preparation() {
    let mut engine = ready(true);
    let actions = nonce_then_submit(&mut engine);
    prepared(&actions, "sendTx");
}

#[test]
fn account_tier_mutation_waits_for_preparation() {
    let mut engine = ready(true);
    let actions = issue(
        &mut engine,
        Command::Venue {
            command: COMMAND,
            action: VenueAction::SetAccountTier {
                tier: "premium".into(),
            },
        },
        30_000,
    );
    prepared(&actions, "changeAccountTier");
}

#[test]
fn unclaimed_history_cannot_finish_or_reissue() {
    let mut engine = ready(true);
    let actions = issue(&mut engine, fills(), 30_000);
    let packet = candidate(&actions, "/trades?");
    let actions = after_reply(&mut engine, &packet, r#"{"code":200,"trades":[]}"#);
    assert!(
        terminal(&actions).is_empty(),
        "unclaimed DATA has no terminal authority"
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_))),
        "unclaimed DATA cannot start another page"
    );
}

#[test]
fn history_root_expires_at_the_original_caller_wait() {
    let mut engine = ready(true);
    issue(&mut engine, fills(), 20);
    engine.prepare_dispatch(NOW + 20.0);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let terminals = terminal(&actions);
    assert_eq!(
        terminals.len(),
        1,
        "original receipt expires once at equality"
    );
    assert!(matches!(terminals[0], Notice::CommandFailed { error, .. }
        if error.kind == Some(safe_cex_engine::ErrorKind::Timeout)));
}

#[test]
fn unsigned_public_history_and_native_empty_result_are_preserved() {
    let mut engine = ready(true);
    let actions = issue(
        &mut engine,
        Command::Query {
            command: COMMAND,
            query: Query::PublicTrades {
                symbol: "ETH".into(),
                limit: Some(10),
            },
        },
        30_000,
    );
    let packet = release(&mut engine, &actions, "/trades?");
    assert!(packet.headers.is_empty());
    let actions = after_reply(&mut engine, &packet, r#"{"code":200,"trades":[]}"#);
    assert!(matches!(terminal(&actions).as_slice(),
        [Notice::CommandDone { result, .. }] if result == "[]"));
}

fn decoded_tx(packet: &HttpRequest) -> Value {
    let encoded = packet
        .body
        .as_ref()
        .unwrap()
        .split('&')
        .find_map(|part| part.strip_prefix("tx_info="))
        .expect("one native transaction");
    let mut bytes = encoded.bytes();
    let mut output = Vec::new();
    while let Some(byte) = bytes.next() {
        output.push(if byte == b'%' {
            let digits = [bytes.next().unwrap(), bytes.next().unwrap()];
            u8::from_str_radix(std::str::from_utf8(&digits).unwrap(), 16).unwrap()
        } else if byte == b'+' {
            b' '
        } else {
            byte
        });
    }
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn native_transaction_values_and_client_index_result_are_preserved() {
    let mut engine = ready(true);
    let actions = nonce_then_submit(&mut engine);
    let packet = release(&mut engine, &actions, "sendTx");
    let tx = decoded_tx(&packet);
    assert_eq!(tx["Nonce"], 10);
    assert_eq!(tx["AccountIndex"], 718895);
    assert_eq!(tx["ApiKeyIndex"], 1);
    assert_eq!(tx["BaseAmount"], 200);
    assert_eq!(tx["Price"], 9000);
    let actions = after_reply(&mut engine, &packet, r#"{"code":200,"tx_hash":"accepted"}"#);
    assert!(matches!(terminal(&actions).as_slice(),
        [Notice::CommandDone { result, .. }]
            if serde_json::from_str::<Vec<String>>(result).unwrap()
                == vec![tx["ClientOrderIndex"].as_u64().unwrap().to_string()]));
}

#[test]
fn proven_21104_reseed_then_acceptance_is_plain_success() {
    let mut engine = ready(true);
    let actions = nonce_then_submit(&mut engine);
    let first = release(&mut engine, &actions, "sendTx");
    let first_tx = decoded_tx(&first);
    let actions = after_reply(
        &mut engine,
        &first,
        r#"{"code":21104,"message":"invalid nonce"}"#,
    );
    assert!(terminal(&actions).is_empty());
    let (timer, at): (TimerId, f64) = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 150.0 => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("bounded native reseed wait");
    engine.handle(at, Event::TimerFired { timer });
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let nonce = release_at(&mut engine, &actions, "nextNonce", at);
    reply_at(&mut engine, at, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let retry = release_at(&mut engine, &actions, "sendTx", at);
    let tx = decoded_tx(&retry);
    assert_eq!(tx["Nonce"], 20);
    assert_eq!(tx["ClientOrderIndex"], first_tx["ClientOrderIndex"]);
    reply_at(
        &mut engine,
        at,
        &retry,
        200,
        r#"{"code":200,"tx_hash":"accepted"}"#,
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(matches!(terminal(&actions).as_slice(),
        [Notice::CommandDone { result, .. }]
            if serde_json::from_str::<Vec<String>>(result).unwrap()
                == vec![first_tx["ClientOrderIndex"].as_u64().unwrap().to_string()]));
}

#[test]
fn public_l1_setup_keeps_the_address_lookup_candidate() {
    let address = "0x1111111111111111111111111111111111111111";
    let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(
        safe_cex_venue_lighter::Options {
            key: Some(address.into()),
            application_id: None,
            ..options()
        },
    ));
    engine.start(NOW);
    let catalog = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(packet) if packet.url.contains("orderBookDetails") => Some(packet),
            _ => None,
        })
        .expect("released catalogue");
    let actions = after_reply(&mut engine, &catalog, META);
    let lookup = candidate(&actions, "/account?by=l1_address");
    assert!(lookup.url.ends_with(&format!("value={address}")));
    assert!(lookup.headers.is_empty());
}

#[test]
fn secret_only_setup_keeps_the_explicit_account_lookup() {
    assert!(options().key.is_none());
    let (_, actions) = lookup();
    let request = candidate(&actions, "/account?by=index");
    assert!(request.url.ends_with("value=718895"));
    assert!(request.headers.is_empty());
}
