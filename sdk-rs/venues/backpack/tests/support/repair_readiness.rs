use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{Algorithm, CredentialSetupPolicy, IdentitySetupPolicy};
use safe_cex_engine::{Action, Command, Event, Venue};
use std::sync::Arc;

struct Observe(Backpack);
impl Venue for Observe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.store.set_markets(vec![Market {
            id: "SOL_USDC_PERP".into(),
            symbol: "SOL_USDC_PERP".into(),
            pretty_symbol: None,
            base: "SOL".into(),
            quote: "USDC".into(),
            active: true,
            precision: Precision {
                amount: 0.01,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.01,
                    max: 1000.,
                },
                leverage: MinMax { min: 1., max: 20. },
                price: None,
            },
            contract_size: Some(1.),
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        assert!(self.0.account.credentials.is_none());
        self.0.orders_read(cx, Next::None);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, text: &str) {
        self.0.on_item(cx, p, text);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.0.on_response(cx, p, r);
        assert!(self.0.repairs_is_empty(), "unready Primary must not mint");
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, e: &str) {
        panic!("unexpected read failure: {e}");
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn accepted_secondary_snapshot_cannot_mint_with_installed_but_native_unready_primary() {
    let seed = |name: &str, n: u8| LocalIdentity {
        key: Secret::new(name.into()),
        secret: Secret::new(STANDARD.encode([n; 32])),
        secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
        ..Default::default()
    };
    let policy = IdentitySetupPolicy {
        signing_algorithm: Some(Algorithm::Ed25519),
        ..Default::default()
    };
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(seed("installed-primary", 7)),
            secondary: Some(seed("installed-read", 9)),
        },
        CredentialSetupPolicy {
            primary: Some(policy),
            secondary: Some(policy),
        },
    )
    .unwrap();
    let venue = Backpack::new(crate::Options {
        identity: identities.primary.clone(),
        read_identity: identities.secondary.clone(),
        read_credentials_status: Some(Ok(())),
        ..Default::default()
    });
    let mut e = safe_cex_engine::test_support::seeded_engine(Observe(venue));
    e.set_signer(Some(Arc::new(signer))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(1000.);
    let work: Vec<_> = e.drain_actions().collect();
    let timer = work
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .expect("actual account read deadline");
    let (issued, r) = work
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                safe_cex_engine::request_context::HttpAction::Prepare {
                    preparation,
                    request: outbound,
                    ..
                } => Some((preparation, outbound)),
                _ => None,
            },
            _ => None,
        })
        .expect("actual managed account Read");
    e.http_prepared(
        1000.,
        issued,
        safe_cex_engine::request_context::Prepared {
            id: r.id.0,
            expires_at: None,
        },
    );
    let (permit, r) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                safe_cex_engine::request_context::HttpAction::Bind {
                    preparation,
                    permit,
                    request: outbound,
                } if preparation == issued => Some((permit, outbound)),
                _ => None,
            },
            _ => None,
        })
        .expect("exact genuine account Bind");
    assert!(matches!(
        e.claim_http(1000., permit),
        Ok(safe_cex_engine::request_context::Claim::Released(_))
    ));
    assert!(r.url.ends_with("/orders"));
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "X-API-Key" && v == "installed-read")
    );
    e.handle(1000.,Event::HttpResponse{request:r.id,response:Response{status:200,headers:"",body:r#"[{"id":"123","symbol":"SOL_USDC_PERP","side":"Bid","orderType":"Market","price":"140","quantity":"5","executedQuantity":"0","triggerPrice":"130","triggerQuantity":"5","status":"New","createdAt":1000}]"#}});
    assert_eq!(
        e.store().orders().len(),
        1,
        "genuine accepted Secondary DATA publication"
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(n)) if n.message=="Backpack automatic trigger repair awaits managed account signing")).count(),1);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::ClearTimer { timer: t } if *t == timer))
            .count(),
        1
    );
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), safe_cex_engine::request_context::HttpAction::Cancel { preparation, .. } if *preparation == issued))).count(), 1);
    assert!(!actions.iter().any(|a| match a {
            Action::Http(_) => true,
            Action::ManagedHttp(a) => !matches!(a.as_ref(), safe_cex_engine::request_context::HttpAction::Cancel { preparation, .. } if *preparation == issued),
            Action::ClearTimer { timer: t } => *t != timer,
            _ => false,
        }));
}
