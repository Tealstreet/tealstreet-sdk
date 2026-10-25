//! Original history wire controls and caller-bound assertion REDs.
mod support;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Query, Response};
use support::{Driver, SYMBOL, ready};

#[derive(Clone, Copy)]
enum Route {
    Fills,
    Orders,
    Funding,
    Trades,
}
impl Route {
    fn query(self, limit: u32) -> Query {
        match self {
            Self::Fills => Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(limit),
            },
            Self::Orders => Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: Some(limit),
            },
            Self::Funding => Query::Income {
                symbol: None,
                kind: Some("FUNDING".into()),
                from_ms: None,
                to_ms: None,
                limit: Some(limit),
            },
            Self::Trades => Query::PublicTrades {
                symbol: SYMBOL.into(),
                limit: Some(limit),
            },
        }
    }
    fn path(self) -> &'static str {
        match self {
            Self::Fills => "/wapi/v1/history/fills",
            Self::Orders => "/wapi/v1/history/orders",
            Self::Funding => "/wapi/v1/history/funding",
            Self::Trades => "/api/v1/trades",
        }
    }
    fn instruction(self) -> &'static str {
        match self {
            Self::Fills => "fillHistoryQueryAll",
            Self::Orders => "orderHistoryQueryAll",
            Self::Funding => "fundingHistoryQueryAll",
            Self::Trades => panic!("public trades have no signing recipe"),
        }
    }
}
const ROUTES: [Route; 4] = [Route::Fills, Route::Orders, Route::Funding, Route::Trades];
fn physical(e: &mut Driver, now: f64) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            Action::Notice(Notice::CommandFailed { error, .. }) => {
                panic!("unexpected history refusal: {error:?}")
            }
            _ => None,
        })
        .collect()
}
fn ask(e: &mut Driver, route: Route, wait: Option<u32>, limit: u32) -> HttpRequest {
    e.handle_command(
        2000.,
        Command::Query {
            command: 71,
            query: route.query(limit),
        },
        wait,
    )
    .unwrap();
    let mut requests = physical(e, 2000.);
    assert_eq!(requests.len(), 1, "one genuine native history request");
    let r = requests.pop().unwrap();
    assert_eq!(r.method, Method::Get);
    assert!(
        r.url
            .starts_with(&format!("https://api.backpack.exchange{}?", route.path()))
    );
    assert_eq!(r.body, None);
    assert_eq!(r.route, None);
    r
}
fn respond(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn done(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 71,
                result,
            }) => Some(result.as_str()),
            _ => None,
        })
        .collect()
}
fn transport_bound(route: Route) {
    let (mut e, _) = ready();
    let r = ask(&mut e, route, Some(37), 2);
    assert!(
        r.timeout_ms.is_some_and(|ms| ms <= 37),
        "physical history must retain the original37ms receipt: {:?}",
        r.timeout_ms
    );
}
fn expired_data(route: Route, now: f64) {
    let (mut e, _) = ready();
    let r = ask(&mut e, route, Some(37), 2);
    let actions = respond(&mut e, &r, now, "[]");
    assert!(
        done(&actions).is_empty(),
        "expired history DATA cannot publish final success"
    );
    assert!(
        !actions.iter().any(|a| match a {
            Action::Http(_) => true,
            Action::ManagedHttp(action) => matches!(
                **action,
                safe_cex_engine::request_context::HttpAction::Prepare { .. }
                    | safe_cex_engine::request_context::HttpAction::Bind { .. }
            ),
            _ => false,
        }),
        "no raw or managed continuation after root expiry"
    );
}
macro_rules! receipt_cases {
    ($timeout:ident, $equal:ident, $late:ident, $route:expr) => {
        #[test]
        fn $timeout() {
            transport_bound($route);
        }
        #[test]
        fn $equal() {
            expired_data($route, 2037.);
        }
        #[test]
        fn $late() {
            expired_data($route, 2038.);
        }
    };
}
receipt_cases!(
    fills_transport_keeps_short_receipt,
    fills_equality_data_is_rejected,
    fills_late_data_is_rejected,
    Route::Fills
);
receipt_cases!(
    orders_transport_keeps_short_receipt,
    orders_equality_data_is_rejected,
    orders_late_data_is_rejected,
    Route::Orders
);
receipt_cases!(
    funding_transport_keeps_short_receipt,
    funding_equality_data_is_rejected,
    funding_late_data_is_rejected,
    Route::Funding
);
receipt_cases!(
    trades_transport_keeps_short_receipt,
    trades_equality_data_is_rejected,
    trades_late_data_is_rejected,
    Route::Trades
);

