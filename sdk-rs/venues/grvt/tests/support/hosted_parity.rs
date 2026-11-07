//! Existing trading fixtures also cross the device-owned signing boundary.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, Identities, IdentitySetupPolicy, KeyEncoding,
    KeyRef, Operation, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Engine, test_support::seeded_engine};
use safe_cex_venue_grvt::{Grvt, Options};
use safe_cex_venues::hosted_wallet::{self, Account};

struct Parity {
    local: LocalSigner,
    ids: Identities,
    account: Account,
}
impl Parity {
    fn check(&self, work: &SignRequest) -> Result<SignResult, SignError> {
        let local = self.local.sign_now(work)?;
        if let [
            Operation::Sha256 { payload },
            Operation::PublicAddress { .. },
            Operation::Sign { .. },
        ] = work.operations.as_slice()
        {
            let safe_cex_engine::signing::PayloadPart::Bytes { bytes } = &payload[0] else {
                panic!("statement bytes");
            };
            let device: safe_cex_engine::signing::Provider = std::sync::Arc::new(setup().0);
            let output = hosted_wallet::sign(
                "grvt",
                &self.ids,
                &self.account,
                &device,
                std::str::from_utf8(bytes).unwrap(),
                "grvt-order",
                work.receipt.expiry_ms.unwrap() - 3_600_000.,
            )
            .unwrap();
            let decoded: Vec<_> = output[2..]
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            assert_eq!(
                decoded,
                local.values[1..].concat(),
                "device and local address/signature bytes"
            );
        }
        Ok(local)
    }
}
fn setup() -> (LocalSigner, Identities) {
    LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("grvt-test-key".into()),
                secret: Secret::new("01".repeat(32)),
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Hex),
                signing_algorithm: Some(Algorithm::Secp256k1Digest),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap()
}
impl Signer for Parity {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(self.check(work))
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(self.check(&work)))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
}
pub fn engine(mut options: Options) -> Engine<Grvt> {
    if std::env::var_os("GRVT_LEGACY_CONTROL").is_some() {
        return seeded_engine(Grvt::new(options));
    }
    let (local, ids) = setup();
    options.signer_address = Some("0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1".into());
    let account = Account {
        testnet: options.testnet,
        grvt_trading_account_id: options.trading_account_id.clone(),
        ..Default::default()
    };
    let mut engine = seeded_engine(Grvt::with_signing(options, ids.primary.clone()));
    engine
        .set_signer(Some(std::sync::Arc::new(Parity {
            local,
            ids: ids.clone(),
            account,
        })))
        .unwrap();
    engine.set_identities(ids).unwrap();
    engine
}
