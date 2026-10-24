//! Original callback controls for the complete private dispatch cutover.
mod support;
use safe_cex_engine::signing::{
    Capability, KeyRef, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Query,
    ReadCredentials, Response, VenueError,
};
use safe_cex_venue_cryptocom::{Cryptocom, Options, sign::Params};
use std::sync::Arc;
use support::Harness;

const NOW: f64 = 1_710_000_000_000.;
const OWNER: u32 = 700;
const IDENTITY: &str = r#"{"code":0,"result":{"this_account":{"uuid":9007199254740993}}}"#;
type TestEngine = Engine<Cryptocom>;

struct Parked;
impl Signer for Parked {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
    fn can_sign(&self, _: &KeyRef, _: safe_cex_engine::signing::Algorithm) -> Capability {
        Capability::Supported
    }
}
struct Refused;
impl Signer for Refused {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(async { Err(SignError::Refused) })
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Err(SignError::Refused))
    }
    fn can_sign(&self, _: &KeyRef, _: safe_cex_engine::signing::Algorithm) -> Capability {
        Capability::Supported
    }
}
fn options() -> Options {
    Options {
        key: Some("fixture-primary-key".into()),
        secret: Some("fixture-primary-material".into()),
        ..Default::default()
    }
}
fn engine(opts: Options) -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(Cryptocom::new(opts))
}
fn actions(e: &mut TestEngine, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn ask(e: &mut TestEngine, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::FetchAccount { command: OWNER }),
    );
    actions(e, now)
}
fn http(a: &[Action], path: &str) -> HttpRequest {
    a.iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .expect("native physical request")
}
fn terminals(a: &[Action]) -> Vec<Result<String, VenueError>> {
    a.iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: OWNER,
                result,
            }) => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(Err(error.clone())),
            _ => None,
        })
        .collect()
}
fn respond(e: &mut TestEngine, now: f64, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    actions(e, now)
}
fn no_private_http(a: &[Action]) {
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/private/"))),
        "opaque provider custody must precede private HTTP release"
    );
}
#[test]
fn identity_waits_for_the_injected_provider() {
    let mut e = engine(options());
    e.set_signer(Some(Arc::new(Parked))).unwrap();
    let a = ask(&mut e, NOW);
    no_private_http(&a);
    assert!(a.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert!(terminals(&a).is_empty());
}
#[test]
fn identity_without_a_provider_cannot_use_raw_secret_fallback() {
    let mut e = engine(options());
    e.set_signer(None).unwrap();
    let a = ask(&mut e, NOW);
    no_private_http(&a);
    let t = terminals(&a);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].as_ref().unwrap_err().outcome, Some(Outcome::NotSent));
}
#[test]
fn identity_provider_refusal_is_one_not_sent_terminal() {
    let mut e = engine(options());
    e.set_signer(Some(Arc::new(Refused))).unwrap();
    let a = ask(&mut e, NOW);
    no_private_http(&a);
    let t = terminals(&a);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].as_ref().unwrap_err().outcome, Some(Outcome::NotSent));
}
#[test]
fn identity_caller_deadline_equality_denies_success_data() {
    let mut e = engine(options());
    e.handle_command(NOW, Command::FetchAccount { command: OWNER }, Some(50))
        .unwrap();
    let r = http(&actions(&mut e, NOW), "private/get-accounts");
    let a = respond(&mut e, NOW + 50., &r, 200, IDENTITY);
    let t = terminals(&a);
    assert_eq!(t.len(), 1);
    let error = t[0]
        .as_ref()
        .expect_err("root equality cannot publish identity");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    assert!(respond(&mut e, NOW + 51., &r, 200, IDENTITY).is_empty());
}
#[test]
fn duplicate_identity_entry_cannot_send_a_second_request() {
    let mut e = engine(options());
    let first = http(&ask(&mut e, NOW), "private/get-accounts");
    let a = ask(&mut e, NOW + 1.);
    no_private_http(&a);
    assert!(terminals(&a).is_empty());
    assert_eq!(
        terminals(&respond(&mut e, NOW + 2., &first, 200, IDENTITY)).len(),
        1
    );
}
#[test]
fn identity_capacity_refusal_preserves_all_live_public_peers() {
    let mut e = engine(options());
    let mut peers = vec![];
    for command in 1..=256 {
        e.handle(
            NOW,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: "BTCUSD-PERP".into(),
                limit: None,
            }),
        );
        let preparation = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(action) => match *action {
                    safe_cex_engine::request_context::HttpAction::Prepare {
                        preparation, ..
                    } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine public peer preparation");
        peers.push(preparation);
    }
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    let a = ask(&mut e, NOW);
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    no_private_http(&a);
    let t = terminals(&a);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].as_ref().unwrap_err().kind, Some(ErrorKind::Internal));
}
#[test]
fn startup_private_snapshot_waits_for_provider_after_public_catalogue() {
    let mut e = engine(options());
    e.set_signer(Some(Arc::new(Parked))).unwrap();
    e.start(NOW);
    let r = http(&actions(&mut e, NOW), "public/get-instruments");
    let a = respond(&mut e, NOW, &r, 200, support::INSTRUMENTS);
    no_private_http(&a);
    assert!(a.iter().any(|a| matches!(a, Action::Sign { .. })));
}
#[test]
fn private_ws_auth_waits_for_provider_at_native_ready_boundary() {
    let mut e = engine(options());
    e.set_signer(Some(Arc::new(Parked))).unwrap();
    let mut h = Harness {
        engine: e,
        actions: vec![],
        failures: vec![],
        now: NOW,
    };
    h.engine.start(h.now);
    h.collect();
    h.reply("public/get-instruments", support::INSTRUMENTS);
    h.reply("public/get-tickers", support::TICKERS);
    let socket = h.socket(true);
    h.actions.clear();
    h.event(Event::WsOpened { socket });
    let (timer, at_ms) = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("native one-second private ready timer");
    h.actions.clear();
    h.now = at_ms;
    h.event(Event::TimerFired { timer });
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("public/auth"))),
        "raw credentials must not bypass parked WS provider"
    );
    assert!(h.actions.iter().any(|a| matches!(a, Action::Sign { .. })));
}
fn placement(id: &str) -> safe_cex_engine::model::PlaceOrderOpts {
    let mut order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.0021,"price":90.04}"#,
    )
    .unwrap();
    order.client_order_id = Some(id.into());
    order
}
fn pace(h: &mut Harness) {
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.))
        .unwrap();
    let Action::SetTimer { timer, at_ms } = h.actions.remove(i) else {
        unreachable!()
    };
    h.now = at_ms;
    h.event(Event::TimerFired { timer });
}
#[test]
fn best_effort_all_failed_batch_cannot_acknowledge_success() {
    let mut h = Harness::new();
    h.load();
    h.actions.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: OWNER,
        orders: vec![placement("one")],
    }));
    h.reply(
        "private/create-order",
        r#"{"code":30014,"message":"invalid request"}"#,
    );
    pace(&mut h);
    let t = terminals(&h.actions);
    assert_eq!(t.len(), 1);
    assert!(t[0].is_err(), "no accepted leg means the batch failed");
}
#[test]
fn write_500_with_signature_code_cannot_resend() {
    let mut h = Harness::new();
    h.load();
    h.actions.clear();
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: OWNER,
        symbol: "BTCUSD-PERP".into(),
    }));
    let r = h.http("private/cancel-all-orders");
    h.actions.clear();
    h.event(Event::HttpResponse {
        request: r.id,
        response: Response {
            status: 500,
            headers: "",
            body: r#"{"code":40101,"message":"bad signature"}"#,
        },
    });
    no_private_http(&h.actions);
    pace(&mut h);
    let t = terminals(&h.actions);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].as_ref().unwrap_err().outcome, Some(Outcome::Unknown));
}
#[test]
fn accepted_id_survives_later_failed_leg_as_partial_failure() {
    let mut h = Harness::new();
    h.load();
    h.actions.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: OWNER,
        orders: vec![placement("one"), placement("two")],
    }));
    h.reply(
        "private/create-order",
        r#"{"code":0,"result":{"order_id":9007199254740993}}"#,
    );
    pace(&mut h);
    h.reply(
        "private/create-order",
        r#"{"code":30014,"message":"invalid request"}"#,
    );
    pace(&mut h);
    let t = terminals(&h.actions);
    assert_eq!(t.len(), 1);
    let error = t[0]
        .as_ref()
        .expect_err("failed legs must remain in the terminal");
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["9007199254740993"]
    );
}
#[test]
fn private_history_uses_original_caller_deadline() {
    let mut h = Harness::new();
    h.load();
    h.actions.clear();
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: OWNER,
                query: Query::Fills {
                    symbol: Some("BTCUSD-PERP".into()),
                    order_id: None,
                    from_ms: Some(h.now - 1000.),
                    to_ms: Some(h.now + 1000.),
                    limit: Some(1),
                },
            },
            Some(50),
        )
        .unwrap();
    h.collect();
    let r = h.http("private/get-trades");
    h.now += 50.;
    h.respond(&r, support::EMPTY);
    let t = terminals(&h.actions);
    assert_eq!(t.len(), 1);
    assert_eq!(
        t[0].as_ref()
            .expect_err("expired root cannot publish empty history")
            .kind,
        Some(ErrorKind::Timeout)
    );
}
fn secondary(valid: bool) -> ReadCredentials {
    ReadCredentials {
        key: "fixture-read-key".into(),
        secret: if valid { "fixture-read-material" } else { "" }.into(),
        passphrase: None,
    }
}
#[test]
fn posted_read_keeps_primary_identity_without_route_or_auth_evidence() {
    let mut opts = options();
    opts.read_credentials = Some(secondary(true));
    let mut e = engine(opts);
    let a = ask(&mut e, NOW);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::AccountAuth { .. })))
    );
    let r = http(&a, "private/get-accounts");
    assert_eq!(r.method, Method::Post);
    assert_eq!(r.route, None);
    let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["api_key"], "fixture-primary-key");
    let before = e.account_snapshot_frame(NOW).to_vec();
    let t = terminals(&respond(&mut e, NOW, &r, 200, IDENTITY));
    assert_eq!(
        t,
        [Ok(
            r#"{"userId":"9007199254740993","affiliateId":null}"#.into()
        )]
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert_eq!(e.health().auth_failures, 0);
}
#[test]
fn invalid_secondary_blocks_start_before_discovery() {
    let mut opts = options();
    opts.read_credentials = Some(secondary(false));
    let expected = opts
        .read_credentials
        .as_ref()
        .unwrap()
        .validate()
        .unwrap_err();
    let mut e = engine(opts);
    e.start(NOW);
    let a = actions(&mut e, NOW);
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
    assert_eq!(a.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::CONFIG_INVALID && event.message == expected)).count(), 1);
}
#[test]
fn invalid_secondary_identity_error_precedes_public_only_and_missing_primary() {
    for public_data_only in [false, true] {
        let read = secondary(false);
        let expected = read.validate().unwrap_err();
        let mut e = engine(Options {
            public_data_only,
            read_credentials: Some(read),
            ..Default::default()
        });
        let a = ask(&mut e, NOW);
        no_private_http(&a);
        let t = terminals(&a);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].as_ref().unwrap_err().message, expected);
    }
}
#[test]
fn native_rest_and_ws_hmac_match_independent_python_literals() {
    let params = Params::default()
        .strings("exec_inst", vec!["REDUCE_ONLY".into(), "POST_ONLY".into()])
        .text("instrument_name", "BTCUSD-PERP")
        .number("limit", u64::MAX);
    for (method, params, auth, expected) in [
        (
            "private/get-trades",
            params,
            false,
            "ad5455a0093aba156f0ad8f96ec18c599add648c4de5dbe3b2059ea1fbbc43aa",
        ),
        (
            "public/auth",
            Params::default(),
            true,
            "0e9d20117ab7df8097634d022b6241c57cc2eba952e8685cbb2240e43efa48fb",
        ),
    ] {
        let body: serde_json::Value =
            serde_json::from_str(&recipe_envelope(method, params, NOW, auth)).unwrap();
        assert_eq!(body["sig"], expected);
        assert_eq!(body["nonce"], NOW as u64);
        assert_eq!(body["id"], body["nonce"]);
        if auth {
            assert!(body.get("params").is_none());
        } else {
            assert_eq!(body["params"]["limit"].as_u64(), Some(u64::MAX));
        }
    }
}
#[test]
fn native_same_millisecond_reissues_keep_the_same_stamp_without_counter() {
    let first = recipe_envelope("private/get-accounts", Params::default(), NOW + 0.1, false);
    assert_eq!(
        first,
        recipe_envelope("private/get-accounts", Params::default(), NOW + 0.9, false)
    );
    let later: serde_json::Value = serde_json::from_str(&recipe_envelope(
        "private/get-accounts",
        Params::default(),
        NOW + 1.,
        false,
    ))
    .unwrap();
    assert_eq!(later["nonce"], NOW as u64 + 1);
}

