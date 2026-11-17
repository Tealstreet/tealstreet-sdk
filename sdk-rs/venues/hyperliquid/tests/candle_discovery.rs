//! Synthetic public catalogues only; no account, signer or venue connection.
use safe_cex_engine::model::Interval;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Response, VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const CORE: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100"}]]"#;
const DEX: &str = r#"[{"universe":[{"name":"xyz:GOLD","szDecimals":2,"maxLeverage":20}]},[{"midPx":"200","markPx":"200"}]]"#;

fn reply(e: &mut Engine<Hyperliquid>, at: f64, request: &HttpRequest, body: &str) {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn request(actions: &[Action], kind: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request)
                if serde_json::from_str::<serde_json::Value>(request.body.as_deref()?).ok()?["type"]
                    == kind =>
            {
                Some(request.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {kind}: {actions:?}"))
}

fn results(actions: &[Action], command: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: own,
                result,
            }) if *own == command => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed {
                command: own,
                error,
            }) if *own == command => Some(Err(error.clone())),
            _ => None,
        })
        .collect()
}

fn no_candles(actions: &[Action]) {
    assert!(!actions.iter().any(|action| match action {
        Action::Http(request) => request.body.as_deref().is_some_and(|body| body.contains("candleSnapshot")),
        Action::ManagedHttp(action) => matches!(action.as_ref(), safe_cex_engine::request_context::HttpAction::Prepare { request, .. }
            if request.body.as_deref().is_some_and(|body| body.contains("candleSnapshot"))),
        _ => false,
    }), "no premature candle request: {actions:?}");
}

fn partial() -> (Engine<Hyperliquid>, HttpRequest) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.);
    let meta = request(&prepared_now(&mut e), "metaAndAssetCtxs");
    reply(&mut e, 1000., &meta, CORE);
    assert!(e.store().loaded().markets);
    assert!(e.store().market("BTC-PERP").is_some());
    let dexes = request(&prepared_now(&mut e), "perpDexs");
    (e, dexes)
}

fn ask(
    e: &mut Engine<Hyperliquid>,
    at: f64,
    command: u32,
    symbol: &str,
    wait: Option<u32>,
) -> Vec<Action> {
    e.handle_command(
        at,
        Command::FetchCandles {
            command,
            symbol: symbol.into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: Some(0.),
            to_ms: Some(60_000.),
        },
        wait,
    )
    .unwrap();
    prepared_now(e)
}

fn assert_missing(actions: &[Action], command: u32) {
    let terminal = results(actions, command);
    assert_eq!(terminal.len(), 1, "one immediate missing-symbol refusal");
    assert_eq!(
        terminal[0].as_ref().unwrap_err().message,
        "Market not found"
    );
    no_candles(actions);
}

