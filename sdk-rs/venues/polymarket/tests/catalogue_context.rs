//! Native Gamma cursor geometry under per-page Background Read custody.
use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, Response, TimerId};
use safe_cex_venue_polymarket::{Options, Polymarket, PolymarketOptions};

type Driver = Engine<Polymarket>;
const START: f64 = 10.;
const DEADLINE: f64 = START + 20_000.;
const TOKEN: &str =
    "107505882767731489358349912513945399560393482969656700824895970500493757150417";
fn row(slug: &str, ids: &[&str]) -> serde_json::Value {
    let outcomes: Vec<_> = (0..ids.len()).map(|i| format!("Outcome{i}")).collect();
    serde_json::json!({
        "conditionId":"0x123", "slug":slug, "question":"Question?", "description":"Detail",
        "active":true, "closed":false, "negRisk":true,
        "clobTokenIds":serde_json::to_string(ids).unwrap(),
        "outcomes":serde_json::to_string(&outcomes).unwrap(),
        "orderPriceMinTickSize":0.005, "orderMinSize":5
    })
}
fn page(rows: Vec<serde_json::Value>, cursor: Option<&str>) -> String {
    let mut body = serde_json::json!({"markets": rows});
    if let Some(cursor) = cursor {
        body["next_cursor"] = cursor.into();
    }
    body.to_string()
}
fn good() -> String {
    page(vec![row("election", &[TOKEN, "2"])], None)
}
fn engine(options: Options) -> Driver {
    safe_cex_engine::test_support::seeded_engine(Polymarket::new(options))
}
fn public() -> Driver {
    engine(Options {
        public_data_only: true,
        ..Default::default()
    })
}
fn actions(e: &mut Driver, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn gamma(a: Vec<Action>) -> HttpRequest {
    let mut requests = a.into_iter().filter_map(|a| match a {
        Action::Http(r) if r.url.contains("gamma-api.polymarket.com/markets/keyset") => Some(r),
        _ => None,
    });
    let r = requests.next().expect("released native Gamma page");
    assert!(requests.next().is_none());
    assert_eq!(r.method, Method::Get);
    assert!(r.headers.is_empty());
    assert!(r.body.is_none());
    r
}
fn start(e: &mut Driver) -> HttpRequest {
    e.start(START);
    gamma(actions(e, START))
}
fn answer(e: &mut Driver, r: &HttpRequest, now: f64, status: u16, body: &str) -> Vec<Action> {
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
fn retry(a: &[Action], now: f64) -> TimerId {
    a.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - now)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("first native catalogue backoff")
}
fn no_catalogue(e: &Driver) {
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    assert!(
        e.store()
            .prediction_markets()
            .unwrap_or_default()
            .is_empty()
    );
}
fn no_gamma(a: &[Action]) {
    assert!(!a.iter().any(|a| match a {
        Action::Http(r)=>r.url.contains("gamma-api.polymarket.com"),
        Action::ManagedHttp(a)=>matches!(a.as_ref(),HttpAction::Prepare{request,..} if request.url.contains("gamma-api.polymarket.com")),
        _=>false,
    }));
}
fn no_command_or_sign(a: &[Action]) {
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            | Action::Sign { .. }
    )));
}

