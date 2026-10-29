#![cfg(all(feature = "address-nonce", feature = "test-support"))]
//! The PublicKey sibling of address setup: a host-answered provider binds the
//! slot's public key, at its algorithm's length, through the same task flow.
use safe_cex_engine::book::Book;
use safe_cex_engine::sign_meta::{SetupOperation, SignKind};
use safe_cex_engine::signing::*;
use safe_cex_engine::*;
use std::sync::Arc;

struct Probe {
    key: Option<(AddressSetupSlots, Algorithm)>,
    address: AddressSetupSlots,
}
impl Venue for Probe {
    type Pending = ();
    fn public_address_slots(&self) -> AddressSetupSlots {
        self.address
    }
    fn public_key_setup(&self) -> Option<(AddressSetupSlots, Algorithm)> {
        self.key
    }
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
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

fn engine(
    key: Option<(AddressSetupSlots, Algorithm)>,
    address: AddressSetupSlots,
    support: bool,
) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe { key, address });
    e.set_identities(Identities {
        primary: Some(Identity::new(
            KeyRef("credential".into()),
            Some(KeyRef("signing".into())),
        )),
        secondary: None,
    })
    .unwrap();
    if support {
        e.set_address_setup_host_support(SetupHostSupport::OwnedPublicAddress1)
            .unwrap();
    }
    e
}

fn task(e: &mut Engine<Probe>) -> (RequestId, SignRequest) {
    e.drain_actions()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("setup task")
}

fn retire(e: &mut Engine<Probe>) -> Arc<SetupRetirement> {
    e.drain_actions()
        .find_map(|action| match action {
            Action::RetireAddressSetup { token, .. } => Some(token),
            _ => None,
        })
        .expect("retirement token")
}

fn primary(e: &Engine<Probe>) -> &Identity {
    e.identities().primary.as_ref().unwrap()
}

fn answer(e: &mut Engine<Probe>, id: RequestId, work: &SignRequest, length: usize) {
    e.handle(
        101.,
        Event::Signed {
            attempt: id,
            result: Ok(SignResult::new(vec![vec![7; length]], work.receipt)),
        },
    );
}

#[test]
fn a_host_answered_key_binds_at_its_algorithm_length_after_retirement() {
    for (algorithm, length) in [(Algorithm::StarkDigest, 32), (Algorithm::SchnorrDigest, 40)] {
        let mut e = engine(
            Some((AddressSetupSlots::Primary, algorithm)),
            AddressSetupSlots::None,
            true,
        );
        assert_eq!(
            e.initialize_address_nonce(100.).unwrap(),
            AddressSetupProgress::Pending
        );
        let (id, work) = task(&mut e);
        assert_eq!(
            work.operations,
            vec![Operation::PublicKey {
                algorithm,
                key: KeyRef("signing".into()),
            }]
        );
        assert_eq!(
            e.sign_meta(id).unwrap().kind,
            SignKind::Setup {
                generation: 1,
                operation: SetupOperation::PublicKey(algorithm),
                slot: CredentialSlot::Primary,
            }
        );
        answer(&mut e, id, &work, length);
        // Nothing binds before the host joins the retirement.
        assert!(primary(&e).verified_setup_public_key(algorithm).is_err());
        let token = retire(&mut e);
        assert!(e.address_setup_retired(token, 102.));
        assert!(e.address_nonce_setup_ready());
        assert_eq!(
            primary(&e).verified_setup_public_key(algorithm),
            Ok(&vec![7; length][..])
        );
        // The key is not an address and answers no other algorithm.
        assert!(primary(&e).verified_signing_address().is_err());
        assert!(
            primary(&e)
                .verified_setup_public_key(Algorithm::Ed25519)
                .is_err()
        );
    }
}

#[test]
fn a_key_of_the_wrong_length_never_binds() {
    for length in [0, 20, 32, 41] {
        let mut e = engine(
            Some((AddressSetupSlots::Primary, Algorithm::SchnorrDigest)),
            AddressSetupSlots::None,
            true,
        );
        e.initialize_address_nonce(100.).unwrap();
        let (id, work) = task(&mut e);
        answer(&mut e, id, &work, length);
        let token = retire(&mut e);
        assert!(e.address_setup_retired(token, 102.));
        assert!(
            primary(&e)
                .verified_setup_public_key(Algorithm::SchnorrDigest)
                .is_err(),
            "{length}"
        );
    }
}

#[test]
fn unsupported_hosts_and_conflicting_declarations_refuse() {
    let mut e = engine(
        Some((AddressSetupSlots::Primary, Algorithm::StarkDigest)),
        AddressSetupSlots::None,
        false,
    );
    assert_eq!(
        e.initialize_address_nonce(100.).unwrap(),
        AddressSetupProgress::Ready
    );
    assert_eq!(
        primary(&e).verified_setup_public_key(Algorithm::StarkDigest),
        Err(SignError::Unsupported)
    );
    // A venue declares an address or a key, never both; a keyless algorithm refuses.
    for (key, address) in [
        (
            Some((AddressSetupSlots::Primary, Algorithm::StarkDigest)),
            AddressSetupSlots::Primary,
        ),
        (
            Some((AddressSetupSlots::Primary, Algorithm::HmacSha256)),
            AddressSetupSlots::None,
        ),
    ] {
        let mut e = engine(key, address, true);
        assert!(e.initialize_address_nonce(100.).is_err());
    }
}