fn recipe_envelope(method: &'static str, params: Params, now: f64, auth: bool) -> String {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Scope};
    use safe_cex_engine::test_support::ProbeVenue;
    use safe_cex_venue_cryptocom::{Pending, sign::Envelope};
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-primary-key".into()),
            secret: Secret::new("fixture-primary-material".into()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    let identity = identities.primary.unwrap();
    let venue = ProbeVenue::new(
        Cryptocom::new(Options::default()),
        move |_: &mut Cryptocom, cx: &mut safe_cex_engine::Cx<'_, _>| {
            if auth {
                let url = "wss://stream.crypto.com/exchange/v1/user";
                let socket = cx.connect(url);
                cx.send_signed(
                    socket,
                    String::new(),
                    Box::new(Envelope::ws_auth(identity.clone(), url)),
                );
            } else {
                let url = format!("https://api.crypto.com/exchange/v1/{method}");
                let context = cx
                    .http_context(
                        Scope::Background,
                        Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 20000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                // This oracle carrier exercises real recipe prepare/apply only.
                // Actual consumer custody is covered by the command controls above.
                let request = cx.request(
                    Method::Post,
                    url.clone(),
                    vec![("Content-Type", "application/json".into())],
                    None,
                    Pending::Tickers {
                        context,
                        rows: vec![],
                        skipped: Default::default(),
                    },
                );
                cx.sign_http(
                    request,
                    Box::new(Envelope::http(
                        identity.clone(),
                        method,
                        params.clone(),
                        &url,
                    )),
                );
            }
        },
    );
    let mut e = Engine::new(
        venue,
        Some(safe_cex_engine::test_support::seeded_entropy(7)),
    )
    .unwrap();
    e.set_signer(Some(Arc::new(provider))).unwrap();
    e.start(now);
    safe_cex_engine::test_support::prepared_actions(&mut e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if !auth => r.body,
            Action::WsSend { text, .. } if auth => Some(text),
            _ => None,
        })
        .expect("real provider recipe envelope")
}

