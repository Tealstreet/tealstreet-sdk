//! Current-API originals. No verified address, setup answer or new seam is fabricated.
//! KRAKEN below is only an existing x1000 arithmetic probe, not an Aster claim.
#![cfg(feature = "test-support")]

use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{HYPERLIQUID_AGENT, KRAKEN, NonceDomain, NonceError, NonceKey};
use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_engine::{Action, Command, Cx, Response, SocketId, TimerId, Venue};
use std::sync::{Arc, Mutex};

type Results = Arc<Mutex<Vec<Result<u64, NonceError>>>>;

struct CounterProbe {
    domain: &'static NonceDomain,
    address: String,
    count: usize,
    results: Results,
}

impl Venue for CounterProbe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let key = NonceKey::ethereum_address(&self.address);
        for _ in 0..self.count {
            self.results
                .lock()
                .unwrap()
                .push(cx.reserve_nonce(self.domain, &key));
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn address(case: u64) -> String {
    format!("0x{:040x}", 0x2093_0000_u64 + case)
}

fn reserve(
    domain: &'static NonceDomain,
    agent: &str,
    now: f64,
    count: usize,
) -> Vec<Result<u64, NonceError>> {
    let results = Results::default();
    let mut engine = safe_cex_engine::test_support::seeded_engine(CounterProbe {
        domain,
        address: agent.into(),
        count,
        results: results.clone(),
    });
    engine.start(now);
    assert!(!engine.drain_actions().any(|action| matches!(
        action,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
    results.lock().unwrap().clone()
}

fn values(domain: &'static NonceDomain, agent: &str, now: f64, count: usize) -> Vec<u64> {
    reserve(domain, agent, now, count)
        .into_iter()
        .map(Result::unwrap)
        .collect()
}

fn cold_replacement(test: &str, domain: &'static NonceDomain, case: u64, scale: u64) {
    let agent = address(case);
    let floor = 100 * scale;
    if std::env::var("ADDRESS_NONCE_COLD_PROBE").as_deref() == Ok(test) {
        let child = values(domain, &agent, 100., 2);
        assert_eq!(child, [floor, floor + 1]);
        println!(
            "ADDRESS_NONCE_COLD:{}",
            serde_json::to_string(&child).unwrap()
        );
        return;
    }
    let prior = values(domain, &agent, 100., 3);
    assert_eq!(prior, [floor, floor + 1, floor + 2]);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("ADDRESS_NONCE_COLD_PROBE", test)
        .output()
        .unwrap();
    assert!(output.status.success(), "cold child: {output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let child: Vec<u64> = stdout
        .lines()
        .find_map(|line| line.split_once("ADDRESS_NONCE_COLD:").map(|(_, json)| json))
        .map(|json| serde_json::from_str(json).unwrap())
        .expect("joined child reports its actual counter values");
    assert_eq!(child[0], prior[0], "cold memory may repeat a prior nonce");
    assert!(child[0] < *prior.last().unwrap());
    assert_eq!(values(domain, &agent, 99., 1), [floor + 3]);
}

#[test]
fn cold_process_replacement_is_an_unpersisted_boundary() {
    cold_replacement(
        "cold_process_replacement_is_an_unpersisted_boundary",
        &HYPERLIQUID_AGENT,
        1,
        1,
    );
}

#[test]
fn same_stamp_scaled_floor_repeats_after_cold_replacement() {
    cold_replacement(
        "same_stamp_scaled_floor_repeats_after_cold_replacement",
        &KRAKEN,
        2,
        1_000,
    );
}

#[test]
fn live_engine_recreation_preserves_the_existing_high_water() {
    let agent = address(3);
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 100., 3), [100, 101, 102]);
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 1., 2), [103, 104]);
}

#[test]
fn normalized_spellings_share_one_live_address_sequence() {
    let lower = address(0xab);
    let upper = lower.to_ascii_uppercase();
    assert_eq!(values(&HYPERLIQUID_AGENT, &lower, 100., 1), [100]);
    assert_eq!(values(&HYPERLIQUID_AGENT, &upper, 100., 1), [101]);
    assert_eq!(values(&HYPERLIQUID_AGENT, &lower[2..], 99., 1), [102]);
}

#[test]
fn different_addresses_remain_independent_at_the_same_stamp() {
    let a = address(4);
    let b = address(5);
    assert_eq!(values(&HYPERLIQUID_AGENT, &a, 100., 2), [100, 101]);
    assert_eq!(values(&HYPERLIQUID_AGENT, &b, 100., 2), [100, 101]);
    assert_eq!(values(&HYPERLIQUID_AGENT, &a, 99., 1), [102]);
}

#[test]
fn distinct_domains_do_not_merge_an_equal_address() {
    let agent = address(6);
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 100., 1), [100]);
    assert_eq!(values(&KRAKEN, &agent, 100., 1), [100_000]);
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 99., 1), [101]);
}

