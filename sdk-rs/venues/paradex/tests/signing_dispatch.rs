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