#[test]
fn identity_transport_then_signature_failure_has_no_extra_retry() {
    let mut e = engine(options());
    let r = http(&ask(&mut e, NOW), "private/get-accounts");
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: r.id,
            error: "connection lost",
        },
    );
    let a = actions(&mut e, NOW + 1.);
    assert!(terminals(&a).is_empty());
    let retry = http(&a, "private/get-accounts");
    let a = respond(&mut e, NOW + 2., &retry, 200, r#"{"code":40101}"#);
    no_private_http(&a);
    assert_eq!(terminals(&a).len(), 1);
    assert!(terminals(&a)[0].is_err());
}

#[test]
fn identity_signature_then_transport_uses_only_remaining_counter_budget() {
    let mut e = engine(options());
    let r = http(&ask(&mut e, NOW), "private/get-accounts");
    let a = respond(&mut e, NOW + 1., &r, 200, r#"{"code":40101}"#);
    let mut r = http(&a, "private/get-accounts");
    for attempt in 1..=3 {
        e.handle(
            NOW + 1. + f64::from(attempt),
            Event::HttpFailed {
                request: r.id,
                error: "connection lost",
            },
        );
        let a = actions(&mut e, NOW + 1. + f64::from(attempt));
        if attempt < 3 {
            r = http(&a, "private/get-accounts");
            assert!(terminals(&a).is_empty());
        } else {
            no_private_http(&a);
            assert_eq!(terminals(&a).len(), 1);
            assert!(terminals(&a)[0].is_err());
        }
    }
}

#[test]
fn identity_four_transport_attempts_are_immediate_with_fresh_native_stamp() {
    let mut e = engine(options());
    let mut r = http(&ask(&mut e, NOW), "private/get-accounts");
    for attempt in 1..=4 {
        let now = NOW + f64::from(attempt);
        e.handle(
            now,
            Event::HttpFailed {
                request: r.id,
                error: "connection lost",
            },
        );
        let a = actions(&mut e, now);
        if attempt < 4 {
            r = http(&a, "private/get-accounts");
            let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["nonce"], now as u64);
            assert!(terminals(&a).is_empty());
        } else {
            no_private_http(&a);
            assert_eq!(terminals(&a).len(), 1);
        }
    }
}