#[test]
fn more_than_one_thousand_same_stamp_reservations_remain_monotonic() {
    let agent = address(7);
    let actual = values(&HYPERLIQUID_AGENT, &agent, 100., 1_505);
    assert_eq!(actual, (100..1_605).collect::<Vec<_>>());
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 99., 1), [1_605]);
}

#[test]
fn parallel_engines_share_one_per_address_counter() {
    let agent = address(8);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let agent = agent.clone();
            std::thread::spawn(move || values(&HYPERLIQUID_AGENT, &agent, 100., 64))
        })
        .collect();
    let mut actual: Vec<_> = handles
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(actual, (100..356).collect::<Vec<_>>());
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 99., 1), [356]);
}

#[test]
fn invalid_time_does_not_seed_a_counter_and_the_safe_limit_exhausts() {
    let agent = address(9);
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
        assert_eq!(
            reserve(&HYPERLIQUID_AGENT, &agent, now, 1),
            [Err(NonceError::InvalidTime)]
        );
    }
    assert_eq!(values(&HYPERLIQUID_AGENT, &agent, 100., 1), [100]);
    let limit = (1_u64 << 53) - 1;
    let exhausted = address(10);
    assert_eq!(
        values(&HYPERLIQUID_AGENT, &exhausted, limit as f64, 1),
        [limit]
    );
    assert_eq!(
        reserve(&HYPERLIQUID_AGENT, &exhausted, 0., 1),
        [Err(NonceError::Exhausted)]
    );
}

fn identity(stable: Option<String>) -> Identity {
    Identity::new(
        KeyRef("address-original/credential".into()),
        Some(KeyRef("address-original/signing".into())),
    )
    .with_stable_id(stable)
}

#[cfg(feature = "dispatch-nonce")]
mod proposed_address_contract {
    use super::*;
    use safe_cex_engine::nonce::NonceSelectionError;
    use safe_cex_engine::signing::SignError;

    // These hostile inputs have NO verified binding. The existing stable-only
    // mint currently accepts them; no successful address setup is simulated.
    #[test]
    fn stable_id_text_cannot_mint_an_unverified_address_claim() {
        let slots = Slots::new(
            Some(identity(Some(address(11)))),
            None,
            SlotPolicy::default(),
        );
        let result = slots.select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT);
        assert_eq!(result.err(), Some(NonceSelectionError::InvalidSubject));
    }

    #[test]
    fn serialized_address_metadata_cannot_create_a_private_binding() {
        let incoming = serde_json::json!({
            "credential": "address-original/credential",
            "signing": "address-original/signing",
            "stableId": address(12),
            "verifiedAddress": {"address": vec![0_u8; 20], "signing": "address-original/signing",
                "algorithm": "secp256k1Digest"}
        });
        let identity: Identity = match serde_json::from_value(incoming) {
            Ok(identity) => identity,
            Err(_) => return, // Refusing unknown forged metadata also denies the grant.
        };
        let slots = Slots::new(Some(identity), None, SlotPolicy::default());
        assert_eq!(
            slots
                .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
                .err(),
            Some(NonceSelectionError::InvalidSubject)
        );
    }

    #[test]
    fn missing_signing_is_classified_before_the_address_subject() {
        let identity = Identity::new(KeyRef("address-original/credential".into()), None);
        let slots = Slots::new(Some(identity), None, SlotPolicy::default());
        let error = slots
            .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
            .err()
            .expect("a missing signing ref cannot create a claim");
        assert_eq!(error.sign_error(), SignError::MissingKey);
    }

    #[test]
    fn existing_kraken_stable_subject_mint_remains_available() {
        let slots = Slots::new(
            Some(identity(Some("address-original/kraken-control".into()))),
            None,
            SlotPolicy::default(),
        );
        let (selection, selected, claim) = slots
            .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
            .unwrap();
        assert_eq!(selection.slot(), claim.slot());
        assert_eq!(
            selected.require_stable_id().unwrap(),
            "address-original/kraken-control"
        );
    }
}

#[cfg(not(feature = "dispatch-nonce"))]
#[test]
fn feature_off_mint_stays_unavailable_without_dispatcher_state() {
    let slots = Slots::new(
        Some(identity(Some("address-original/off-control".into()))),
        None,
        SlotPolicy::default(),
    );
    for domain in [&KRAKEN, &HYPERLIQUID_AGENT] {
        assert!(
            slots
                .select_with_nonce(CredentialRole::Mutation, domain)
                .is_err()
        );
    }
}

#[test]
fn ordinary_identity_wire_contains_no_address_binding() {
    let original = identity(Some("address-original/wire-control".into()));
    let value = serde_json::to_value(&original).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "credential": "address-original/credential",
            "signing": "address-original/signing",
            "stableId": "address-original/wire-control"
        })
    );
    assert_eq!(serde_json::from_value::<Identity>(value).unwrap(), original);
}