#[test]
fn catalogue_requires_prepare_before_any_http_release() {
    let mut e = public();
    e.start(START);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "Gamma escaped before host Prepare/Bind/Claim"
    );
    assert!(a.iter().any(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{request,..} if request.url.contains("/markets/keyset")))));
    no_command_or_sign(&a);
}
#[test]
fn unprepared_page_has_original_admission_deadline_and_late_host_is_inert() {
    let mut e = public();
    e.start(START);
    let a: Vec<_> = e.drain_actions().collect();
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == DEADLINE => Some(*timer),
            _ => None,
        })
        .expect("unprepared Gamma page needs an admission expiry timer");
    let token = a
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.handle(DEADLINE, Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    no_catalogue(&e);
    no_gamma(&a);
    retry(&a, DEADLINE);
    e.http_prepared(
        DEADLINE + 1.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    no_gamma(&a);
    no_command_or_sign(&a);
    assert!(
        !a.iter().any(
            |a| matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Bind{..}))
        )
    );
}
#[test]
fn final_data_at_page_deadline_cannot_publish_or_start_midpoints() {
    let mut e = public();
    let r = start(&mut e);
    let a = answer(&mut e, &r, DEADLINE, 200, &good());
    no_catalogue(&e);
    assert!(!a.iter().any(|a| match a {
        Action::Http(r) => r.url.ends_with("/midpoints"),
        _ => false,
    }));
    retry(&a, DEADLINE);
    no_command_or_sign(&a);
}
#[test]
fn streamed_expired_row_has_no_native_decode_or_skipped_row_effect() {
    let mut e = public();
    let r = start(&mut e);
    e.handle(
        START + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(DEADLINE,Event::HttpChunk{request:r.id,data:br#"{"markets":[{"slug":"broken","clobTokenIds":"not-json","outcomes":"[]"}],"next_cursor":"late"}"#});
    let mut a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.message.contains("Skipped malformed public discovery row"))),"expired row reached the native decoder");
    e.handle(DEADLINE + 1., Event::HttpEnd { request: r.id });
    a.extend(e.drain_actions());
    no_catalogue(&e);
    no_gamma(&a);
    retry(&a, DEADLINE);
}
#[test]
fn second_page_deadline_does_not_commit_first_page_prefix() {
    let mut e = public();
    let first = start(&mut e);
    let second = gamma(answer(
        &mut e,
        &first,
        100.,
        200,
        &page(vec![row("first", &["1"])], Some("next")),
    ));
    no_catalogue(&e);
    let a = answer(
        &mut e,
        &second,
        20_100.,
        200,
        &page(vec![row("second", &["2"])], None),
    );
    no_catalogue(&e);
    no_gamma(&a);
    retry(&a, 20_100.);
}
#[test]
fn wire_preserves_repeated_encoded_slug_filters_and_no_private_intent() {
    let mut e = engine(Options {
        public_data_only: true,
        polymarket: Some(PolymarketOptions {
            market_slugs: Some(vec!["one/a".into(), "two b".into()]),
            ..Default::default()
        }),
        ..Default::default()
    });
    let r = start(&mut e);
    assert_eq!(
        r.url,
        "https://gamma-api.polymarket.com/markets/keyset?active=true&closed=false&limit=100&slug=one%2Fa&slug=two%20b"
    );
    assert!(r.route.is_none());
    let a = answer(&mut e, &r, 20., 200, &good());
    no_command_or_sign(&a);
    assert!(!a.iter().any(|a| matches!(a, Action::WsConnect { .. })));
}
#[test]
fn opaque_cursor_walk_is_atomic_and_metadata_indices_survive_dedup() {
    let mut e = public();
    let first = start(&mut e);
    let second = gamma(answer(
        &mut e,
        &first,
        20.,
        200,
        &page(vec![row("election", &[TOKEN, "2"])], Some("a/b+=opaque")),
    ));
    no_catalogue(&e);
    assert!(second.url.ends_with("&after_cursor=a%2Fb%2B%3Dopaque"));
    assert!(!second.url.contains("offset="));
    let a = answer(
        &mut e,
        &second,
        30.,
        200,
        &page(
            vec![row("election", &[TOKEN, "2"]), row("sports", &["3"])],
            None,
        ),
    );
    assert_eq!(e.store().markets().len(), 3);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets()[0].id, TOKEN);
    let meta = e.store().prediction_markets().unwrap();
    assert_eq!(meta.len(), 3);
    assert_eq!(meta[2].market, 2);
    assert_eq!(meta[2].slug, "sports");
    assert!(meta[0].neg_risk);
    assert_eq!(meta[0].tick_size, 0.005);
    assert_eq!(e.store().markets()[0].precision.price, 3.);
    assert!(
        a.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/midpoints")))
    );
    no_command_or_sign(&a);
}
#[test]
fn page_bounds_do_not_add_a_twenty_second_logical_walk_limit() {
    let mut e = public();
    let first = start(&mut e);
    let second = gamma(answer(
        &mut e,
        &first,
        19_000.,
        200,
        &page(vec![row("first", &["1"])], Some("next")),
    ));
    no_catalogue(&e);
    answer(
        &mut e,
        &second,
        38_000.,
        200,
        &page(vec![row("second", &["2"])], None),
    );
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 2);
}
#[test]
fn later_transport_failure_discards_prefix_and_retries_first_page_after_backoff() {
    let mut e = public();
    let first = start(&mut e);
    let second = gamma(answer(
        &mut e,
        &first,
        20.,
        200,
        &page(vec![row("first", &["1"])], Some("next")),
    ));
    e.handle(
        30.,
        Event::HttpFailed {
            request: second.id,
            error: "transport gone",
        },
    );
    let a = actions(&mut e, 30.);
    no_catalogue(&e);
    no_gamma(&a);
    let timer = retry(&a, 30.);
    e.handle(1030., Event::TimerFired { timer });
    let recovered = gamma(actions(&mut e, 1030.));
    assert_eq!(first.url, recovered.url);
    assert_ne!(first.id, recovered.id);
    answer(
        &mut e,
        &recovered,
        1031.,
        200,
        &page(vec![row("first", &["1"])], None),
    );
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].id, "1");
}
#[test]
fn repeated_cursor_and_invalid_terminal_envelopes_never_publish_prefix() {
    for (body, continued) in [
        (page(vec![row("second", &["2"])], Some("cycle")), true),
        ("{\"markets\":[],\"next_cursor\":\"\"}".into(), false),
        ("{\"markets\":[],\"next_cursor\":false}".into(), false),
        ("{\"error\":\"bad\"}".into(), false),
    ] {
        let mut e = public();
        let first = start(&mut e);
        let second = gamma(answer(
            &mut e,
            &first,
            20.,
            200,
            &page(vec![row("first", &["1"])], Some("cycle")),
        ));
        let a = answer(&mut e, &second, 30., 200, &body);
        no_catalogue(&e);
        no_gamma(&a);
        retry(&a, 30.);
        if continued {
            assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.message.contains("cursor did not advance"))));
        }
    }
}
#[test]
fn full_missing_cursor_and_oversized_pages_keep_native_failure_policy() {
    for count in [100, 101] {
        let mut e = public();
        let r = start(&mut e);
        let body = page((0..count).map(|_| row("same", &["1"])).collect(), None);
        let a = answer(&mut e, &r, 20., 200, &body);
        no_catalogue(&e);
        no_gamma(&a);
        retry(&a, 20.);
    }
}
#[test]
fn malformed_row_preserves_valid_neighbour_and_empty_catalogue_retries() {
    let mut e = public();
    // Skipped discovery rows log at Debug.
    e.set_log_level(safe_cex_engine::Level::Debug);
    let r = start(&mut e);
    let body = page(
        vec![
            serde_json::json!({"slug":"bad","clobTokenIds":"broken","outcomes":"[]"}),
            row("election", &[TOKEN]),
        ],
        None,
    );
    let a = answer(&mut e, &r, 20., 200, &body);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.message.contains("Skipped malformed public discovery row"))));
    let mut empty = public();
    let r = start(&mut empty);
    let a = answer(&mut empty, &r, 20., 200, "{\"markets\":[]}");
    no_catalogue(&empty);
    retry(&a, 20.);
}