#[test]
fn account_signature_transport_counter_resets_only_at_next_stage() {
    let mut h = Harness::new();
    let r = h.http("private/get-positions");
    h.actions.clear();
    h.event(Event::HttpFailed {
        request: r.id,
        error: "connection lost",
    });
    let retry = h.http("private/get-positions");
    h.respond(&retry, support::EMPTY);
    let balance = h.http("private/user-balance");
    h.respond(&balance, r#"{"code":40101}"#);
    let retry = h.http("private/user-balance");
    h.respond(&retry, support::BALANCE);
    h.reply("private/get-open-orders", support::EMPTY);
    h.reply("private/advanced/get-open-orders", support::EMPTY);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 10000.))
    );
}

#[test]
fn account_transport_then_signature_error_does_not_gain_another_retry() {
    let mut h = Harness::new();
    let r = h.http("private/get-positions");
    h.actions.clear();
    h.event(Event::HttpFailed {
        request: r.id,
        error: "connection lost",
    });
    let r = h.http("private/get-positions");
    h.respond(&r, r#"{"code":40101}"#);
    no_private_http(&h.actions);
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 10000.))
            .count(),
        1
    );
}

#[test]
fn account_signature_then_transport_uses_only_remaining_counter_budget() {
    let mut h = Harness::new();
    let r = h.http("private/get-positions");
    h.actions.clear();
    h.respond(&r, r#"{"code":40101}"#);
    let mut r = h.http("private/get-positions");
    for attempt in 1..=3 {
        h.event(Event::HttpFailed {
            request: r.id,
            error: "connection lost",
        });
        if attempt < 3 {
            r = h.http("private/get-positions");
        } else {
            no_private_http(&h.actions);
            assert_eq!(
                h.actions
                    .iter()
                    .filter(
                        |a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 10000.)
                    )
                    .count(),
                1
            );
        }
    }
}

