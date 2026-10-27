//! Current signed account evidence and bounded private-read recovery.
mod support;
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Event, HttpRequest, Notice, Response,
};
use support::*;
fn http(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn answer(
    e: &mut TestEngine,
    r: &HttpRequest,
    now: f64,
    status: u16,
    headers: &str,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}
fn auth(actions: &[Action]) -> Vec<(CredentialSlot, AccountReadScope, AuthOutcome)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::AccountAuth {
                credential,
                scope,
                outcome,
            }) => Some((*credential, *scope, *outcome)),
            _ => None,
        })
        .collect()
}
fn started(secondary: bool) -> (TestEngine, Vec<Action>) {
    started_at(secondary, 0.0)
}
fn started_at(secondary: bool, now: f64) -> (TestEngine, Vec<Action>) {
    let mut e = support::engine(support::Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..support::Options::default()
    });
    e.start(now);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
    let markets = http(&actions, "/trading_pairs");
    let actions = answer(&mut e, &markets, now, 200, "", PAIRS);
    assert!(
        auth(&actions).is_empty(),
        "public catalogue is unsigned auth evidence"
    );
    (e, actions)
}
#[test]
fn inactive_rate_pause_does_not_block_initial_reads_on_a_negative_clock() {
    let (_, actions) = started_at(false, -1000.0);
    for path in [
        "/futures/account?",
        "get_pending_positions",
        "/trade/get_pending_orders",
    ] {
        http(&actions, path);
    }
}
#[test]
fn actual_account_requests_report_selected_signer_and_valid_required_shapes_once() {
    for secondary in [false, true] {
        let slot = if secondary {
            CredentialSlot::Secondary
        } else {
            CredentialSlot::Primary
        };
        let (mut e, actions) = started(secondary);
        for (path, body, scope) in [
            ("/futures/account?", ACCOUNT, AccountReadScope::Balance),
            (
                "get_pending_positions",
                POSITIONS,
                AccountReadScope::Positions,
            ),
            (
                "/trade/get_pending_orders",
                r#"{"code":0,"data":{"orderList":[]}}"#,
                AccountReadScope::Orders,
            ),
        ] {
            let r = http(&actions, path);
            assert!(
                r.headers.iter().any(|(k, v)| *k == "api-key"
                    && v == if secondary { "read-key" } else { "primary-key" })
            );
            let result = answer(&mut e, &r, 100.0, 200, "", body);
            assert_eq!(auth(&result), vec![(slot, scope, AuthOutcome::Accepted)]);
            assert!(
                auth(&answer(&mut e, &r, 101.0, 200, "", body)).is_empty(),
                "duplicate callback has no evidence"
            );
        }
    }
}
#[test]
fn only_strict_documented_native_refusals_prove_account_credential_rejection() {
    for native in ["403", "10003", "10004", "\"403\"", "\"10003\"", "\"10004\""] {
        let (mut e, actions) = started(true);
        let r = http(&actions, "get_pending_positions");
        let body = format!(r#"{{"code":{native},"msg":"refused"}}"#);
        assert_eq!(
            auth(&answer(&mut e, &r, 100.0, 200, "", &body)),
            vec![(
                CredentialSlot::Secondary,
                AccountReadScope::Positions,
                AuthOutcome::Rejected
            )],
            "{native}"
        );
    }
}
#[test]
fn ambiguous_codes_statuses_and_malformed_success_shapes_remain_auth_inert() {
    for (status, body) in [
        (401, r#"{"code":10003}"#),
        (403, r#"{"code":10004}"#),
        (500, r#"{"code":403}"#),
        (200, r#"{"code":10007}"#),
        (200, r#"{"code":10010}"#),
        (200, r#"{"code":10011}"#),
        (200, r#"{"code":403.1}"#),
        (200, r#"{"code":"403.0"}"#),
        (200, r#"{"code":null}"#),
        (200, r#"{"code":true}"#),
        (200, r#"{"msg":"invalid api key"}"#),
        (200, r#"{"code":0,"data":null}"#),
        (200, r#"{"code":0,"data":{}}"#),
        (200, r#"{"code":0,"data":[]"#),
    ] {
        let (mut e, actions) = started(true);
        let r = http(&actions, "get_pending_positions");
        assert!(
            auth(&answer(&mut e, &r, 100.0, status, "", body)).is_empty(),
            "{status} {body}"
        );
    }
}
#[test]
fn private_missing_list_diagnostics_never_copy_the_raw_account_body() {
    let (mut e, actions) = started(false);
    let r = http(&actions, "get_pending_positions");
    let result = answer(
        &mut e,
        &r,
        100.0,
        200,
        "",
        r#"{"code":10004,"msg":"IP whitelist refused","privateAccount":"DO-NOT-LOG-ACCOUNT-PAYLOAD"}"#,
    );
    for action in result {
        if let Action::Notice(Notice::Event(event)) = action {
            assert!(!event.message.contains("DO-NOT-LOG-ACCOUNT-PAYLOAD"));
        }
    }
}
#[test]
fn a_private_rate_pause_cannot_be_bypassed_by_the_balance_tick() {
    let (mut e, actions) = started(false);
    let r = http(&actions, "/futures/account?");
    let mut pending = answer(
        &mut e,
        &r,
        100.0,
        429,
        "Retry-After: 1\r\n",
        r#"{"code":10006,"msg":"too frequent"}"#,
    );
    let mut timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 500.0 => Some(*timer),
            _ => None,
        })
        .expect("balance tick");
    for now in [500.0, 1000.0, 1500.0, 2000.0] {
        e.handle(now, Event::TimerFired { timer });
        let current: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(
            !current
                .iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.contains("/futures/account?"))),
            "tick bypassed rate pause at {now}"
        );
        timer = current
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == now + 500.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        pending.extend(current);
    }
    assert!(
        pending
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms>=60100.0)),
        "rate pause must floor short Retry-After at60s"
    );
}

fn retry(actions: &[Action], now: f64) -> (safe_cex_engine::TimerId, f64) {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= now + 750.0 && *at_ms <= now + 1000.0 =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("actual jittered retry")
}
#[test]
fn secondary_refusal_then_current_recovery_has_only_its_scoped_evidence() {
    let (mut e, actions) = started(true);
    let r = http(&actions, "get_pending_positions");
    let failed = answer(
        &mut e,
        &r,
        100.0,
        200,
        "",
        r#"{"code":10004,"msg":"IP refused"}"#,
    );
    assert_eq!(
        auth(&failed),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Positions,
            AuthOutcome::Rejected
        )]
    );
    assert!(!failed.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::AUTH_REJECTED)),"legacy auth diagnostic would invent primary/account failure in SDK");
    let (timer, at) = retry(&failed, 100.0);
    e.handle(at, Event::TimerFired { timer });
    let current: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let r = http(&current, "get_pending_positions");
    let recovered = answer(&mut e, &r, at + 1.0, 200, "", POSITIONS);
    assert_eq!(
        auth(&recovered),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Positions,
            AuthOutcome::Accepted
        )]
    );
    assert!(!recovered.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::AUTH_REJECTED)));
}
fn settings(e: &mut TestEngine, now: f64, symbol: &str) -> HttpRequest {
    e.handle(
        now,
        Event::Command(safe_cex_engine::Command::SubscribeCandles {
            symbol: symbol.into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect();
    http(&actions, "/get_leverage_margin_mode")
}
#[test]
fn settings_symbols_do_not_heal_each_other_and_empty_margin_mode_preserves_existing_fallback() {
    let (mut e, _) = started(true);
    let mut markets = e.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.symbol = "ETHUSDT".into();
    eth.id = "ETH/USDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    e.store_mut().set_markets(markets);
    let btc = settings(&mut e, 100.0, "BTCUSDT");
    let failed = answer(
        &mut e,
        &btc,
        200.0,
        200,
        "",
        r#"{"code":403,"msg":"not permitted"}"#,
    );
    assert_eq!(
        auth(&failed),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Settings,
            AuthOutcome::Rejected
        )]
    );
    let eth = settings(&mut e, 300.0, "ETHUSDT");
    assert!(
        auth(&answer(
            &mut e,
            &eth,
            400.0,
            200,
            "",
            r#"{"code":0,"data":{"leverage":30,"marginMode":"CROSS"}}"#
        ))
        .is_empty()
    );
    let (timer, at) = retry(&failed, 200.0);
    e.handle(at, Event::TimerFired { timer });
    let current: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let btc = http(&current, "/get_leverage_margin_mode");
    let recovered = answer(
        &mut e,
        &btc,
        at + 1.0,
        200,
        "",
        r#"{"code":0,"data":{"leverage":20,"marginMode":""}}"#,
    );
    assert_eq!(
        auth(&recovered),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Settings,
            AuthOutcome::Accepted
        )]
    );
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    e.handle(
        at + 2.0,
        Event::Command(safe_cex_engine::Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(
        !safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.contains("/get_leverage_margin_mode")))
    );
}

