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

fn merged_spot_read() -> (Engine<Hyperliquid>, HttpRequest) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        enable_spot_markets: true,
        ..Default::default()
    }));
    e.start(1000.);
    let actions = prepared_now(&mut e);
    let core = request(&actions, "metaAndAssetCtxs");
    let spot = request(&actions, "spotMetaAndAssetCtxs");
    reply(&mut e, 1000., &core, CORE);
    let dexes = request(&prepared_now(&mut e), "perpDexs");
    reply(&mut e, 1050., &dexes, "[null]");
    prepared_now(&mut e);
    (e, spot)
}

fn two_spot_rows(contexts: serde_json::Value) -> String {
    serde_json::json!([
        {
            "tokens": [
                {"name":"USDC", "index":0, "szDecimals":8},
                {"name":"UBTC", "index":1, "szDecimals":5},
                {"name":"UETH", "index":2, "szDecimals":4}
            ],
            "universe": [
                {"name":"@11", "index":11, "tokens":[1,0]},
                {"name":"@22", "index":22, "tokens":[2,0]}
            ]
        }, contexts
    ])
    .to_string()
}

fn partial_spot_read_keeps_its_bound(contexts: serde_json::Value) {
    let (mut e, spot) = merged_spot_read();
    let actions = ask(&mut e, 1100., 116, "UBTC/USDC", Some(1000));
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 2100. => Some(*timer),
            _ => None,
        })
        .expect("original candle discovery deadline");
    reply(&mut e, 1200., &spot, &two_spot_rows(contexts));
    let actions = prepared_now(&mut e);
    assert!(
        e.store().market("UETH/USDC").is_some(),
        "valid sibling publishes"
    );
    assert!(e.store().market("UBTC/USDC").is_none());
    assert!(
        results(&actions, 116).is_empty(),
        "publication does not establish the requested row's absence"
    );
    no_candles(&actions);

    let known = request(
        &ask(&mut e, 1201., 117, "UETH/USDC", None),
        "candleSnapshot",
    );
    assert!(known.body.as_deref().unwrap().contains(r#""coin":"@22""#));
    reply(&mut e, 1202., &known, "[]");
    assert_eq!(results(&prepared_now(&mut e), 117), vec![Ok("[]".into())]);
    e.handle(2100., Event::TimerFired { timer });
    let actions = prepared_now(&mut e);
    let terminal = results(&actions, 116);
    assert_eq!(terminal.len(), 1);
    assert_eq!(
        terminal[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    no_candles(&actions);
}

#[test]
fn merged_spot_publication_does_not_settle_a_malformed_requested_context() {
    partial_spot_read_keeps_its_bound(serde_json::json!([
        {"coin":"@11", "midPx":"malformed"},
        {"coin":"@22", "midPx":"3000", "markPx":"3001"}
    ]));
}

#[test]
fn merged_spot_publication_does_not_settle_a_missing_requested_context() {
    partial_spot_read_keeps_its_bound(serde_json::json!([
        {"coin":"@22", "midPx":"3000", "markPx":"3001"}
    ]));
}

#[test]
fn a_complete_merged_spot_catalogue_releases_its_row_and_refuses_absent_rows() {
    let (mut e, spot) = merged_spot_read();
    assert!(results(&ask(&mut e, 1100., 118, "UBTC/USDC", Some(1000)), 118).is_empty());
    assert!(results(&ask(&mut e, 1100., 119, "REMOVED/USDC", Some(1000)), 119).is_empty());
    reply(
        &mut e,
        1200.,
        &spot,
        &two_spot_rows(serde_json::json!([
            {"coin":"@11", "midPx":"60000", "markPx":"60010"},
            {"coin":"@22", "midPx":"3000", "markPx":"3001"}
        ])),
    );
    let actions = prepared_now(&mut e);
    assert!(results(&actions, 118).is_empty());
    let terminal = results(&actions, 119);
    assert_eq!(terminal.len(), 1);
    assert_eq!(
        terminal[0].as_ref().unwrap_err().message,
        "Market not found"
    );
    let candles = request(&actions, "candleSnapshot");
    assert!(candles.body.as_deref().unwrap().contains(r#""coin":"@11""#));
    reply(&mut e, 1201., &candles, "[]");
    assert_eq!(results(&prepared_now(&mut e), 118), vec![Ok("[]".into())]);
    assert_missing(&ask(&mut e, 1202., 120, "REMOVED/USDC", None), 120);
}

#[test]
fn an_excluded_merged_spot_row_refuses_while_its_sibling_is_unresolved() {
    let (mut e, spot) = merged_spot_read();
    assert!(results(&ask(&mut e, 1100., 121, "UBTC/USDC", Some(1000)), 121).is_empty());
    reply(
        &mut e,
        1200.,
        &spot,
        &two_spot_rows(serde_json::json!([
            {"coin":"@11", "midPx":null},
            {"coin":"@22", "midPx":"malformed"}
        ])),
    );
    assert_missing(&prepared_now(&mut e), 121);
    assert_missing(&ask(&mut e, 1201., 122, "UBTC/USDC", None), 122);
    assert!(results(&ask(&mut e, 1202., 123, "UETH/USDC", Some(1000)), 123).is_empty());
}

fn standalone_spot_read(eth_price: &str) -> Engine<Hyperliquid> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.);
    let spot = request(&prepared_now(&mut e), "spotMetaAndAssetCtxs");
    let mut catalogue: serde_json::Value =
        serde_json::from_str(&two_spot_rows(serde_json::json!([
            {"coin":"@11", "midPx":null},
            {"coin":"@22", "midPx":eth_price},
            {"coin":"@33", "midPx":"150", "markPx":"151"}
        ])))
        .unwrap();
    catalogue[0]["tokens"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"SOL", "index":3, "szDecimals":2}));
    catalogue[0]["universe"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"name":"@33", "index":33, "tokens":[3,0]}));
    reply(&mut e, 1050., &spot, &catalogue.to_string());
    prepared_now(&mut e);
    assert!(e.store().market("SOL/USDC").is_some());
    e
}