#[test]
fn orders_refresh_initially_and_every_fourth_account_poll() {
    let mut h = Harness::new();
    h.load();
    for cycle in 1..=4 {
        let (timer, at_ms) = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000. => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("native ten-second account poll");
        h.actions.clear();
        h.now = at_ms;
        h.event(Event::TimerFired { timer });
        h.reply("private/get-positions", support::EMPTY);
        h.reply("private/user-balance", support::BALANCE);
        if cycle == 4 {
            h.reply("private/get-open-orders", support::EMPTY);
            h.reply("private/advanced/get-open-orders", support::EMPTY);
        } else {
            assert!(
                !h.actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.contains("get-open-orders")))
            );
        }
    }
}

#[test]
fn advanced_order_retry_retains_regular_rows_and_snapshot_authority() {
    let mut h = Harness::new();
    h.reply("private/get-positions", support::EMPTY);
    h.reply("private/user-balance", support::BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, support::ORDER),
    );
    assert!(h.engine.store().order("9007199254740993").is_none());
    let r = h.http("private/advanced/get-open-orders");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "connection lost",
    });
    h.reply("private/advanced/get-open-orders", support::EMPTY);
    assert_eq!(
        h.engine
            .store()
            .order("9007199254740993")
            .unwrap()
            .remaining,
        1.
    );
}