#[test]
fn native_all_four_routes_complete_before_original_expiry() {
    for route in ROUTES {
        let (mut e, _) = ready();
        let r = ask(&mut e, route, Some(37), 2);
        let before = e.account_snapshot_frame(2036.).to_vec();
        let actions = respond(&mut e, &r, 2036., "[]");
        assert_eq!(done(&actions), ["[]"]);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
        assert_eq!(e.account_snapshot_frame(2036.), before);
    }
}
fn full_filtered_page() -> &'static str {
    r#"[{"orderId":"123","symbol":"UNLISTED_PERP","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":9007199254740993,"timestamp":"2025-12-15T13:28:00Z"}]"#
}
fn second_page(e: &mut Driver) -> HttpRequest {
    let first = ask(e, Route::Fills, Some(37), 1);
    e.handle(
        2020.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 200,
                headers: "",
                body: full_filtered_page(),
            },
        },
    );
    let mut requests = physical(e, 2020.);
    assert_eq!(
        requests.len(),
        1,
        "filtered full page still advances native offset"
    );
    let next = requests.pop().unwrap();
    assert!(next.url.contains("offset=1"));
    next
}
#[test]
fn second_page_keeps_remaining_original_receipt() {
    let (mut e, _) = ready();
    let next = second_page(&mut e);
    assert!(
        next.timeout_ms.is_some_and(|ms| ms <= 17),
        "page2 cannot renew the37ms caller wait: {:?}",
        next.timeout_ms
    );
}
#[test]
fn second_page_cannot_finish_beyond_original_expiry() {
    let (mut e, _) = ready();
    let next = second_page(&mut e);
    let actions = respond(&mut e, &next, 2038., "[]");
    assert!(
        done(&actions).is_empty(),
        "page2 success still requires the original caller root"
    );
    assert!(
        !actions.iter().any(|action| match action {
            Action::Http(_) => true,
            Action::ManagedHttp(action) => matches!(
                **action,
                safe_cex_engine::request_context::HttpAction::Prepare { .. }
                    | safe_cex_engine::request_context::HttpAction::Bind { .. }
            ),
            _ => false,
        }),
        "page2 expiry forbids raw and managed continuation"
    );
}
#[test]
fn native_second_page_completes_before_original_expiry() {
    let (mut e, _) = ready();
    let next = second_page(&mut e);
    let actions = respond(&mut e, &next, 2036., "[]");
    assert_eq!(done(&actions), ["[]"]);
}
#[test]
fn native_private_wire_keeps_primary_signature_and_window() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let seed = STANDARD.encode([7u8; 32]);
    let native =
        safe_cex_venue_backpack::sign::Credentials::new("test-public-key".into(), &seed).unwrap();
    for route in [Route::Fills, Route::Orders, Route::Funding] {
        let (mut e, _) = ready();
        let r = ask(&mut e, route, Some(37), 2);
        assert_eq!(
            r.url,
            format!(
                "https://api.backpack.exchange{}?limit=2&sortDirection=Desc",
                route.path()
            )
        );
        let params = [
            ("limit".into(), "2".into()),
            ("sortDirection".into(), "Desc".into()),
        ]
        .into();
        assert_eq!(
            r.headers,
            native.headers(route.instruction(), &[params], 2000.)
        );
        assert_eq!(done(&respond(&mut e, &r, 2036., "[]")), ["[]"]);
    }
}
#[test]
fn native_public_trade_high_precision_id_and_economics_remain_exact() {
    let (mut e, _) = ready();
    let r = ask(&mut e, Route::Trades, Some(37), 2);
    assert_eq!(r.headers, [("X-Broker-Id", "1200".to_string())]);
    let body = r#"[{"id":9007199254740993,"isBuyerMaker":true,"price":"104.12","quantity":"0.1","quoteQuantity":"10.412","timestamp":1792425943313}]"#;
    let actions = respond(&mut e, &r, 2036., body);
    let output = done(&actions);
    assert_eq!(output.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(output[0]).unwrap();
    assert_eq!(rows[0]["id"], "9007199254740993");
    assert_eq!(rows[0]["price"], 104.12);
    assert_eq!(rows[0]["qty"], 0.1);
    assert_eq!(rows[0]["quoteQty"], 10.412);
    assert_eq!(rows[0]["side"], "sell");
}

#[test]
fn default_wait_physical_history_has_declared_twenty_second_page_cap() {
    for route in ROUTES {
        let (mut e, _) = ready();
        let r = ask(&mut e, route, None, 2);
        assert!(
            r.timeout_ms.is_some_and(|ms| ms <= 20_000),
            "NEW per-physical-page cap, clipped to original caller root"
        );
    }
}

#[test]
fn native_public_only_saved_read_key_preserves_three_private_routes() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::ReadCredentials;
    use safe_cex_venue_backpack::{Backpack, Options};
    let seed = STANDARD.encode([9u8; 32]);
    for route in [Route::Fills, Route::Orders, Route::Funding] {
        let (provider, identities) = LocalSigner::from_setup_with_policy(
            LocalSetup {
                primary: None,
                secondary: Some(LocalIdentity {
                    key: Secret::new("saved-read-key".into()),
                    secret: Secret::new(seed.clone()),
                    secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
                    ..Default::default()
                }),
            },
            ed25519_seeds(),
        )
        .unwrap();
        let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
            public_data_only: true,
            read_identity: identities.secondary.clone(),
            read_credentials: Some(ReadCredentials {
                key: "saved-read-key".into(),
                secret: seed.clone(),
                passphrase: None,
            }),
            ..Default::default()
        }));
        e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
        e.set_identities(identities).unwrap();
        let r = ask(&mut e, route, Some(37), 2);
        let native =
            safe_cex_venue_backpack::sign::Credentials::new("saved-read-key".into(), &seed)
                .unwrap();
        let params = [
            ("limit".into(), "2".into()),
            ("sortDirection".into(), "Desc".into()),
        ]
        .into();
        assert_eq!(
            r.headers,
            native.headers(route.instruction(), &[params], 2000.)
        );
        assert_eq!(done(&respond(&mut e, &r, 2036., "[]")), ["[]"]);
    }
}