fn fired(e: &mut TestEngine, timer: safe_cex_engine::TimerId, at: f64) -> Vec<Action> {
    e.handle(at, Event::TimerFired { timer });
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}
fn private_gets(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r)
                if r.url.contains("/api/v1/futures/") && !r.url.contains("/market/") =>
            {
                Some(r)
            }
            _ => None,
        })
        .collect()
}
#[test]
fn each_documented_rate_signal_preserves_headers_and_bounds_the_private_pause() {
    for (status, headers, body, delay) in [
        (429, "Retry-After: 1\r\n", r#"{"code":10006}"#, 60_000.0),
        (
            418,
            "retry-after: 120\r\n",
            r#"{"code":0,"data":["#,
            120_000.0,
        ),
        (
            429,
            "Retry-After: 999999999\r\n",
            "proxy refused",
            3_600_000.0,
        ),
        (200, "", r#"{"code":10005,"data":{}}"#, 60_000.0),
        (
            200,
            "Retry-After: 120\r\n",
            r#"{"code":"10006","data":null}"#,
            120_000.0,
        ),
    ] {
        let (mut e, actions) = started(false);
        let r = http(&actions, "get_pending_positions");
        let failed = answer(&mut e, &r, 100.0, status, headers, body);
        assert!(auth(&failed).is_empty());
        let timer = failed
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 100.0 + delay => Some(*timer),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing bounded pause for {status} {body}: {failed:?}"));
        let early = fired(&mut e, timer, 100.0 + delay - 1.0);
        assert!(
            private_gets(&early).is_empty(),
            "even an early timer cannot bypass host pause"
        );
        let timer = early
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 100.0 + delay => Some(*timer),
                _ => None,
            })
            .expect("remaining pause rearmed");
        http(
            &fired(&mut e, timer, 100.0 + delay),
            "get_pending_positions",
        );
    }
}

