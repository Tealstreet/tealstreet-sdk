//! Component vectors through the injected provider, not production adoption.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{
    Identity, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Command, Cx, Engine, Method, Response, SocketId, TimerId, Venue};
use safe_cex_venue_weex::dispatch_signing::{Http, Upgrade};

struct Probe {
    identity: Identity,
    method: Method,
    path: String,
    body: Option<String>,
    upgrade: bool,
    alter: u8,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if self.upgrade {
            let step = Upgrade::new(self.identity.clone());
            let mut spec = step.unsigned();
            if self.alter == 1 {
                spec.url.push_str("/wrong");
            }
            if self.alter == 3 {
                spec.headers.reverse();
            }
            if self.alter == 4 {
                spec.binary = true;
            }
            if self.alter == 5 {
                spec.channel = Some(safe_cex_engine::SocketChannel::Public);
            }
            cx.connect_signed(spec, Box::new(step));
        } else {
            let step = Http::new(
                self.identity.clone(),
                self.method,
                self.path.clone(),
                self.body.clone(),
            );
            let (mut url, mut headers, mut body) = step.unsigned();
            if self.alter == 1 {
                url.push_str("/wrong");
            }
            if self.alter == 2 {
                body = Some(String::new());
            }
            if self.alter == 3 {
                headers.reverse();
            }
            let method = if self.alter == 4 {
                Method::Post
            } else {
                self.method
            };
            let id = cx.request(method, url, headers, body, ());
            cx.sign_http(id, Box::new(step));
        }
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn setup() -> (Identity, Provider) {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("k".into()),
            secret: Secret::new("secret".into()),
            passphrase: Some(Secret::new("p".into())),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    (identities.primary.unwrap(), std::sync::Arc::new(provider))
}
fn run(method: Method, path: &str, body: Option<&str>, upgrade: bool, alter: u8) -> Vec<Action> {
    let (identity, provider) = setup();
    run_with(identity, provider, method, path, body, upgrade, alter)
}
#[allow(clippy::too_many_arguments)]
fn run_with(
    identity: Identity,
    provider: Provider,
    method: Method,
    path: &str,
    body: Option<&str>,
    upgrade: bool,
    alter: u8,
) -> Vec<Action> {
    let mut engine: Engine<Probe> = safe_cex_engine::test_support::seeded_engine(Probe {
        identity,
        method,
        path: path.into(),
        body: body.map(str::to_owned),
        upgrade,
        alter,
    });
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1234.9);
    engine.drain_actions().collect()
}
fn headers(expected_mac: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ACCESS-KEY", "k".into()),
        ("ACCESS-PASSPHRASE", "p".into()),
        ("ACCESS-TIMESTAMP", "1234".into()),
        ("ACCESS-SIGN", expected_mac.into()),
        ("Content-Type", "application/json".into()),
        ("User-Agent", "Tealstreet".into()),
    ]
}
#[test]
fn exact_get_and_post_vectors_use_provider_owned_passphrase_and_raw_mac() {
    for (method, path, body, mac) in [
        (
            Method::Get,
            "/capi/v3/openOrders?limit=100&page=0",
            None,
            "bCU4mL/nryWPHh2ijQKaUlRR81jLGrRazQ5ZFQPfR+E=",
        ),
        (
            Method::Post,
            "/capi/v3/order",
            Some(r#"{"symbol":"BTCUSDT"}"#),
            "LGImWmNEu6Q2o2LhDIPtwbUT0PBJLHsWBbY7mB0+0JY=",
        ),
    ] {
        let actions = run(method, path, body, false, 0);
        let requests = actions
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            requests.len(),
            1,
            "the provider must release one native request"
        );
        assert_eq!(
            requests[0].url,
            format!("https://api-contract.weex.com{path}")
        );
        assert_eq!(requests[0].body.as_deref(), body);
        assert_eq!(requests[0].headers, headers(mac));
    }
}
#[test]
fn exact_upgrade_vector_has_no_get_prefix_or_login_frame() {
    let actions = run(Method::Get, "", None, true, 0);
    let connects = actions
        .iter()
        .filter_map(|action| match action {
            Action::WsConnect {
                url,
                headers,
                channel,
                ..
            } => Some((url, headers, channel)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(connects.len(), 1, "signed upgrade must release once");
    assert_eq!(connects[0].0, "wss://ws-contract.weex.com/v3/ws/private");
    assert_eq!(
        connects[0].1,
        &headers("8IL8D6LBPFWJeTejIF/swS90xLfKkMPm++NyP0LDezA=")
    );
    assert_eq!(
        *connects[0].2,
        Some(safe_cex_engine::SocketChannel::Private)
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::WsSend { .. }))
    );
}
#[test]
fn altered_url_never_releases_http_or_an_unauthenticated_upgrade() {
    for upgrade in [false, true] {
        assert!(
            !run(Method::Get, "/capi/v3/openOrders", None, upgrade, 1)
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::WsConnect { .. }))
        );
    }
}

#[test]
fn method_body_header_and_upgrade_role_tampering_is_not_sent() {
    for (upgrade, alterations) in [(false, &[2, 3, 4][..]), (true, &[3, 4, 5][..])] {
        for &alter in alterations {
            let actions = run(Method::Get, "/capi/v3/openOrders", None, upgrade, alter);
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_) | Action::WsConnect { .. }))
            );
        }
    }
}

