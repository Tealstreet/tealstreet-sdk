#[path = "support/reference_signer.rs"]
mod reference_signer;
use reference_signer::{Signer, send_body};
// `TxRecipe` through the real engine dispatcher and `LocalSigner` produces
// the legacy `Signer`'s `sendTx`/`sendTxBatch` body byte for byte (R3), and
// holds parked transactions until a valid result arrives (R4).
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::entropy::Entropy;
use safe_cex_engine::signing::Signer as _;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, Identity, IdentitySetupPolicy, KeyEncoding, Operation,
    PayloadPart, SignError,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, RequestId, Response, SocketId,
    TimerId, Venue,
};
use safe_cex_venue_lighter::sign::{Account, Builder, Order, Transaction, TxRecipe};
use std::sync::{Arc, Mutex};

const ORIGIN: &str = "https://mainnet.zklighter.elliot.ai";
const KEY: &str =
    "0x0b2d3a1c9e8f7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a3928170605040302010000000000";
const NOW: f64 = 1_700_000_000_000.;
const ENTROPY: u8 = 0x5a;

fn account() -> Account {
    Account::new(304, 42, 3).unwrap()
}

fn order(order_id: u64) -> Order {
    Order {
        order_id,
        market_id: 1,
        is_buy: true,
        price: 310_000,
        amount: 1_000,
        order_type: 0,
        time_in_force: 1,
        reduce_only: false,
        trigger_price: 0,
        order_expiry: None,
    }
}

fn provider() -> (Arc<LocalSigner>, Identity) {
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            signing_algorithm: Some(Algorithm::SchnorrDigest),
            ..Default::default()
        }),
        secondary: None,
    };
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("lighter-api-key".into()),
                secret: Secret::new(KEY.into()),
                secret_encoding: KeyEncoding::Hex,
                ..Default::default()
            }),
            secondary: None,
        },
        policy,
    )
    .unwrap();
    (Arc::new(signer), ids.primary.unwrap())
}

/// Constant signing entropy, so the hedged nonce matches the legacy fill.
fn entropy() -> Entropy {
    Entropy::new(
        Box::new(|bytes| {
            bytes.fill(1);
            Ok(())
        }),
        Box::new(|bytes| {
            bytes.fill(ENTROPY);
            Ok(())
        }),
    )
}

struct Issue {
    recipe: Option<(String, TxRecipe)>,
    failed: Arc<Mutex<Vec<String>>>,
}
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let (url, recipe) = self.recipe.take().unwrap();
        let id = cx.request(
            Method::Post,
            url,
            vec![("Content-Type", "application/x-www-form-urlencoded".into())],
            None,
            (),
        );
        cx.sign_http(id, Box::new(recipe));
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        self.failed.lock().unwrap().push(error.into());
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// A counter source: each draw differs, exposing a reused entropy prefix.
fn counting_entropy() -> Entropy {
    let mut next = 0u8;
    Entropy::new(
        Box::new(|bytes| {
            bytes.fill(1);
            Ok(())
        }),
        Box::new(move |bytes| {
            next = next.wrapping_add(1);
            bytes.fill(next);
            Ok(())
        }),
    )
}

fn engine(
    signer: Arc<dyn safe_cex_engine::signing::Signer>,
    identity: Identity,
    txs: Vec<(Transaction, u64)>,
) -> (Engine<Issue>, Arc<Mutex<Vec<String>>>) {
    engine_with(signer, identity, txs, entropy())
}

fn engine_with(
    signer: Arc<dyn safe_cex_engine::signing::Signer>,
    identity: Identity,
    txs: Vec<(Transaction, u64)>,
    entropy: Entropy,
) -> (Engine<Issue>, Arc<Mutex<Vec<String>>>) {
    let path = if txs.len() == 1 {
        "sendTx"
    } else {
        "sendTxBatch"
    };
    let failed = Arc::new(Mutex::new(Vec::new()));
    let recipe = TxRecipe::new(identity, account(), txs).unwrap();
    let mut engine = Engine::new(
        Issue {
            recipe: Some((format!("{ORIGIN}/api/v1/{path}"), recipe)),
            failed: failed.clone(),
        },
        Some(entropy),
    )
    .unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    (engine, failed)
}

fn legacy(txs: &[(Transaction, u64)], now_ms: u64) -> (&'static str, String) {
    let signer = Signer::new(KEY, 304, 42, 3).unwrap();
    let signed: Vec<_> = txs
        .iter()
        .map(|(tx, nonce)| {
            signer
                .sign(tx, *nonce, now_ms, |bytes| {
                    bytes.fill(ENTROPY);
                    Ok(())
                })
                .unwrap()
        })
        .collect();
    send_body(&signed)
}

