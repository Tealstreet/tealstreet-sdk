//! Real native venue client IDs: deterministic only under explicit host injection.
#![cfg(not(target_arch = "wasm32"))]
#[path = "../../../venues/binance/tests/support/mod.rs"]
mod support;
use safe_cex_engine::entropy::{Entropy, Fill};
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice};
use safe_cex_venue_binance::BinanceFutures;

fn venue() -> (BinanceFutures, safe_cex_engine::signing::Provider) {
    support::Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..support::Options::default()
    }
    .build()
}
fn client_ids(entropy: Entropy) -> Result<Vec<String>, String> {
    let public = support::Harness::new();
    let (venue, signer) = venue();
    let mut engine = Engine::new(venue, Some(entropy)).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine
        .store_mut()
        .set_markets(public.engine.store().markets().to_vec());
    engine
        .store_mut()
        .set_tickers(public.engine.store().tickers().to_vec());
    let mut ids = Vec::new();
    for command in 1..=3 {
        let order: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        engine.handle(
            1000.,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order],
            }),
        );
        for action in safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.) {
            match action {
                Action::Http(request) => {
                    let query = request.url.split_once('?').unwrap().1;
                    ids.push(
                        query
                            .split('&')
                            .find_map(|pair| pair.strip_prefix("newClientOrderId="))
                            .unwrap()
                            .into(),
                    );
                }
                Action::Notice(Notice::CommandFailed { error, .. }) => return Err(error.message),
                _ => {}
            }
        }
    }
    Ok(ids)
}
#[test]
fn seeded_native_venue_ids_repeat_but_production_ids_do_not() {
    let seeded = || safe_cex_engine::test_support::seeded_entropy(73);
    let first = client_ids(seeded()).unwrap();
    assert_eq!(first.len(), 3);
    assert_ne!(first[0], first[1]);
    assert_eq!(first, client_ids(seeded()).unwrap());
    assert_ne!(
        first,
        client_ids(safe_cex_engine::test_support::seeded_entropy(74)).unwrap()
    );
    assert_ne!(
        client_ids(safe_cex_native::production_entropy()).unwrap(),
        client_ids(safe_cex_native::production_entropy()).unwrap()
    );
}
#[test]
fn missing_provider_is_typed_at_construction_and_replacement_freezes_at_start() {
    let error = Engine::new(venue().0, None).err().unwrap();
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    let (venue, signer) = venue();
    let mut engine = Engine::new(venue, Some(safe_cex_native::production_entropy())).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(0.);
    assert_eq!(
        engine.set_entropy(Box::new(|_| Ok(()))).unwrap_err().kind,
        Some(ErrorKind::InvalidInput)
    );
    assert_eq!(
        engine
            .set_signing_entropy(Box::new(|_| Ok(())))
            .unwrap_err()
            .kind,
        Some(ErrorKind::InvalidInput)
    );
}
#[test]
fn identity_failure_does_not_send_a_native_order() {
    let failure: Fill = Box::new(|bytes| {
        bytes.fill(1);
        Err("fixture entropy unavailable".into())
    });
    let entropy = Entropy::new(
        failure,
        Box::new(|_| panic!("Binance uses no signing random draw")),
    );
    assert!(
        client_ids(entropy)
            .unwrap_err()
            .contains("entropy unavailable")
    );
}