#[test]
fn candles_wait_for_the_requested_dex_row_after_core_markets_are_ready() {
    let (mut e, dexes) = partial();
    let actions = ask(&mut e, 1100., 100, "XYZ|GOLD-PERP", None);
    assert!(
        results(&actions, 100).is_empty(),
        "partial MARKETS is not a definitive absence"
    );
    no_candles(&actions);
    reply(&mut e, 1200., &dexes, r#"[null,{"name":"xyz"}]"#);
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 100).is_empty());
    no_candles(&actions);
    let dex = request(&actions, "metaAndAssetCtxs");
    reply(&mut e, 1300., &dex, DEX);
    let actions = prepared_now(&mut e);
    let candles = request(&actions, "candleSnapshot");
    assert_eq!(
        candles.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"xyz:GOLD","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
    assert!(results(&actions, 100).is_empty());
    reply(
        &mut e,
        1400.,
        &candles,
        r#"[{"t":1000,"o":"1","h":"2","l":"1","c":"2","v":"3"}]"#,
    );
    let terminal = results(&prepared_now(&mut e), 100);
    assert_eq!(terminal.len(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(terminal[0].as_ref().unwrap()).unwrap()[0]["close"],
        2.
    );
}

#[test]
fn known_core_candles_do_not_wait_for_other_catalogues() {
    let (mut e, _) = partial();
    let actions = ask(&mut e, 1100., 101, "BTC-PERP", None);
    let candles = request(&actions, "candleSnapshot");
    assert_eq!(
        candles.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"BTC","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
    reply(&mut e, 1101., &candles, "[]");
    assert_eq!(results(&prepared_now(&mut e), 101), vec![Ok("[]".into())]);
}

#[test]
fn missing_symbol_refuses_when_discovery_settles_and_stays_fast_afterwards() {
    let (mut e, dexes) = partial();
    let actions = ask(&mut e, 1100., 102, "XYZ|REMOVED-PERP", None);
    assert!(results(&actions, 102).is_empty());
    reply(&mut e, 1200., &dexes, "[null]");
    assert_missing(&prepared_now(&mut e), 102);
    assert_missing(&ask(&mut e, 1201., 103, "XYZ|REMOVED-PERP", None), 103);
}

#[test]
fn pending_discovery_expires_at_the_original_bound_without_a_late_send() {
    for wait in [None, Some(50), Some(60_000)] {
        let (mut e, dexes) = partial();
        let actions = ask(&mut e, 1100., 104, "XYZ|GOLD-PERP", wait);
        assert!(results(&actions, 104).is_empty());
        let deadline = 1100. + f64::from(wait.unwrap_or(20_000).min(20_000));
        let timer = actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
                _ => None,
            })
            .expect("a wake at the original capped deadline, even with no metadata response");
        e.handle(deadline, Event::TimerFired { timer });
        let terminal = results(&prepared_now(&mut e), 104);
        assert_eq!(terminal.len(), 1);
        assert_eq!(
            terminal[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::Timeout)
        );
        reply(&mut e, deadline + 1., &dexes, r#"[null,{"name":"xyz"}]"#);
        let actions = prepared_now(&mut e);
        no_candles(&actions);
        // A discovery request can itself have expired; either way no late candle can run.
        if let Some(dex) = actions.iter().find_map(|action| match action {
            Action::Http(request)
                if request
                    .body
                    .as_deref()
                    .is_some_and(|body| body.contains("metaAndAssetCtxs")) =>
            {
                Some(request.clone())
            }
            _ => None,
        }) {
            reply(&mut e, deadline + 2., &dex, DEX);
        }
        let actions = prepared_now(&mut e);
        no_candles(&actions);
        assert!(results(&actions, 104).is_empty());
    }
}

#[test]
fn an_early_host_timer_cannot_spend_the_only_timeout_wake() {
    let (mut e, _) = partial();
    let actions = ask(&mut e, 1100., 105, "XYZ|GOLD-PERP", Some(100));
    assert!(results(&actions, 105).is_empty());
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1200. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1150., Event::TimerFired { timer });
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 105).is_empty());
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1200. => Some(*timer),
            _ => None,
        })
        .expect("early host firing re-arms the remaining bound");
    e.handle(1200., Event::TimerFired { timer });
    assert_eq!(
        results(&prepared_now(&mut e), 105)[0]
            .as_ref()
            .unwrap_err()
            .kind,
        Some(ErrorKind::Timeout)
    );
}

#[test]
fn failed_dex_discovery_is_not_authoritative_missing_symbol_evidence() {
    let (mut e, dexes) = partial();
    let actions = ask(&mut e, 1100., 106, "XYZ|GOLD-PERP", Some(100));
    assert!(results(&actions, 106).is_empty());
    e.handle(
        1150.,
        Event::HttpFailed {
            request: dexes.id,
            error: "synthetic network loss",
        },
    );
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 106).is_empty());
    no_candles(&actions);
}

#[test]
fn an_absent_core_row_is_definitive_before_optional_dex_discovery() {
    let (mut e, _) = partial();
    assert_missing(&ask(&mut e, 1100., 107, "REMOVED-PERP", None), 107);
}