fn protection(order_id: u64, order_type: u8, trigger_price: u32) -> Order {
    Order {
        order_id,
        is_buy: false,
        amount: 0,
        order_type,
        time_in_force: 0,
        reduce_only: true,
        trigger_price,
        ..order(order_id)
    }
}

fn cases() -> Vec<Vec<(Transaction, u64)>> {
    let create = |order_id, builder| Transaction::Create {
        order: order(order_id),
        builder,
    };
    vec![
        vec![(create(7, None), 11)],
        vec![(
            create(
                8,
                Some(Builder {
                    account_index: 99,
                    maker_fee: 5,
                    taker_fee: 10,
                }),
            ),
            12,
        )],
        vec![(
            Transaction::Cancel {
                market_id: 1,
                order_id: 281_474_976_710_655,
            },
            13,
        )],
        vec![(
            Transaction::Leverage {
                market_id: 2,
                leverage: 20.,
                margin_mode: 1,
            },
            14,
        )],
        vec![
            (create(9, None), 15),
            (
                Transaction::Cancel {
                    market_id: 1,
                    order_id: 3,
                },
                16,
            ),
        ],
        vec![(
            Transaction::Grouped {
                grouping: 1,
                orders: vec![order(20), protection(21, 2, 300_000)],
                builder: None,
            },
            17,
        )],
        vec![(
            Transaction::Grouped {
                grouping: 3,
                orders: vec![
                    order(22),
                    protection(23, 2, 300_000),
                    protection(24, 4, 320_000),
                ],
                builder: Some(Builder {
                    account_index: 99,
                    maker_fee: 5,
                    taker_fee: 10,
                }),
            },
            18,
        )],
    ]
}

fn released(engine: &mut Engine<Issue>) -> Vec<HttpRequest> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect()
}

/// R3: Create (with and without builder attributes), grouped OTO and OTOCO,
/// Cancel, Leverage and a batch all equal the legacy wire body byte for byte.
#[test]
fn tx_recipe_matches_the_legacy_send_tx_body() {
    for txs in cases() {
        let (signer, identity) = provider();
        let (mut engine, failed) = engine(signer, identity, txs.clone());
        let http = prepared_actions(&mut engine, NOW)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(http) => Some(http),
                _ => None,
            })
            .expect("signed request released");
        let (path, body) = legacy(&txs, NOW as u64);
        assert_eq!(http.url, format!("{ORIGIN}/api/v1/{path}"));
        assert_eq!(http.body.as_deref(), Some(body.as_str()));
        assert_eq!(
            http.headers,
            vec![("Content-Type", "application/x-www-form-urlencoded".into())]
        );
        assert!(failed.lock().unwrap().is_empty());
    }
}

