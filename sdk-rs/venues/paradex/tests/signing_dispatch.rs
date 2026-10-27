//! Dispatch-time Stark signing: the release clock stamps every signature, a
//! batch shares one stamp, and no provider result binds a request unless each
//! value verifies for its own hash under the sealed key.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::signing::{PayloadPart, SignRequest, SignResult};
use safe_cex_engine::{Action, Command, Event, RequestId};
use safe_cex_venue_paradex::Options;
use serde_json::Value;
use support::*;

const CONFIG: &str = r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#;
/// The Stark curve order, big-endian.
const ORDER: [u8; 32] = [
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xb7, 0x81, 0x12, 0x6d, 0xca, 0xe7, 0xb2, 0x32, 0x1e, 0x66, 0xa2, 0x41, 0xad, 0xc6, 0x4d, 0x2f,
];

fn options() -> Options {
    Options {
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    }
}

fn settle(h: &mut Harness) {
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
}

fn private() -> Harness {
    let mut h = Harness::keyed(options(), STARK_KEY);
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply("/system/config", CONFIG);
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    settle(&mut h);
    h
}

/// The host-answered provider's pending signature work.
fn sign_work(h: &mut Harness) -> (RequestId, SignRequest) {
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::Sign { .. }))
        .expect("pending signature work");
    let Action::Sign { attempt, work, .. } = h.actions.remove(i) else {
        unreachable!()
    };
    (attempt, work)
}

fn provider_sign(provider: &safe_cex_engine::signing::Provider, work: SignRequest) -> SignResult {
    let mut future = provider.sign(work);
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(result) => result.unwrap(),
        std::task::Poll::Pending => unreachable!("local signing completes inline"),
    }
}

fn answer(h: &mut Harness, attempt: RequestId, result: SignResult) {
    h.e.handle(
        h.now,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    h.collect();
}

fn deferred() -> Harness {
    let mut h = Harness::deferred(options(), STARK_KEY);
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply("/system/config", CONFIG);
    let (attempt, work) = sign_work(&mut h);
    let result = provider_sign(&h.e.signer().unwrap(), work);
    answer(&mut h, attempt, result);
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    settle(&mut h);
    h
}

fn order(client: &str, amount: f64) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTC-USD-PERP","type":{},"side":"buy","amount":{amount},"price":100.1,"clientOrderId":"{client}"}}"#,
        serde_json::to_string(&OrderType::Limit).unwrap()
    ))
    .unwrap()
}

fn stamps(body: &str) -> Vec<u64> {
    let legs: Value = serde_json::from_str(body).unwrap();
    legs.as_array()
        .unwrap()
        .iter()
        .map(|leg| leg["signature_timestamp"].as_u64().unwrap())
        .collect()
}

#[test]
fn a_write_planned_behind_a_jwt_refresh_signs_with_its_release_clock() {
    let mut h = private();
    // The JWT is past its refresh point: the write plans now and queues.
    let planned = h.now + 180_001.;
    h.now = planned;
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order("one", 1.5)],
    });
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders")));
    let auth = h.take("/auth/");
    h.now = planned + 5_000.25;
    h.respond(auth, 200, r#"{"jwt_token":"jwt-2"}"#);
    let write = h.take("/orders/batch");
    assert_eq!(
        stamps(write.body.as_deref().unwrap()),
        vec![(planned + 5_000.) as u64]
    );
}

/// Preservation, not RED: base also stamped every leg with one floored clock.
#[test]
fn every_leg_of_a_batch_shares_one_floored_release_stamp() {
    let mut h = private();
    h.now += 0.75;
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![order("a", 1.5), order("b", 2.5), order("c", 3.5)],
    });
    let write = h.take("/orders/batch");
    assert_eq!(
        stamps(write.body.as_deref().unwrap()),
        vec![1_700_000_000_000; 3]
    );
}