struct Malformed {
    local: Provider,
    fault: u8,
}
impl Signer for Malformed {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(async { Err(SignError::Unavailable) })
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.fault == 11 {
            return Some(Err(SignError::Refused));
        }
        let mut result = self.local.sign_ready(request)?.ok()?;
        match self.fault {
            0 => {
                result.values.pop();
            }
            1 => result.values[1].pop().map(|_| ()).unwrap(),
            2 => result.values[0].clear(),
            3 => result.values[2].clear(),
            4 => result.values[0] = vec![0xff],
            5 => result.values[2] = b"bad\r\nheader".to_vec(),
            6 => result.values[0] = vec![b'x'; 8193],
            7 => result.receipt.stamp_ms = Some(1235.),
            8 => result.receipt.expiry_ms = Some(2000.),
            9 => result.headers.push(("X-Extra".into(), "untrusted".into())),
            10 => result.receipt.stamp_ms = Some(1234.5),
            _ => unreachable!(),
        }
        Some(Ok(result))
    }
}
#[test]
fn malformed_provider_output_and_refusal_never_release_either_transport() {
    for upgrade in [false, true] {
        for fault in 0..=11 {
            let (identity, local) = setup();
            let actions = run_with(
                identity,
                std::sync::Arc::new(Malformed { local, fault }),
                Method::Get,
                "/capi/v3/openOrders",
                None,
                upgrade,
                0,
            );
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_) | Action::WsConnect { .. })),
                "fault {fault}, upgrade {upgrade}"
            );
        }
    }
}
#[test]
fn missing_passphrase_ref_never_asks_the_provider_or_releases() {
    for upgrade in [false, true] {
        let (mut identity, _) = setup();
        let provider: Provider = std::sync::Arc::new(NeverSigner);
        identity.passphrase = None;
        let actions = run_with(
            identity,
            provider,
            Method::Get,
            "/capi/v3/openOrders",
            None,
            upgrade,
            0,
        );
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Sign { .. } | Action::Http(_) | Action::WsConnect { .. }
        )));
    }
}
#[test]
fn explicitly_empty_body_remains_present_on_wire() {
    let requests = run(
        Method::Get,
        "/capi/v3/openOrders?limit=100&page=0",
        Some(""),
        false,
        0,
    );
    let http = requests
        .iter()
        .find_map(|action| {
            if let Action::Http(http) = action {
                Some(http)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(http.body.as_deref(), Some(""));
    assert_eq!(
        http.headers,
        headers("bCU4mL/nryWPHh2ijQKaUlRR81jLGrRazQ5ZFQPfR+E=")
    );
}

struct NeverSigner;
impl Signer for NeverSigner {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("invalid intent must not reach provider")
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        panic!("invalid intent must not reach provider")
    }
}
#[test]
fn non_native_authority_fragment_or_transport_normalization_never_reaches_provider() {
    for path in [
        ".example.invalid/capture",
        "",
        "?query=1",
        "/path#fragment",
        "/path\\other",
        "/line\r\n",
        "/space here",
        "/café",
        "/capi/v3/../account",
        "/capi/v3/%2e%2e/account",
        "/capi/v3/account<bad>",
        "/capi/v3/account?value=\"bad\"",
    ] {
        let (identity, _) = setup();
        let actions = run_with(
            identity,
            std::sync::Arc::new(NeverSigner),
            Method::Get,
            path,
            None,
            false,
            0,
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Sign { .. } | Action::Http(_))),
            "path {path:?}"
        );
    }
}

struct ManagedProbe(Identity);
impl Venue for ManagedProbe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        use safe_cex_engine::request_context::{Credential, Operation, Policy, RetryPolicy, Scope};
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: Some(Credential {
                        slot: safe_cex_engine::CredentialSlot::Primary,
                        reference: self.0.credential.clone(),
                    }),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let recipe = Http::new(
            self.0.clone(),
            Method::Get,
            "/capi/v3/openOrders?limit=100&page=0".into(),
            None,
        );
        let (url, _, body) = recipe.unsigned();
        assert!(
            cx.request_signed_managed(context, None, Method::Get, url, body, Box::new(recipe), ())
                .is_ok()
        );
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn managed_component_preserves_declared_public_headers_then_exact_native_vector() {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    let (identity, provider) = setup();
    let mut engine = safe_cex_engine::test_support::seeded_engine(ManagedProbe(identity));
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1234.);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    let prepares = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(prepares.len(), 1);
    assert_eq!(
        prepares[0].1.headers,
        vec![
            ("Content-Type", "application/json".into()),
            ("User-Agent", "Tealstreet".into())
        ],
        "managed preparation must snapshot the actual WEEX public declaration"
    );
    assert!(prepares[0].1.signer_headers.is_empty());
    engine.http_prepared(
        1234.9,
        prepares[0].0,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let binds = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((*permit, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        binds.len(),
        1,
        "real managed route signs and binds once through the provider"
    );
    assert_eq!(
        binds[0].1.headers,
        headers("bCU4mL/nryWPHh2ijQKaUlRR81jLGrRazQ5ZFQPfR+E=")
    );
    assert!(matches!(
        engine.claim_http(1234.9, binds[0].0),
        Ok(Claim::Released(_))
    ));
}