#[test]
fn tx_recipe_refuses_an_outbound_that_does_not_match_its_path() {
    let (signer, identity) = provider();
    let recipe = TxRecipe::new(identity, account(), cases().remove(0)).unwrap();
    let failed = Arc::new(Mutex::new(Vec::new()));
    let mut engine = Engine::new(
        Issue {
            recipe: Some((format!("{ORIGIN}/api/v1/sendTxBatch"), recipe)),
            failed: failed.clone(),
        },
        Some(entropy()),
    )
    .unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    assert!(
        !prepared_actions(&mut engine, NOW)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(failed.lock().unwrap().len(), 1);
}

struct Parked(Arc<LocalSigner>);
impl safe_cex_engine::signing::Signer for Parked {
    fn immediate(&self) -> bool {
        false
    }
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.0.sign(work)
    }
    fn sign_ready(
        &self,
        _: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, SignError>> {
        None
    }
}

fn parked_with(
    txs: Vec<(Transaction, u64)>,
    entropy: Entropy,
) -> (Engine<Issue>, Arc<LocalSigner>, Arc<Mutex<Vec<String>>>) {
    let (local, identity) = provider();
    let (engine, failed) = engine_with(Arc::new(Parked(local.clone())), identity, txs, entropy);
    (engine, local, failed)
}

fn parked() -> (Engine<Issue>, Arc<LocalSigner>, Arc<Mutex<Vec<String>>>) {
    parked_with(cases().remove(0), entropy())
}

fn work(engine: &mut Engine<Issue>) -> (RequestId, safe_cex_engine::signing::SignRequest) {
    let mut works = vec![];
    for action in engine.drain_actions() {
        match action {
            Action::Sign { attempt, work, .. } => works.push((attempt, work)),
            Action::Http(_) => panic!("parked provider must not send"),
            _ => (),
        }
    }
    assert_eq!(works.len(), 1);
    works.pop().unwrap()
}

fn payloads(work: &safe_cex_engine::signing::SignRequest) -> Vec<&[u8]> {
    work.operations
        .iter()
        .map(|operation| match operation {
            Operation::Sign { payload, .. } => match &payload[..] {
                [PayloadPart::Bytes { bytes }] => bytes.as_slice(),
                _ => panic!("one byte payload"),
            },
            _ => panic!("Sign operations only"),
        })
        .collect()
}

fn payload(work: &safe_cex_engine::signing::SignRequest) -> &[u8] {
    let [payload] = payloads(work)[..] else {
        panic!("one Sign operation");
    };
    payload
}

/// R4: entropy is drawn per transaction and again on every preparation.
#[test]
fn entropy_is_fresh_per_transaction_and_per_preparation() {
    let (mut engine, local, _) = parked_with(cases().remove(4), counting_entropy());
    engine.prepare_dispatch(NOW);
    let (old, old_work) = work(&mut engine);
    let result = local.sign_ready(&old_work).unwrap().unwrap();
    engine.handle(
        NOW + 5_001.,
        Event::Signed {
            attempt: old,
            result: Ok(result),
        },
    );
    let (_, fresh_work) = work(&mut engine);
    let prefixes: Vec<&[u8]> = [&old_work, &fresh_work]
        .into_iter()
        .flat_map(payloads)
        .map(|payload| &payload[..32])
        .collect();
    assert_eq!(prefixes.len(), 4);
    for (index, prefix) in prefixes.iter().enumerate() {
        assert!(
            prefixes[index + 1..].iter().all(|other| other != prefix),
            "entropy prefix {index} was reused"
        );
    }
}

/// R4: a parked provider holds the transaction; a stale result releases
/// nothing; re-preparation keeps the transaction nonce and re-stamps expiry.
#[test]
fn parked_provider_holds_the_transaction_and_keeps_its_nonce() {
    let (mut engine, local, failed) = parked();
    engine.prepare_dispatch(NOW);
    let (old, old_work) = work(&mut engine);
    assert_eq!(old_work.receipt.stamp_ms, Some(NOW));
    assert_eq!(old_work.receipt.expiry_ms, Some(NOW + 600_000.));
    let old_result = local.sign_ready(&old_work).unwrap().unwrap();
    let later = NOW + 5_001.;
    engine.handle(
        later,
        Event::Signed {
            attempt: old,
            result: Ok(old_result.clone()),
        },
    );
    let (fresh, fresh_work) = work(&mut engine);
    assert_ne!(old, fresh);
    assert_eq!(fresh_work.receipt.stamp_ms, Some(later));
    // Words after the 32 entropy bytes: chain, type, nonce, expiry.
    let word = |work, index: usize| {
        let bytes = &payload(work)[32 + index * 8..32 + index * 8 + 8];
        u64::from_le_bytes(bytes.try_into().unwrap())
    };
    assert_eq!(word(&old_work, 2), 11);
    assert_eq!(
        word(&fresh_work, 2),
        11,
        "re-preparation keeps the tx nonce"
    );
    assert_eq!(word(&fresh_work, 3), later as u64 + 600_000);
    engine.handle(
        later,
        Event::Signed {
            attempt: old,
            result: Ok(old_result),
        },
    );
    assert!(
        released(&mut engine).is_empty(),
        "a stale result releases nothing"
    );
    engine.handle(
        later + 1.,
        Event::Signed {
            attempt: fresh,
            result: Ok(local.sign_ready(&fresh_work).unwrap().unwrap()),
        },
    );
    let requests = released(&mut engine);
    assert_eq!(requests.len(), 1);
    let (_, body) = legacy(&cases().remove(0), later as u64);
    assert_eq!(requests[0].body.as_deref(), Some(body.as_str()));
    assert!(failed.lock().unwrap().is_empty());
}

/// R4: a refusal or a malformed result fails typed and never sends.
#[test]
fn parked_refusals_and_malformed_results_never_send() {
    for kind in 0..4 {
        let (mut engine, local, failed) = parked();
        engine.prepare_dispatch(NOW);
        let (attempt, work) = work(&mut engine);
        let mut result = local.sign_ready(&work).unwrap().unwrap();
        let result = match kind {
            0 => Err(SignError::Unavailable),
            1 => {
                result.values[0].truncate(64);
                Ok(result)
            }
            2 => {
                result.values.push(vec![0; 80]);
                Ok(result)
            }
            _ => {
                result.values.clear();
                Ok(result)
            }
        };
        engine.handle(NOW + 1., Event::Signed { attempt, result });
        assert!(
            released(&mut engine).is_empty(),
            "kind {kind} must not send"
        );
        assert_eq!(failed.lock().unwrap().len(), 1, "kind {kind} fails typed");
    }
}