#[test]
fn secret_erasure_removes_primary_and_unused_secondary_material() {
    use safe_cex_crypto::secrets::EraseSecrets;
    let mut opts = options();
    opts.read_credentials = Some(secondary(true));
    opts.erase_secrets();
    assert!(opts.key.as_deref().is_none_or(str::is_empty));
    assert!(opts.secret.as_deref().is_none_or(str::is_empty));
    if let Some(read) = opts.read_credentials {
        assert!(read.key.is_empty());
        assert!(read.secret.is_empty());
        assert!(read.passphrase.as_deref().is_none_or(str::is_empty));
    }
}

#[test]
fn empty_batches_keep_credentials_precedence_without_http_or_progress() {
    for keyed in [false, true] {
        for command in [
            Command::PlaceOrders {
                command: OWNER,
                orders: vec![],
            },
            Command::CancelOrders {
                command: OWNER,
                ids: vec![],
            },
        ] {
            let mut e = engine(if keyed { options() } else { Options::default() });
            e.handle(NOW, Event::Command(command));
            let a = actions(&mut e, NOW);
            assert!(a.iter().all(|a| matches!(a, Action::Notice(_))));
            assert!(
                !a.iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandProgress { .. })))
            );
            let t = terminals(&a);
            assert_eq!(t.len(), 1);
            if keyed {
                assert_eq!(t[0].as_ref().unwrap(), "[]");
            } else {
                assert_eq!(
                    t[0].as_ref().unwrap_err().message,
                    "Crypto.com credentials are required"
                );
            }
        }
    }
}

#[test]
fn recipe_apply_without_prepare_cannot_inject_a_signed_body() {
    use safe_cex_engine::signing::{Identity, SignReceipt};
    use safe_cex_engine::{RequestId, SignStep};
    let identity: Identity =
        serde_json::from_str(r#"{"credential":"primary.credential","signing":"primary.signing"}"#)
            .unwrap();
    let url = "https://api.crypto.com/exchange/v1/private/get-accounts";
    let mut recipe = safe_cex_venue_cryptocom::sign::Envelope::http(
        identity,
        "private/get-accounts",
        Params::default(),
        url,
    );
    let original = HttpRequest {
        id: RequestId(1),
        method: Method::Post,
        route: None,
        timeout_ms: Some(20000),
        url: url.into(),
        headers: vec![("Content-Type", "application/json".into())],
        signer_headers: vec![],
        body: None,
    };
    let mut outbound = Action::Http(original.clone());
    let result = SignResult::new(
        vec![b"fixture-primary-key".to_vec(), vec![7; 32]],
        SignReceipt::default(),
    );
    assert!(
        recipe.apply(&mut outbound, &result).is_err(),
        "apply needs its attested preparation and exact receipt"
    );
    let Action::Http(after) = outbound else {
        unreachable!()
    };
    assert_eq!(
        after, original,
        "refusal must not partially mutate the request"
    );
}