#[test]
fn a_request_waiting_on_discovery_keeps_its_original_time_geometry() {
    let (mut e, dexes) = partial();
    e.handle_command(
        60_000.,
        Command::FetchCandles {
            command: 108,
            symbol: "XYZ|GOLD-PERP".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        },
        Some(1000),
    )
    .unwrap();
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 108).is_empty());
    reply(&mut e, 60_100., &dexes, r#"[null,{"name":"xyz"}]"#);
    let dex = request(&prepared_now(&mut e), "metaAndAssetCtxs");
    reply(&mut e, 60_200., &dex, DEX);
    let candles = request(&prepared_now(&mut e), "candleSnapshot");
    assert_eq!(
        candles.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"xyz:GOLD","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
}

#[test]
fn an_expired_wait_cannot_send_or_complete_a_reused_command_id() {
    let (mut e, dexes) = partial();
    let actions = ask(&mut e, 1100., 109, "XYZ|GOLD-PERP", Some(50));
    assert!(results(&actions, 109).is_empty());
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1150. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1150., Event::TimerFired { timer });
    assert_eq!(results(&prepared_now(&mut e), 109).len(), 1);
    let core = request(&ask(&mut e, 1151., 109, "BTC-PERP", None), "candleSnapshot");
    reply(&mut e, 1152., &dexes, r#"[null,{"name":"xyz"}]"#);
    let dex = request(&prepared_now(&mut e), "metaAndAssetCtxs");
    reply(&mut e, 1153., &dex, DEX);
    let actions = prepared_now(&mut e);
    no_candles(&actions);
    assert!(results(&actions, 109).is_empty());
    reply(&mut e, 1154., &core, "[]");
    assert_eq!(results(&prepared_now(&mut e), 109), vec![Ok("[]".into())]);
}

#[test]
fn a_delisted_dex_row_refuses_even_while_another_row_needs_recovery() {
    let (mut e, dexes) = partial();
    assert!(results(&ask(&mut e, 1100., 110, "XYZ|GOLD-PERP", None), 110).is_empty());
    reply(&mut e, 1200., &dexes, r#"[null,{"name":"xyz"}]"#);
    let dex = request(&prepared_now(&mut e), "metaAndAssetCtxs");
    reply(
        &mut e,
        1300.,
        &dex,
        r#"[{"universe":[{"name":"xyz:GOLD","szDecimals":2,"maxLeverage":20,"isDelisted":true},{"name":"xyz:SILVER","szDecimals":2,"maxLeverage":20}]},[{"midPx":"200","markPx":"200"},{"midPx":"bad"}]]"#,
    );
    assert_missing(&prepared_now(&mut e), 110);
    assert_missing(&ask(&mut e, 1301., 111, "XYZ|GOLD-PERP", None), 111);
}

#[test]
fn a_settled_missing_row_does_not_wait_for_an_unrelated_dex() {
    let (mut e, dexes) = partial();
    assert!(results(&ask(&mut e, 1100., 112, "XYZ|REMOVED-PERP", None), 112).is_empty());
    reply(
        &mut e,
        1200.,
        &dexes,
        r#"[null,{"name":"xyz"},{"name":"other"}]"#,
    );
    let dex = request(&prepared_now(&mut e), "metaAndAssetCtxs");
    reply(&mut e, 1300., &dex, DEX);
    let actions = prepared_now(&mut e);
    assert_missing(&actions, 112);
    let other = request(&actions, "metaAndAssetCtxs");
    assert!(other.body.as_deref().unwrap().contains("other"));
}

#[test]
fn discovering_the_row_does_not_renew_its_http_deadline() {
    let (mut e, dexes) = partial();
    assert!(results(&ask(&mut e, 1100., 113, "XYZ|GOLD-PERP", Some(1000)), 113).is_empty());
    reply(&mut e, 1200., &dexes, r#"[null,{"name":"xyz"}]"#);
    let dex = request(&prepared_now(&mut e), "metaAndAssetCtxs");
    reply(&mut e, 1300., &dex, DEX);
    let actions = prepared_now(&mut e);
    let candles = request(&actions, "candleSnapshot");
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 2100. => Some(*timer),
            _ => None,
        })
        .expect("HTTP keeps the original deadline after discovery wait");
    e.handle(2100., Event::TimerFired { timer });
    let terminal = results(&prepared_now(&mut e), 113);
    assert_eq!(terminal.len(), 1);
    assert_eq!(
        terminal[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    reply(&mut e, 2101., &candles, "[]");
    assert!(results(&prepared_now(&mut e), 113).is_empty());
}

#[test]
fn a_malformed_dex_list_never_proves_absence() {
    let (mut e, dexes) = partial();
    let actions = ask(&mut e, 1100., 114, "XYZ|GOLD-PERP", Some(100));
    assert!(results(&actions, 114).is_empty());
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1200. => Some(*timer),
            _ => None,
        })
        .unwrap();
    reply(&mut e, 1150., &dexes, r#"[null,{"unexpected":"fixture"}]"#);
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 114).is_empty());
    no_candles(&actions);
    e.handle(1200., Event::TimerFired { timer });
    assert_eq!(
        results(&prepared_now(&mut e), 114)[0]
            .as_ref()
            .unwrap_err()
            .kind,
        Some(ErrorKind::Timeout)
    );
}

#[test]
fn merged_spot_candles_wait_for_the_late_spot_catalogue() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        enable_spot_markets: true,
        ..Default::default()
    }));
    e.start(1000.);
    let actions = prepared_now(&mut e);
    let meta = request(&actions, "metaAndAssetCtxs");
    let spot = request(&actions, "spotMetaAndAssetCtxs");
    reply(&mut e, 1000., &meta, CORE);
    let dexes = request(&prepared_now(&mut e), "perpDexs");
    assert!(results(&ask(&mut e, 1100., 115, "UBTC/USDC", None), 115).is_empty());
    reply(&mut e, 1200., &dexes, "[null]");
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 115).is_empty());
    no_candles(&actions);
    reply(
        &mut e,
        1300.,
        &spot,
        r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"UBTC","index":1,"szDecimals":5,"fullName":"Unit Bitcoin"}],"universe":[{"name":"@1","tokens":[1,0],"index":1}]},[{"coin":"@1","midPx":"60000","markPx":"60010","prevDayPx":"59000","dayNtlVlm":"1000"}]]"#,
    );
    let candles = request(&prepared_now(&mut e), "candleSnapshot");
    assert_eq!(
        candles.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"@1","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
    reply(&mut e, 1400., &candles, "[]");
    assert_eq!(results(&prepared_now(&mut e), 115), vec![Ok("[]".into())]);
}