/// RED against base, which stamped every leg with its plan clock.
#[test]
fn a_batch_planned_behind_a_jwt_refresh_shares_its_floored_release_stamp() {
    let mut h = private();
    let planned = h.now + 180_001.25;
    h.now = planned;
    h.command(Command::PlaceOrders {
        command: 3,
        orders: vec![order("a", 1.5), order("b", 2.5), order("c", 3.5)],
    });
    let auth = h.take("/auth/");
    h.now = planned + 7_000.5;
    h.respond(auth, 200, r#"{"jwt_token":"jwt-2"}"#);
    let write = h.take("/orders/batch");
    assert_eq!(
        stamps(write.body.as_deref().unwrap()),
        vec![h.now.floor() as u64; 3]
    );
    assert_ne!(h.now.floor() as u64, planned.floor() as u64);
}

#[test]
fn only_verified_results_for_the_exact_hashes_bind_a_batch_body() {
    for case in 0..9 {
        let mut h = deferred();
        h.command(Command::PlaceOrders {
            command: 3,
            orders: vec![order("a", 1.5), order("b", 2.5)],
        });
        // No HTTP exists before the provider answers.
        assert!(!h.pending.iter().any(|r| r.url.contains("/orders/batch")));
        let (attempt, work) = sign_work(&mut h);
        let provider = h.e.signer().unwrap();
        let mut result = provider_sign(&provider, work.clone());
        match case {
            0 => {}
            // r and s swapped.
            1 => {
                let (r, s) = result.values[0].split_at_mut(32);
                r.swap_with_slice(s);
            }
            2 => drop(result.values.pop()),
            3 => result.values.push(result.values[0].clone()),
            4 => result.values.swap(0, 1),
            // A well-formed signature over another digest.
            5 => {
                let mut other = work.clone();
                if let safe_cex_engine::signing::Operation::Sign { payload, .. } =
                    &mut other.operations[0]
                    && let PayloadPart::Bytes { bytes } = &mut payload[0]
                {
                    bytes[31] ^= 1;
                }
                result.values[0] = provider_sign(&provider, other).values.remove(0);
            }
            // A well-formed signature by another key.
            6 => {
                let (_, other, _) = keyed(options(), &format!("0x{}2", "0".repeat(63)));
                result.values[0] = provider_sign(&other, work.clone()).values.remove(0);
            }
            7 => result.values[0][..32].fill(0),
            8 => result.values[0][..32].copy_from_slice(&ORDER),
            _ => unreachable!(),
        }
        answer(&mut h, attempt, result);
        let sent = h.pending.iter().any(|r| r.url.contains("/orders/batch"));
        assert_eq!(sent, case == 0, "case {case}");
        if case != 0 {
            assert!(h.result(3).is_err(), "case {case}");
        }
    }
}

#[test]
fn an_auth_result_that_does_not_verify_never_sends_auth() {
    let mut h = Harness::deferred(options(), STARK_KEY);
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply("/system/config", CONFIG);
    let (attempt, work) = sign_work(&mut h);
    let mut result = provider_sign(&h.e.signer().unwrap(), work);
    let (r, s) = result.values[0].split_at_mut(32);
    r.swap_with_slice(s);
    answer(&mut h, attempt, result);
    assert!(!h.pending.iter().any(|r| r.url.contains("/auth/")));
}

/// The latest signing deadline the engine scheduled.
fn deadline(h: &Harness) -> f64 {
    h.actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { at_ms, .. } if *at_ms > h.now => Some(*at_ms),
            _ => None,
        })
        .expect("signing deadline")
}

