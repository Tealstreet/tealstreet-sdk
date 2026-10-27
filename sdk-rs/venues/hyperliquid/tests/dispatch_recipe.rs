use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::HYPERLIQUID_AGENT;
use safe_cex_engine::request_context::plan::{CallAuth, SerialCall, SerialPolicy};
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation, PreflightPolicy, PreflightPurpose, Prepared,
    ReconciliationData,
};
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, KeyRef, OwnedSetupExecution, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_engine::{
    Action, AddressSetupSlots, Command, Cx, Engine, Event, HttpRelease, HttpRequest, Method,
    RequestId, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_hyperliquid::sign::{self, dispatch::Exchange};
use serde::Serialize;
use std::sync::Arc;

const KEY: &str = "00000000000000000000000000000000000000000000000000000000000000ab";
#[derive(Clone, Serialize)]
struct Schedule {
    #[serde(rename = "type")]
    kind: &'static str,
    time: u64,
}
struct DelayedLocal(Arc<LocalSigner>);
impl Signer for DelayedLocal {
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.0.sign(work)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
    fn owned_public_address_capability(&self) -> Capability {
        self.0.owned_public_address_capability()
    }
    fn create_public_address_setup(
        &self,
        work: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        self.0.create_public_address_setup(work)
    }
}
struct Probe {
    testnet: bool,
    vault: Option<String>,
    wrong_url: bool,
}
impl Venue for Probe {
    type Pending = Context;
    fn address_nonce_domain(&self) -> Option<&'static safe_cex_engine::nonce::NonceDomain> {
        Some(&HYPERLIQUID_AGENT)
    }
    fn public_address_slots(&self) -> AddressSetupSlots {
        AddressSetupSlots::Primary
    }
    fn dispatch_nonce() -> bool {
        true
    }
    fn http_release() -> HttpRelease<Self> {
        HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        ReconciliationData::new("hl-schedule-control".into())
    }
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let identities = cx.address_nonce_identities();
        let slots = Slots::new(identities.primary.clone(), None, SlotPolicy::default());
        let (selection, identity, claim) = slots
            .select_with_nonce(CredentialRole::Mutation, &HYPERLIQUID_AGENT)
            .unwrap();
        assert!(identity.credential_ref().is_none());
        let authority = selection.authority(identity);
        let recipe =
            Exchange::new(identity, claim, &action(), self.vault.clone(), self.testnet).unwrap();
        let mut url = recipe.url().to_owned();
        if self.wrong_url {
            url.push_str("?altered=1");
        }
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(authority.clone()),
            })
            .unwrap();
        let (plan, slot) = cx
            .admit_serial_plan(
                &preflight,
                SerialCall {
                    auth: CallAuth::Preflight,
                    pacing: None,
                    operation: Operation::Mutation,
                    timeout_ms: 20_000,
                    attempts: 1,
                },
                SerialPolicy {
                    credential: Some(authority),
                    pacing: None,
                    refusals_complete: false,
                },
            )
            .unwrap();
        let context = cx.plan_leg_context(&plan, &slot).unwrap();
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            url,
            None,
            Box::new(recipe),
            context,
        )
        .unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn action() -> Schedule {
    Schedule {
        kind: "scheduleCancel",
        time: 100_000,
    }
}
fn engine(
    testnet: bool,
    vault: Option<String>,
    wrong_url: bool,
) -> (Engine<Probe>, Arc<LocalSigner>) {
    let policy: CredentialSetupPolicy = serde_json::from_str(r#"{"primary":{"source":"scalarOnly","signingAlgorithm":"secp256k1Digest","secretEncoding":"hex"}}"#).unwrap();
    let (signer, identities, _) = LocalSigner::from_json_with_facts(
        serde_json::json!({"primary":{"secret":KEY}}).to_string(),
        policy,
        false,
    )
    .unwrap();
    assert!(identities.primary.as_ref().unwrap().credential_absent());
    let signer = Arc::new(signer);
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        testnet,
        vault,
        wrong_url,
    });
    e.set_signer(Some(Arc::new(DelayedLocal(signer.clone()))))
        .unwrap();
    e.set_identities(identities).unwrap();
    e.initialize_address_nonce(100.).unwrap();
    assert!(e.address_nonce_setup_ready());
    e.start(100.);
    e.drain_actions().for_each(drop);
    (e, signer)
}
fn work(e: &mut Engine<Probe>) -> (RequestId, SignRequest) {
    e.handle(
        2000.,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTC-PERP".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (token, request) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("actual managed preparation");
    assert!(request.body.is_none());
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    e.http_prepared(
        2000.,
        token,
        Prepared {
            id: request.id.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("real delayed provider work")
}
fn answer(
    e: &mut Engine<Probe>,
    attempt: RequestId,
    result: SignResult,
) -> Option<(safe_cex_engine::request_context::Permit, HttpRequest)> {
    e.handle(
        2000.,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    e.drain_actions().find_map(|a| match a {
        Action::ManagedHttp(a) => match *a {
            HttpAction::Bind {
                permit, request, ..
            } => Some((permit, request)),
            _ => None,
        },
        Action::Http(_) => panic!("no legacy HTTP release"),
        _ => None,
    })
}
#[test]
fn provider_recipe_matches_native_signature_and_body_on_both_networks_and_vaults() {
    for testnet in [false, true] {
        for vault in [
            None,
            Some("0x1111111111111111111111111111111111111111".to_owned()),
        ] {
            let (mut e, signer) = engine(testnet, vault.clone(), false);
            let (attempt, work) = work(&mut e);
            assert_eq!(work.operations.len(), 2);
            assert_eq!(work.receipt.stamp_ms, Some(2000.));
            assert!(work.receipt.nonce.is_some());
            let result = signer.sign_now(&work).unwrap();
            let (permit, packet) = answer(&mut e, attempt, result).expect("verified body binds");
            let expected_signature = sign::sign(
                &action(),
                vault.as_deref(),
                work.receipt.nonce.unwrap(),
                !testnet,
                KEY,
            )
            .unwrap();
            let mut expected = serde_json::json!({"action":action(),"nonce":work.receipt.nonce.unwrap(),"signature":expected_signature});
            if let Some(vault) = vault {
                expected["vaultAddress"] = vault.into();
            }
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(packet.body.as_deref().unwrap()).unwrap(),
                expected
            );
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct NativePayload<'a, A: serde::Serialize> {
                action: &'a A,
                nonce: u64,
                signature: sign::Signature,
                #[serde(skip_serializing_if = "Option::is_none")]
                vault_address: Option<&'a str>,
            }
            let native_body = serde_json::to_string(&NativePayload {
                action: &action(),
                nonce: work.receipt.nonce.unwrap(),
                signature: sign::sign(
                    &action(),
                    expected["vaultAddress"].as_str(),
                    work.receipt.nonce.unwrap(),
                    !testnet,
                    KEY,
                )
                .unwrap(),
                vault_address: expected["vaultAddress"].as_str(),
            })
            .unwrap();
            assert_eq!(packet.body.as_deref(), Some(native_body.as_str()));
            assert!(matches!(
                e.claim_http(2000., permit),
                Ok(Claim::Released(_))
            ));
            assert!(e.claim_http(2000., permit).is_err());
        }
    }
}
#[test]
fn malformed_or_changed_provider_results_never_bind_a_body() {
    for case in 0..6 {
        let (mut e, signer) = engine(false, None, false);
        let (attempt, work) = work(&mut e);
        let mut result = signer.sign_now(&work).unwrap();
        match case {
            0 => result.receipt.nonce = result.receipt.nonce.map(|n| n + 1),
            1 => result.values[0].pop().map(drop).unwrap(),
            2 => result.values[0][0] ^= 1,
            3 => result.values[1].pop().map(drop).unwrap(),
            4 => result.headers.push(("X-Injected".into(), "bad".into())),
            5 => {
                let order: [u8; 32] = [
                    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                    0xff, 0xff, 0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2,
                    0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
                ];
                let signature = &mut result.values[1];
                let mut borrow = 0i16;
                for i in (0..32).rev() {
                    let value = i16::from(order[i]) - i16::from(signature[32 + i]) - borrow;
                    signature[32 + i] = value.rem_euclid(256) as u8;
                    borrow = i16::from(value < 0);
                }
                signature[64] = if signature[64] >= 27 {
                    27 + ((signature[64] - 27) ^ 1)
                } else {
                    signature[64] ^ 1
                };
            }
            _ => unreachable!(),
        }
        if case == 3 {
            assert_eq!(
                work.validate_result(&result),
                Err(safe_cex_engine::signing::SignError::InvalidResult)
            );
        }
        assert!(answer(&mut e, attempt, result).is_none(), "case {case}");
    }
}
#[test]
fn altered_outbound_url_refuses_before_provider_work() {
    let (mut e, _) = engine(false, None, true);
    e.handle(
        2000.,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTC-PERP".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let preparation = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        2000.,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Sign { .. } | Action::Http(_)) || matches!(a, Action::ManagedHttp(action) if matches!(*action, HttpAction::Bind { .. }))));
}