#[test]
fn native_page_two_failure_retains_only_accepted_page_one() {
    let (mut e, _) = ready();
    let first = ask(&mut e, Route::Fills, Some(37), 2);
    let accepted = full_filtered_page().replace("UNLISTED_PERP", SYMBOL);
    let page = format!(
        "[{},{}]",
        &accepted[1..accepted.len() - 1],
        &full_filtered_page()[1..full_filtered_page().len() - 1]
    );
    e.handle(
        2010.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 200,
                headers: "",
                body: &page,
            },
        },
    );
    let mut next = physical(&mut e, 2010.);
    assert_eq!(next.len(), 1);
    let next = next.pop().unwrap();
    assert!(next.url.contains("offset=2"));
    e.handle(
        2011.,
        Event::HttpStart {
            request: next.id,
            status: 200,
            headers: "",
        },
    );
    let unaccepted = accepted.replace("9007199254740993", "9007199254740995");
    let prefix = format!("{},", &unaccepted[..unaccepted.len() - 1]);
    e.handle(
        2011.,
        Event::HttpChunk {
            request: next.id,
            data: prefix.as_bytes(),
        },
    );
    e.handle(
        2012.,
        Event::HttpFailed {
            request: next.id,
            error: "page2 fixture transport loss",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let result = done(&actions);
    assert_eq!(result.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(result[0]).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "9007199254740993");
    assert!(!result[0].contains("9007199254740995"));
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "history.incomplete")).count(), 1);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
}

// Source-stage gate: the raw helper must not satisfy selected-provider custody.
fn provider_read_identity_owns_history_signature(route: Route) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::ReadCredentials;
    use safe_cex_venue_backpack::{Backpack, Options};
    let selected_seed = STANDARD.encode([9u8; 32]);
    let raw_seed = STANDARD.encode([8u8; 32]);
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: None,
            secondary: Some(LocalIdentity {
                key: Secret::new("provider-read-key".into()),
                secret: Secret::new(selected_seed.clone()),
                secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
                ..Default::default()
            }),
        },
        ed25519_seeds(),
    )
    .unwrap();
    assert!(identities.primary.is_none());
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        read_identity: identities.secondary.clone(),
        read_credentials_status: Some(Ok(())),
        read_credentials: Some(ReadCredentials {
            key: "different-raw-key".into(),
            secret: raw_seed.clone(),
            passphrase: None,
        }),
        ..Default::default()
    }));
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    let request = ask(&mut e, route, Some(37), 2);
    let params = [
        ("limit".into(), "2".into()),
        ("sortDirection".into(), "Desc".into()),
    ]
    .into();
    let selected =
        safe_cex_venue_backpack::sign::Credentials::new("provider-read-key".into(), &selected_seed)
            .unwrap();
    let raw =
        safe_cex_venue_backpack::sign::Credentials::new("different-raw-key".into(), &raw_seed)
            .unwrap();
    let expected = selected.headers(route.instruction(), &[params], 2000.);
    let raw_params = [
        ("limit".into(), "2".into()),
        ("sortDirection".into(), "Desc".into()),
    ]
    .into();
    assert_ne!(
        expected,
        raw.headers(route.instruction(), &[raw_params], 2000.),
        "working oracle distinguishes the two real signing identities"
    );
    assert_eq!(
        request.headers, expected,
        "history must sign with the selected provider identity, never raw read credentials"
    );
    assert_eq!(done(&respond(&mut e, &request, 2036., "[]")), ["[]"]);
}
#[test]
fn selected_provider_fills_signature_overrides_different_raw_read_material() {
    provider_read_identity_owns_history_signature(Route::Fills);
}
#[test]
fn selected_provider_orders_signature_overrides_different_raw_read_material() {
    provider_read_identity_owns_history_signature(Route::Orders);
}
#[test]
fn selected_provider_funding_signature_overrides_different_raw_read_material() {
    provider_read_identity_owns_history_signature(Route::Funding);
}