#[derive(Default)]
struct Timers(Vec<(safe_cex_engine::TimerId, f64)>);
impl Timers {
    fn observe(&mut self, actions: &[Action]) {
        for action in actions {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    self.0.retain(|(old, _)| old != timer);
                    self.0.push((*timer, *at_ms));
                }
                Action::ClearTimer { timer } => self.0.retain(|(old, _)| old != timer),
                _ => {}
            }
        }
    }
    fn next(&mut self, until: f64) -> Option<(safe_cex_engine::TimerId, f64)> {
        let (i, _) = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, (_, at))| *at <= until)
            .min_by(|(_, a), (_, b)| a.1.total_cmp(&b.1))?;
        Some(self.0.swap_remove(i))
    }
}
#[test]
fn host_pause_blocks_healthy_keys_initial_continuations_ticks_and_lost_read_repair() {
    let (mut e, initial) = started(false);
    let mut timers = Timers::default();
    timers.observe(&initial);
    let balance = http(&initial, "/futures/account?");
    let failed = answer(
        &mut e,
        &balance,
        100.0,
        429,
        "Retry-After: 1\r\n",
        r#"{"code":10006}"#,
    );
    timers.observe(&failed);
    // A successful unfailed sibling must not clear the host's rate window.
    let positions = http(&initial, "get_pending_positions");
    let accepted = answer(
        &mut e,
        &positions,
        200.0,
        200,
        "",
        r#"{"code":0,"data":{"positionList":[]}}"#,
    );
    timers.observe(&accepted);
    let regular = http(&initial, "/trade/get_pending_orders");
    let continuation = answer(
        &mut e,
        &regular,
        300.0,
        200,
        "",
        r#"{"code":0,"data":{"orderList":[]}}"#,
    );
    assert!(
        private_gets(&continuation).is_empty(),
        "initial TPSL continuation must wait too"
    );
    timers.observe(&continuation);
    e.handle(
        400.0,
        Event::Command(safe_cex_engine::Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let interested: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(
        !private_gets(&interested)
            .iter()
            .any(|r| r.url.contains("get_leverage_margin_mode"))
    );
    timers.observe(&interested);
    while let Some((timer, at)) = timers.next(60_099.0) {
        let actions = fired(&mut e, timer, at);
        assert!(
            private_gets(&actions).is_empty(),
            "private request bypassed pause at {at}: {actions:?}"
        );
        timers.observe(&actions);
    }
    let mut resumed = Vec::new();
    while let Some((timer, at)) = timers.next(60_100.0) {
        let actions = fired(&mut e, timer, at);
        timers.observe(&actions);
        resumed.extend(actions);
    }
    for path in [
        "/futures/account?",
        "/tpsl/get_pending_orders",
        "get_leverage_margin_mode",
    ] {
        http(&resumed, path);
    }
    assert_eq!(
        private_gets(&resumed)
            .iter()
            .filter(|r| r.url.contains("get_leverage_margin_mode"))
            .count(),
        1,
        "one deferred settings source, not one per tick"
    );
}

#[test]
fn retries_back_off_per_key_to_the_cap_and_only_current_success_resets_them() {
    let (mut e, initial) = started(true);
    // A live private session: opened promptly, then silent until its first pong.
    let socket = initial
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.contains("/private/") => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(0.0, Event::WsOpened { socket });
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    let mut r = http(&initial, "get_pending_positions");
    let mut now = 100.0;
    for nominal in [1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 30000.0, 30000.0] {
        let failure = answer(&mut e, &r, now, 200, "", r#"{"code":10004}"#);
        assert_eq!(auth(&failure).last().unwrap().2, AuthOutcome::Rejected);
        let retries: Vec<_> = failure
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms }
                    if *at_ms >= now + nominal * 0.75 && *at_ms <= now + nominal =>
                {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .collect();
        assert_eq!(retries.len(), 1, "one retry at nominal {nominal}");
        let (timer, at) = retries[0];
        let next = fired(&mut e, timer, at);
        r = http(&next, "get_pending_positions");
        now = at + 1.0;
    }
    let recovered = answer(
        &mut e,
        &r,
        now,
        200,
        "",
        r#"{"code":0,"data":{"positionList":[]}}"#,
    );
    assert_eq!(auth(&recovered).last().unwrap().2, AuthOutcome::Accepted);
    // Opening the private socket starts a fresh read, without resetting failures itself.
    // The live session closes and the engine reconnects; the test follows the new socket.
    e.handle(now + 1.0, Event::WsClosed { socket, code: 1006 });
    let (timer, at) = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("the private reconnect backoff");
    let socket = fired(&mut e, timer, at)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.contains("/private/") => Some(socket),
            _ => None,
        })
        .expect("the private reconnect");
    let now = at;
    e.handle(now + 1.0, Event::WsOpened { socket });
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let r = http(&actions, "get_pending_positions");
    let failed = answer(&mut e, &r, now + 2.0, 200, "", r#"{"code":10004}"#);
    retry(&failed, now + 2.0);
}

#[test]
fn malformed_regular_recovery_and_valid_tpsl_cannot_heal_regular_refusal() {
    let (mut e, socket) = startup();
    e.handle(
        100.0,
        Event::WsMessage {
            socket,
            text: r#"{"ch":"order","data":{}}"#,
        },
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1100.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let actions = fired(&mut e, timer, 1100.0);
    let regular = http(&actions, "/trade/get_pending_orders");
    let tpsl = http(&actions, "/tpsl/get_pending_orders");
    let refused = answer(&mut e, &regular, 1200.0, 200, "", r#"{"code":10003}"#);
    assert_eq!(
        auth(&refused),
        vec![(
            CredentialSlot::Primary,
            AccountReadScope::Orders,
            AuthOutcome::Rejected
        )]
    );
    let (timer, at) = retry(&refused, 1200.0);
    let actions = fired(&mut e, timer, at);
    let regular = http(&actions, "/trade/get_pending_orders");
    let malformed = answer(
        &mut e,
        &regular,
        at + 1.0,
        200,
        "",
        r#"{"code":0,"data":{}}"#,
    );
    assert!(auth(&malformed).is_empty());
    let sibling = answer(&mut e, &tpsl, at + 2.0, 200, "", TPSL);
    assert!(
        auth(&sibling).is_empty(),
        "malformed code0 must not clear regular endpoint's refusal bit"
    );
    let (timer, due) = malformed
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= at + 1501.0 && *at_ms <= at + 2001.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("second regular retry");
    let actions = fired(&mut e, timer, due);
    let regular = http(&actions, "/trade/get_pending_orders");
    assert_eq!(
        auth(&answer(&mut e, &regular, due + 1.0, 200, "", ORDERS)),
        vec![(
            CredentialSlot::Primary,
            AccountReadScope::Orders,
            AuthOutcome::Accepted
        )]
    );
}
#[test]
fn settings_scope_tracks_more_than_thirty_two_symbols_without_aliasing() {
    let (mut e, _) = started(true);
    let template = e.store().markets()[0].clone();
    let markets = (0..40)
        .map(|i| {
            let mut m = template.clone();
            m.symbol = format!("COIN{i}USDT");
            m.id = m.symbol.clone();
            m.base = format!("COIN{i}");
            m
        })
        .collect();
    e.store_mut().set_markets(markets);
    let reads: Vec<_> = (0..40)
        .map(|i| settings(&mut e, 100.0 + i as f64, &format!("COIN{i}USDT")))
        .collect();
    let refused = answer(&mut e, &reads[32], 200.0, 200, "", r#"{"code":10004}"#);
    assert_eq!(auth(&refused).last().unwrap().2, AuthOutcome::Rejected);
    for r in reads.iter().take(32).chain(reads.iter().skip(33)) {
        assert!(
            auth(&answer(
                &mut e,
                r,
                300.0,
                200,
                "",
                r#"{"code":0,"data":{"leverage":20,"marginMode":"CROSS"}}"#
            ))
            .is_empty(),
            "unrelated symbol cannot heal symbol32"
        );
    }
    let (timer, at) = retry(&refused, 200.0);
    let actions = fired(&mut e, timer, at);
    let r = http(&actions, "get_leverage_margin_mode");
    assert!(r.url.contains("COIN32USDT"));
    assert_eq!(
        auth(&answer(
            &mut e,
            &r,
            at + 1.0,
            200,
            "",
            r#"{"code":0,"data":{"leverage":20,"marginMode":"CROSS"}}"#
        )),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Settings,
            AuthOutcome::Accepted
        )]
    );
}
#[test]
fn superseded_position_success_or_refusal_cannot_heal_or_extend_replacement_retry() {
    for body in [
        r#"{"code":0,"data":{"positionList":[]}}"#,
        r#"{"code":10004}"#,
    ] {
        let (mut e, initial) = started(true);
        let old = http(&initial, "get_pending_positions");
        let mut timers = Timers::default();
        timers.observe(&initial);
        let mut replacement = None;
        while let Some((timer, at)) = timers.next(10_000.0) {
            let actions = fired(&mut e, timer, at);
            if let Some(r) = private_gets(&actions)
                .into_iter()
                .find(|r| r.url.contains("get_pending_positions"))
            {
                replacement = Some(r.clone());
            }
            timers.observe(&actions);
        }
        let current = replacement.expect("lost position read replaced after10s");
        let failed = answer(&mut e, &current, 10_001.0, 200, "", r#"{"code":10003}"#);
        assert_eq!(auth(&failed).last().unwrap().2, AuthOutcome::Rejected);
        let (timer, at) = retry(&failed, 10_001.0);
        let stale = answer(&mut e, &old, 10_002.0, 200, "", body);
        assert!(auth(&stale).is_empty(), "stale {body}");
        assert!(
            !stale
                .iter()
                .any(|a| matches!(a,Action::ClearTimer{timer:cleared}if *cleared==timer)),
            "old success cannot reset new retry"
        );
        let actions = fired(&mut e, timer, at);
        let current = http(&actions, "get_pending_positions");
        assert_eq!(
            auth(&answer(
                &mut e,
                &current,
                at + 1.0,
                200,
                "",
                r#"{"code":0,"data":{"positionList":[]}}"#
            ))
            .last()
            .unwrap()
            .2,
            AuthOutcome::Accepted
        );
    }
}
#[test]
fn owned_preflight_during_a_malformed_rate_pause_fails_once_not_sent_and_never_replays() {
    let (mut e, socket) = startup();
    e.handle(
        100.0,
        Event::WsMessage {
            socket,
            text: r#"{"ch":"position","data":{}}"#,
        },
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1100.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let actions = fired(&mut e, timer, 1100.0);
    let positions = http(&actions, "get_pending_positions");
    let failure = answer(
        &mut e,
        &positions,
        1200.0,
        429,
        "Retry-After: 120\r\n",
        r#"{"code":0,"data":["#,
    );
    assert!(auth(&failure).is_empty());
    let order=serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
    e.handle(
        1300.0,
        Event::Command(safe_cex_engine::Command::PlaceOrders {
            command: 88,
            orders: vec![order],
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 88, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(private_gets(&actions).is_empty());
    let mut timers = Timers::default();
    timers.observe(&failure);
    timers.observe(&actions);
    let mut all = Vec::new();
    while let Some((timer, at)) = timers.next(122_000.0) {
        let actions = fired(&mut e, timer, at);
        timers.observe(&actions);
        for r in private_gets(&actions) {
            assert_eq!(r.method, safe_cex_engine::Method::Get);
            let body = if r.url.contains("positions") {
                POSITIONS
            } else if r.url.contains("/tpsl/") {
                TPSL
            } else if r.url.contains("orders") {
                ORDERS
            } else {
                ACCOUNT
            };
            let answered = answer(&mut e, r, at + 0.1, 200, "", body);
            timers.observe(&answered);
            all.extend(answered);
        }
        all.extend(actions);
    }
    assert!(
        !all.iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==safe_cex_engine::Method::Post))
    );
    assert!(
        !all.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: 88, .. } | Notice::CommandFailed { command: 88, .. }
            )
        )),
        "retired prepare must not be resurrected"
    );
}

#[test]
fn shared_pause_expiry_coalesces_deferred_and_failed_sources_in_either_timer_order() {
    for reversed in [false, true] {
        let (mut e, initial) = started(false);
        let positions = http(&initial, "get_pending_positions");
        let balance = http(&initial, "/futures/account?");
        let mut timers = Timers::default();
        timers.observe(&initial);
        let pause = answer(&mut e, &balance, 100.0, 429, "", r#"{"code":10006}"#);
        timers.observe(&pause);
        // Lost-read repair defers a healthy-but-unanswered positions endpoint.
        while let Some((timer, at)) = timers.next(10_000.0) {
            let actions = fired(&mut e, timer, at);
            assert!(private_gets(&actions).is_empty());
            timers.observe(&actions);
        }
        // Its actual failure now also owns a retry at that same host deadline.
        let failure = answer(&mut e, &positions, 10_001.0, 200, "", r#"{"code":10004}"#);
        timers.observe(&failure);
        while let Some((timer, at)) = timers.next(60_099.0) {
            let actions = fired(&mut e, timer, at);
            assert!(private_gets(&actions).is_empty());
            timers.observe(&actions);
        }
        let mut expiry = Vec::new();
        while let Some((timer, at)) = timers.next(60_100.0) {
            expiry.push((timer, at));
        }
        assert!(expiry.len() >= 3, "backoff/deferred/tick deadlines present");
        if reversed {
            expiry.reverse();
        }
        let mut emitted = Vec::new();
        for (timer, at) in expiry {
            emitted.extend(fired(&mut e, timer, at));
        }
        assert_eq!(
            private_gets(&emitted)
                .iter()
                .filter(|r| r.url.contains("get_pending_positions"))
                .count(),
            1,
            "coalesce expiry order reversed={reversed}"
        );
    }
}
#[test]
fn valid_empty_lists_and_bad_financial_rows_accept_credentials_without_false_readiness() {
    for (path, body, scope) in [
        (
            "get_pending_positions",
            r#"{"code":0,"data":{"positionList":[]}}"#,
            AccountReadScope::Positions,
        ),
        (
            "get_pending_positions",
            r#"{"code":0,"data":{"positionList":[{"symbol":"BTCUSDT","qty":{}}]}}"#,
            AccountReadScope::Positions,
        ),
        (
            "/trade/get_pending_orders",
            r#"{"code":0,"data":{"orderList":[]}}"#,
            AccountReadScope::Orders,
        ),
        (
            "/trade/get_pending_orders",
            r#"{"code":0,"data":{"orderList":[{"symbol":"BTCUSDT","orderId":"bad","qty":{}}]}}"#,
            AccountReadScope::Orders,
        ),
    ] {
        let (mut e, initial) = started(true);
        let r = http(&initial, path);
        let actions = answer(&mut e, &r, 100.0, 200, "", body);
        assert_eq!(
            auth(&actions),
            vec![(CredentialSlot::Secondary, scope, AuthOutcome::Accepted)]
        );
        if body.contains("\"qty\":{}") {
            assert!(actions.iter().any(|a| matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::ROW_SKIPPED)),"typed malformed row must be skipped");
            if scope == AccountReadScope::Positions {
                assert!(
                    !e.store().loaded().positions,
                    "malformed initial positions cannot establish readiness"
                );
            }
        } else if scope == AccountReadScope::Positions {
            assert!(
                e.store().loaded().positions,
                "valid empty positions can establish readiness"
            );
        }
        assert!(
            !e.store().loaded().orders,
            "one empty/partial half cannot authorize both initial endpoints"
        );
    }
}