fn not_sent(h: &Harness, command: u32) {
    assert!(h.result(command).is_err(), "command {command}");
    assert_eq!(
        h.failures.last().unwrap().outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
}

fn swap_r_and_s(result: &mut SignResult) {
    let (r, s) = result.values[0].split_at_mut(32);
    r.swap_with_slice(s);
}

#[test]
fn an_expired_signature_never_revives_its_write() {
    let mut h = deferred();
    h.command(Command::PlaceOrders {
        command: 4,
        orders: vec![order("a", 1.5)],
    });
    let (attempt, work) = sign_work(&mut h);
    let result = provider_sign(&h.e.signer().unwrap(), work);
    let at = deadline(&h);
    h.timer(at);
    not_sent(&h, 4);
    answer(&mut h, attempt, result);
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders")));
    // The job is released: the next write signs and sends.
    h.command(Command::PlaceOrders {
        command: 5,
        orders: vec![order("b", 2.5)],
    });
    let (attempt, work) = sign_work(&mut h);
    let result = provider_sign(&h.e.signer().unwrap(), work);
    answer(&mut h, attempt, result);
    assert!(h.pending.iter().any(|r| r.url.contains("/orders/batch")));
}

#[test]
fn a_refused_auth_signature_retries_and_the_account_recovers() {
    let mut h = Harness::deferred(options(), STARK_KEY);
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply("/system/config", CONFIG);
    let (attempt, work) = sign_work(&mut h);
    // A write queues behind the pending JWT.
    h.command(Command::PlaceOrders {
        command: 6,
        orders: vec![order("a", 1.5)],
    });
    let mut result = provider_sign(&h.e.signer().unwrap(), work);
    swap_r_and_s(&mut result);
    answer(&mut h, attempt, result);
    assert!(!h.pending.iter().any(|r| r.url.contains("/auth/")));
    not_sent(&h, 6);
    // The real retry signs a fresh /auth, and private work proceeds.
    let retry = h.now + 2_000.;
    h.timer(retry);
    let (attempt, work) = sign_work(&mut h);
    let result = provider_sign(&h.e.signer().unwrap(), work);
    answer(&mut h, attempt, result);
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    settle(&mut h);
    h.command(Command::PlaceOrders {
        command: 7,
        orders: vec![order("b", 2.5)],
    });
    let (attempt, work) = sign_work(&mut h);
    let result = provider_sign(&h.e.signer().unwrap(), work);
    answer(&mut h, attempt, result);
    let write = h.take("/orders/batch");
    h.respond(write, 200, r#"{"orders":[{"id":"id-b","client_id":"b"}]}"#);
    assert_eq!(h.result(7), Ok("[\"id-b\"]"));
}

#[test]
fn a_refused_order_or_amend_signature_releases_its_job_and_lock() {
    let mut h = deferred();
    h.command(Command::PlaceOrders {
        command: 8,
        orders: vec![order("a", 1.5)],
    });
    let (attempt, work) = sign_work(&mut h);
    let mut result = provider_sign(&h.e.signer().unwrap(), work);
    swap_r_and_s(&mut result);
    answer(&mut h, attempt, result);
    not_sent(&h, 8);
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders")));
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    h.e.handle(h.now, Event::WsMessage { socket: private, text: r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"SELL","size":"2","remaining_size":"2","price":"120","flags":[],"instruction":"GTC","created_at":1700000000000}}}"# });
    h.collect();
    let amend = |h: &mut Harness, command| {
        h.command(Command::AmendOrder {
            command,
            id: "123".into(),
            price: Some(121.5),
            amount: Some(3.),
        });
        sign_work(h)
    };
    let (attempt, work) = amend(&mut h, 9);
    let mut result = provider_sign(&h.e.signer().unwrap(), work);
    swap_r_and_s(&mut result);
    answer(&mut h, attempt, result);
    not_sent(&h, 9);
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders/123")));
    // As for any refused amendment, the row waits for newer venue evidence.
    h.command(Command::AmendOrder {
        command: 11,
        id: "123".into(),
        price: Some(121.5),
        amount: Some(3.),
    });
    not_sent(&h, 11);
    h.now += 1.;
    let update = format!(
        r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"orders.ALL","data":{{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"SELL","size":"2","remaining_size":"2","price":"120","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":{}}}}}}}"#,
        h.now as u64
    );
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: &update,
        },
    );
    h.collect();
    // The amend lock was released: the same order amends again and sends.
    let (attempt, work) = amend(&mut h, 10);
    let result = provider_sign(&h.e.signer().unwrap(), work);
    answer(&mut h, attempt, result);
    let request = h.take("/orders/123");
    h.respond(request, 200, r#"{"id":"123"}"#);
    assert_eq!(h.result(10), Ok("[\"123\"]"));
}

#[test]
fn an_ethereum_key_on_a_mismatched_or_incomplete_config_never_authenticates() {
    const ETHEREUM: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    let complete = r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET","l1_chain_id":"1","paraclear_account_hash":"0x123","paraclear_account_proxy_hash":"0x456"}"#;
    for (config, authenticates) in [
        (complete.to_owned(), true),
        (
            complete.replace(r#""l1_chain_id":"1""#, r#""l1_chain_id":"11155111""#),
            false,
        ),
        (complete.replace(r#""l1_chain_id":"1","#, ""), false),
        (
            complete.replace(r#","paraclear_account_hash":"0x123""#, ""),
            false,
        ),
        (
            complete.replace(r#","paraclear_account_proxy_hash":"0x456""#, ""),
            false,
        ),
    ] {
        let mut h = Harness::keyed(options(), ETHEREUM);
        h.reply("/markets", MARKETS);
        h.reply("summary", TICKERS);
        h.reply("/system/config", &config);
        assert_eq!(
            h.pending.iter().any(|r| r.url.contains("/auth/")),
            authenticates,
            "{config}"
        );
    }
}

/// No engine API cancels one command's held signature; the host retires it by
/// reporting the attempt failed (its own timeout, abort or disposal).
#[test]
fn a_host_retired_signature_never_revives_its_write() {
    for error in [
        safe_cex_engine::signing::SignError::Unavailable,
        safe_cex_engine::signing::SignError::Refused,
    ] {
        let mut h = deferred();
        h.command(Command::PlaceOrders {
            command: 12,
            orders: vec![order("a", 1.5)],
        });
        let (attempt, work) = sign_work(&mut h);
        let result = provider_sign(&h.e.signer().unwrap(), work);
        h.e.handle(
            h.now,
            Event::Signed {
                attempt,
                result: Err(error),
            },
        );
        h.collect();
        not_sent(&h, 12);
        answer(&mut h, attempt, result);
        assert!(!h.pending.iter().any(|r| r.url.contains("/orders")));
    }
}