#[test]
fn standalone_spot_exclusion_refuses_despite_a_malformed_sibling() {
    let mut e = standalone_spot_read("malformed");
    assert!(e.store().market("UBTC/USDC").is_none());
    assert_missing(&ask(&mut e, 1100., 124, "UBTC/USDC", Some(100)), 124);
}

#[test]
fn standalone_spot_settled_exclusion_stays_immediate() {
    let mut e = standalone_spot_read("3000");
    assert_missing(&ask(&mut e, 1100., 125, "UBTC/USDC", Some(100)), 125);
}

#[test]
fn standalone_spot_usable_sibling_reads_while_unresolved_row_keeps_its_deadline() {
    let mut e = standalone_spot_read("malformed");
    let waiting = ask(&mut e, 1100., 126, "UETH/USDC", Some(100));
    assert!(results(&waiting, 126).is_empty());
    no_candles(&waiting);
    let timer = waiting
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1200. => Some(*timer),
            _ => None,
        })
        .expect("original standalone discovery deadline");
    let candles = request(
        &ask(&mut e, 1101., 127, "SOL/USDC", Some(100)),
        "candleSnapshot",
    );
    assert!(candles.body.as_deref().unwrap().contains(r#""coin":"@33""#));
    reply(&mut e, 1102., &candles, "[]");
    assert_eq!(results(&prepared_now(&mut e), 127), vec![Ok("[]".into())]);
    e.handle(1200., Event::TimerFired { timer });
    let actions = prepared_now(&mut e);
    let terminal = results(&actions, 126);
    assert_eq!(terminal.len(), 1);
    assert_eq!(
        terminal[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    no_candles(&actions);
}
