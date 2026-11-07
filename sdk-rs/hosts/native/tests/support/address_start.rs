use safe_cex_engine::book::Book;
use safe_cex_engine::nonce::NonceDomain;
use safe_cex_engine::signing::{Identities, Provider, SignError};
use safe_cex_engine::*;
use std::sync::{Arc, Mutex};

pub type Observations = Arc<Mutex<Vec<Vec<Result<[u8; 20], SignError>>>>>;
pub const ADDRESS: [u8; 20] = [
    0x7e, 0x5f, 0x45, 0x52, 0x09, 0x1a, 0x69, 0x12, 0x5d, 0x5d, 0xfc, 0xb7, 0xb8, 0xc2, 0x65, 0x90,
    0x29, 0x39, 0x5b, 0xdf,
];
pub struct Probe {
    pub observations: Observations,
    pub domain: Option<&'static NonceDomain>,
    pub slots: AddressSetupSlots,
}
impl Venue for Probe {
    type Pending = ();
    fn address_nonce_domain(&self) -> Option<&'static NonceDomain> {
        self.domain
    }
    fn public_address_slots(&self) -> AddressSetupSlots {
        self.slots
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let ids = cx.address_nonce_identities();
        self.observations.lock().unwrap().push(
            [&ids.primary, &ids.secondary]
                .into_iter()
                .flatten()
                .map(|id| id.verified_signing_address())
                .collect(),
        );
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
pub fn engine(probe: Probe, signer: Option<Provider>, identities: Identities) -> Engine<Probe> {
    let fill = || {
        Box::new(|bytes: &mut [u8]| {
            bytes.fill(17);
            Ok(())
        }) as entropy::Fill
    };
    let mut engine = Engine::new(probe, Some(entropy::Entropy::new(fill(), fill()))).unwrap();
    engine.set_identities(identities).unwrap();
    engine.set_signer(signer).unwrap();
    engine
}