fn before_primary_account_start(
    discovery_held: bool,
    secondary: bool,
    public_only: bool,
) -> Driver {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::ReadCredentials;
    use safe_cex_venue_backpack::{Backpack, Options};
    let primary_seed = STANDARD.encode([7u8; 32]);
    let read_seed = STANDARD.encode([9u8; 32]);
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: (!public_only).then(|| LocalIdentity {
                key: Secret::new("primary-before-start".into()),
                secret: Secret::new(primary_seed.clone()),
                secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
                ..Default::default()
            }),
            secondary: secondary.then(|| LocalIdentity {
                key: Secret::new("secondary-before-start".into()),
                secret: Secret::new(read_seed.clone()),
                secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
                ..Default::default()
            }),
        },
        ed25519_seeds(),
    )
    .unwrap();
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: public_only,
        key: Some("primary-before-start".into()),
        secret: Some(primary_seed),
        identity: identities.primary.clone(),
        read_identity: identities.secondary.clone(),
        read_credentials_status: secondary.then_some(Ok(())),
        read_credentials: secondary.then(|| ReadCredentials {
            key: "secondary-before-start".into(),
            secret: read_seed,
            passphrase: None,
        }),
        recv_window: Some(5000.),
    }));
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    if discovery_held {
        e.start(1000.);
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Http(request) if request.url == "https://api.backpack.exchange/api/v1/markets")).count(), 1, "genuine catalogue is held without delivering discovery");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Sign { .. } | Action::SignedOutbound { .. }))
        );
    }
    e
}
fn native_primary_unavailable(route: Route, discovery_held: bool) {
    let mut e = before_primary_account_start(discovery_held, false, false);
    let before = e.account_snapshot_frame(2000.).to_vec();
    e.handle_command(
        2000.,
        Command::Query {
            command: 71,
            query: route.query(2),
        },
        Some(37),
    )
    .unwrap();
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 2000.);
    assert!(
        actions.iter().all(|a| matches!(a, Action::Notice(_))),
        "installed primary identity cannot bypass native credential readiness"
    );
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 71, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].message, "Backpack credentials unavailable");
    assert_eq!(failures[0].kind, None);
    assert_eq!(failures[0].outcome, None);
    assert!(done(&actions).is_empty());
    assert_eq!(e.account_snapshot_frame(2000.), before);
}
macro_rules! native_primary_startup_cases {
    ($pre:ident, $held:ident, $route:expr) => {
        #[test]
        fn $pre() {
            native_primary_unavailable($route, false);
        }
        #[test]
        fn $held() {
            native_primary_unavailable($route, true);
        }
    };
}
native_primary_startup_cases!(
    native_primary_only_fills_before_start_refuses,
    native_primary_only_fills_held_discovery_refuses,
    Route::Fills
);
native_primary_startup_cases!(
    native_primary_only_orders_before_start_refuses,
    native_primary_only_orders_held_discovery_refuses,
    Route::Orders
);
native_primary_startup_cases!(
    native_primary_only_funding_before_start_refuses,
    native_primary_only_funding_held_discovery_refuses,
    Route::Funding
);

#[test]
fn native_secondary_history_works_before_start_and_with_held_discovery() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let seed = STANDARD.encode([9u8; 32]);
    let native =
        safe_cex_venue_backpack::sign::Credentials::new("secondary-before-start".into(), &seed)
            .unwrap();
    for public_only in [false, true] {
        for held in [false, true] {
            for route in [Route::Fills, Route::Orders, Route::Funding] {
                let mut e = before_primary_account_start(held, true, public_only);
                let request = ask(&mut e, route, Some(37), 2);
                let params = [
                    ("limit".into(), "2".into()),
                    ("sortDirection".into(), "Desc".into()),
                ]
                .into();
                assert_eq!(
                    request.headers,
                    native.headers(route.instruction(), &[params], 2000.),
                    "secondary stays available; pre-account native window is60000, not uninitialized recv_window5000"
                );
                assert_eq!(done(&respond(&mut e, &request, 2036., "[]")), ["[]"]);
            }
        }
    }
}

/// Backpack seeds are Ed25519 keys, declared as the provider builder does.
fn ed25519_seeds() -> safe_cex_engine::signing::CredentialSetupPolicy {
    let ed25519 = Some(safe_cex_engine::signing::IdentitySetupPolicy {
        signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Ed25519),
        ..Default::default()
    });
    safe_cex_engine::signing::CredentialSetupPolicy {
        primary: ed25519,
        secondary: ed25519,
    }
}