#[test]
fn catalogue_retry_and_midpoint_timeout_keep_separate_current_owners() {
    let mut e = public();
    let first = start(&mut e);
    e.handle(
        20.,
        Event::HttpFailed {
            request: first.id,
            error: "catalogue transport",
        },
    );
    let failed = actions(&mut e, 20.);
    no_catalogue(&e);
    no_gamma(&failed);
    let catalogue_retry = retry(&failed, 20.);
    e.handle(
        1020.,
        Event::TimerFired {
            timer: catalogue_retry,
        },
    );
    let current = gamma(actions(&mut e, 1020.));
    let completed = answer(&mut e, &current, 1021., 200, &good());
    assert!(e.store().loaded().markets);
    let midpoint = completed
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/midpoints") => Some(r),
            _ => None,
        })
        .expect("catalogue completion releases the first managed midpoint");
    e.handle(
        1022.,
        Event::HttpFailed {
            request: first.id,
            error: "duplicate old catalogue failure",
        },
    );
    let stale = actions(&mut e, 1022.);
    assert!(!stale.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(e.store().tickers().is_empty());
    let deadline = 1021. + 20_000.;
    let timed_out = answer(&mut e, &midpoint, deadline, 200, r#"{"2":"0.4"}"#);
    assert!(e.store().tickers().is_empty());
    assert!(!e.store().loaded().tickers);
    no_gamma(&timed_out);
    let midpoint_retry = retry(&timed_out, deadline);
    e.handle(
        deadline + 1.,
        Event::HttpFailed {
            request: midpoint.id,
            error: "duplicate old midpoint failure",
        },
    );
    let duplicate = actions(&mut e, deadline + 1.);
    assert!(
        !duplicate
            .iter()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    e.handle(
        deadline + 1000.,
        Event::TimerFired {
            timer: midpoint_retry,
        },
    );
    let retry_actions = actions(&mut e, deadline + 1000.);
    no_gamma(&retry_actions);
    let retry = retry_actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/midpoints") => Some(r),
            _ => None,
        })
        .expect("only the failed midpoint offset retries");
    assert_ne!(retry.id, midpoint.id);
    let body = format!(r#"{{"{TOKEN}":"0.5","2":"0.4"}}"#);
    let done = answer(&mut e, &retry, deadline + 1001., 200, &body);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 2);
    no_gamma(&done);
    no_command_or_sign(&done);
}
