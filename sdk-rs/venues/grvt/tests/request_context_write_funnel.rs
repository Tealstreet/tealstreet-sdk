// Original write-funnel controls. Native/reply-ingress probes may release raw
// requests; managed-custody probes require actual Prepare and never fabricate it.
include!("trading.rs");

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
};
use safe_cex_engine::{Capabilities, Cx, ErrorKind, Outcome, Venue, VenueError};
use safe_cex_venue_grvt::Pending;
const WF_PEER: &str = "__grvt_write_peer";
const WF_RETIRE: &str = "__grvt_write_retire";
const WF_OWNER: u32 = 700;
struct WfProbe {
    venue: Grvt,
    roots: Vec<Context>,
    validated: usize,
}
impl Venue for WfProbe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.venue.cancel_needs_stored_order()
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.on_public_feed(cx)
    }
    fn guard_pending_placements(&self) -> bool {
        self.venue.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.venue.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.venue.fence_uncertain_cancel_all()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.venue
            .on_http_preparation_failed(cx, pending, reason, error)
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.venue.on_stream_error(cx, pending, response)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer)
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == WF_PEER => {
                if let Ok(root) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.roots.push(root);
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["validate_peers"] => {
                for root in &self.roots {
                    cx.validate_http_context(root).unwrap();
                }
                self.validated = self.roots.len();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == [WF_RETIRE] => {
                let root = self.roots.remove(0);
                cx.fail_context(&root, "retired fixture peer").unwrap();
            }
            _ => self.venue.on_command(cx, command),
        }
    }
}

fn wf_drain(e: &mut Engine<WfProbe>) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(e, NOW + 3.).into_iter()
}
fn wf_boot_with_builders(
    catalog: &str,
    builders: Option<&str>,
) -> (Engine<WfProbe>, SocketId, Option<HttpRequest>) {
    let mut private_socket = None;
    let mut held = None;
    let mut e = safe_cex_engine::test_support::seeded_engine(WfProbe {
        venue: Grvt::new(Options {
            key: Some("grvt-test-key".into()),
            secret: Some(format!("0x{}", "01".repeat(32))),
            trading_account_id: Some("9007199254740993".into()),
            session_proxy: true,
            ..Options::default()
        }),
        roots: vec![],
        validated: 0,
    });
    e.start(NOW);
    let mut pending: VecDeque<_> = wf_drain(&mut e)
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    for _ in 0..30 {
        let Some(request) = pending.pop_front() else {
            break;
        };
        let (headers, body) = if request.url.ends_with("all_instruments") {
            ("", catalog)
        } else if request.url.ends_with("auth/api_key/login") {
            (
                "Set-Cookie: gravity=test-session; Path=/\nX-Grvt-Account-Id: test-account",
                r#"{"sub_account_id":"9007199254740993"}"#,
            )
        } else if request.url.ends_with("aggregated_account_summary") {
            (
                "",
                r#"{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"9007199254740993"}]}}"#,
            )
        } else if request.url.ends_with("get_authorized_builders") {
            let Some(builders) = builders else {
                held = Some(request);
                continue;
            };
            ("", builders)
        } else if request.url.ends_with("get_all_initial_leverage") {
            ("", r#"{"r":[{"i":"BTC_USDT_Perp","l":"10"}]}"#)
        } else if request.url.ends_with("open_orders") {
            ("", OPEN_ORDERS)
        } else {
            ("", r#"{"r":[]}"#)
        };
        e.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers,
                    body,
                },
            },
        );
        for action in wf_drain(&mut e) {
            match action {
                Action::Http(request) => pending.push_back(request),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => private_socket = Some(socket),
                _ => {}
            }
        }
    }
    assert!(pending.is_empty());
    (e, private_socket.expect("GRVT private socket"), held)
}
fn wf_place(id: u32) -> Command {
    Command::PlaceOrders {
        command: id,
        orders: vec![opts()],
    }
}
fn wf_command<V: Venue>(e: &mut Engine<V>, command: Command) -> Vec<Action> {
    e.handle(NOW + 4., Event::Command(command));
    e.drain_actions().collect()
}
fn wf_prep(actions: &[Action], path: &str) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with(path))),
        "write-funnel call must prepare before transport"
    );
    let mut prepares = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(a) => match &**a {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } if request.url.ends_with(path) => Some((*preparation, request.clone())),
            _ => None,
        },
        _ => None,
    });
    let found = prepares.next().expect("one genuine write-funnel Prepare");
    assert!(prepares.next().is_none());
    found
}
fn wf_bind(
    e: &mut Engine<Grvt>,
    now: f64,
    p: Preparation,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation, p);
                    Some((permit, request))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("actual Bind; provider fixture is a later source gate for signed calls")
}
fn wf_reply_at<V: Venue>(
    e: &mut Engine<V>,
    now: f64,
    request: &HttpRequest,
    status: u16,
    text: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body: text,
            },
        },
    );
    e.drain_actions().collect()
}
fn wf_results(actions: &[Action], id: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn wf_no_trade_io(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| match a {
            Action::Http(r) => r.url.contains("/lite/v1/") && !r.url.ends_with("all_instruments"),
            Action::ManagedHttp(a) =>
                matches!(&**a, HttpAction::Prepare { .. } | HttpAction::Bind { .. }),
            Action::Sign { .. } => true,
            _ => false,
        }),
        "no queued call/provider/transport work on refusal"
    );
}
#[test]
fn wf_placement_starts_with_managed_prepare() {
    let mut e = boot();
    let a = wf_command(&mut e, wf_place(WF_OWNER));
    wf_prep(&a, "create_order");
}
#[test]
fn wf_cancel_starts_with_managed_prepare() {
    let mut e = boot();
    seed(&mut e);
    let a = wf_command(
        &mut e,
        Command::CancelOrders {
            command: WF_OWNER,
            ids: vec!["42".into()],
        },
    );
    wf_prep(&a, "cancel_order");
}
#[test]
fn wf_symbol_cancel_read_starts_with_managed_prepare() {
    let mut e = boot();
    let a = wf_command(
        &mut e,
        Command::CancelSymbolOrders {
            command: WF_OWNER,
            symbol: "BTC_USDT_PERP".into(),
        },
    );
    wf_prep(&a, "open_orders");
}
#[test]
fn wf_amend_inspection_starts_with_managed_prepare() {
    let mut e = boot();
    seed(&mut e);
    let a = wf_command(
        &mut e,
        Command::AmendOrder {
            command: WF_OWNER,
            id: "42".into(),
            price: Some(110.),
            amount: None,
        },
    );
    wf_prep(&a, "/order");
}
#[test]
fn wf_leverage_starts_with_managed_prepare() {
    let mut e = boot();
    let a = wf_command(
        &mut e,
        Command::SetLeverage {
            command: WF_OWNER,
            symbol: "BTC_USDT_PERP".into(),
            leverage: 20.,
            side: None,
        },
    );
    wf_prep(&a, "set_initial_leverage");
}
#[test]
fn wf_bracket_continuation_gets_its_own_prepare() {
    let mut e = boot();
    let mut o = opts();
    o.stop_loss = Some(90.);
    o.client_order_id = None;
    let first = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: WF_OWNER,
                orders: vec![o],
            },
        ),
        "create_order",
    );
    let a = wf_reply_at(&mut e, NOW + 5., &first, 200, r#"{"r":{"oi":"0x00"}}"#);
    wf_prep(&a, "create_order");
    assert!(wf_results(&a, WF_OWNER).is_empty());
}
#[test]
fn wf_fifo_wake_from_another_commands_data_prepares_the_new_owner() {
    let mut e = boot();
    let first = http(command(&mut e, wf_place(WF_OWNER)), "create_order");
    let a = wf_command(
        &mut e,
        Command::SetLeverage {
            command: WF_OWNER + 1,
            symbol: "BTC_USDT_PERP".into(),
            leverage: 20.,
            side: None,
        },
    );
    wf_no_trade_io(&a);
    let a = wf_reply_at(&mut e, NOW + 5., &first, 200, r#"{"r":{"oi":"0x00"}}"#);
    assert_eq!(wf_results(&a, WF_OWNER).len(), 1);
    assert!(wf_results(&a, WF_OWNER + 1).is_empty());
    wf_prep(&a, "set_initial_leverage");
}
#[test]
fn wf_unclaimed_create_data_cannot_finish_the_command() {
    let mut e = boot();
    let actions = wf_command(&mut e, wf_place(WF_OWNER));
    // Native raw compatibility is only reply ingress; a managed request is
    // inspected directly without preparing, binding or claiming it.
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("create_order") => Some(r.clone()),
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare { request, .. } if request.url.ends_with("create_order") => {
                    Some(request.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("native raw or genuinely unclaimed managed request");
    let a = wf_reply_at(&mut e, NOW + 5., &request, 200, r#"{"r":{"oi":"0x00"}}"#);
    assert!(
        wf_results(&a, WF_OWNER).is_empty(),
        "unclaimed DATA grants no command completion"
    );
}

#[test]
fn wf_new_call_bound_is_twenty_seconds_from_admission() {
    let mut e = boot();
    let a = wf_command(&mut e, wf_place(WF_OWNER));
    let (_, r) = wf_prep(&a, "create_order");
    assert_eq!(r.timeout_ms, Some(20_000));
}
#[test]
fn wf_held_unsigned_prepare_expires_without_sending() {
    let mut e = boot();
    let a = wf_command(
        &mut e,
        Command::CancelSymbolOrders {
            command: WF_OWNER,
            symbol: "BTC_USDT_PERP".into(),
        },
    );
    let (_, r) = wf_prep(&a, "open_orders");
    let a = wf_reply_at(&mut e, NOW + 20_004., &r, 200, OPEN_ORDERS);
    wf_no_trade_io(&a);
    let outcomes = wf_results(&a, WF_OWNER);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].as_ref().unwrap_err().outcome,
        Some(Outcome::NotSent)
    );
}
#[test]
fn wf_unsigned_bind_cannot_claim_at_the_new_call_deadline() {
    let mut e = boot();
    let a = wf_command(
        &mut e,
        Command::CancelSymbolOrders {
            command: WF_OWNER,
            symbol: "BTC_USDT_PERP".into(),
        },
    );
    let (p, _) = wf_prep(&a, "open_orders");
    let (permit, r) = wf_bind(&mut e, NOW + 5., p);
    assert!(r.timeout_ms.unwrap() <= 20_000);
    assert!(!matches!(
        e.claim_http(NOW + 20_004., permit),
        Ok(Claim::Released(_))
    ));
}
fn wf_capacity(invalid: bool) {
    let (mut e, _, _) = wf_boot_with_builders(CATALOG, Some(AUTHORIZED));
    for command in 1000..1256 {
        e.handle_command(
            NOW + 4.,
            Command::FetchOrderBook {
                command,
                symbol: WF_PEER.into(),
                limit: None,
            },
            Some(60_000),
        )
        .unwrap();
        assert!(e.drain_actions().next().is_none());
    }
    assert_eq!(e.venue().roots.len(), 256);
    let mut o = opts();
    if invalid {
        o.amount = 0.;
    }
    let cmd = Command::PlaceOrders {
        command: WF_OWNER,
        orders: vec![o.clone()],
    };
    let a = wf_command(&mut e, cmd.clone());
    wf_no_trade_io(&a);
    let outcomes = wf_results(&a, WF_OWNER);
    assert_eq!(outcomes.len(), 1);
    let error = outcomes[0]
        .as_ref()
        .expect_err("shared capacity owns the terminal");
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(e.store().order("9223372036854775807").is_none());
    e.handle(
        NOW + 4.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![WF_RETIRE.into()],
        }),
    );
    assert_eq!(
        wf_results(&e.drain_actions().collect::<Vec<_>>(), 1000).len(),
        1
    );
    let a = wf_command(&mut e, cmd);
    if invalid {
        let r = wf_results(&a, WF_OWNER);
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::InvalidInput)
        );
    } else {
        wf_prep(&a, "create_order");
    }
    e.handle(
        NOW + 4.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        }),
    );
    assert_eq!(e.venue().validated, 255);
}
#[test]
fn wf_capacity_precedes_valid_placement_and_recovers() {
    wf_capacity(false);
}
#[test]
fn wf_capacity_precedes_invalid_planning_and_recovers() {
    wf_capacity(true);
}
fn wf_status_case(status: u16, code: u32) {
    let mut e = boot();
    let original = http(command(&mut e, wf_place(WF_OWNER)), "create_order");
    let text = format!(r#"{{"c":{code},"m":"controlled refusal"}}"#);
    let a = reply(&mut e, &original, status, &text);
    if code != 1000 {
        assert_eq!(wf_results(&a, WF_OWNER).len(), 1);
        assert_eq!(
            wf_results(&a, WF_OWNER)[0].as_ref().unwrap_err().outcome,
            Some(if status < 500 {
                Outcome::Rejected
            } else {
                Outcome::Unknown
            })
        );
        assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.url.ends_with("auth/api_key/login") || r.url.ends_with("create_order"))));
        return;
    }
    assert!(wf_results(&a, WF_OWNER).is_empty());
    let login = http(a, "auth/api_key/login");
    e.handle(
        NOW + 4.,
        Event::HttpResponse {
            request: login.id,
            response: Response {
                status: 200,
                headers: "Set-Cookie: gravity=renewed; Path=/\nX-Grvt-Account-Id: rotated",
                body: r#"{"sub_account_id":"9007199254740993"}"#,
            },
        },
    );
    let a = drain(&mut e).collect::<Vec<_>>();
    let resent = http(a, "create_order");
    assert_eq!(original.body, resent.body);
    assert_ne!(original.id, resent.id);
    assert!(
        resent
            .headers
            .iter()
            .any(|(k, v)| *k == "X-Grvt-Account-Id" && v == "rotated")
    );
    let a = reply(&mut e, &resent, status, &text);
    assert_eq!(wf_results(&a, WF_OWNER).len(), 1);
    assert_eq!(
        wf_results(&a, WF_OWNER)[0].as_ref().unwrap_err().outcome,
        Some(if status < 500 {
            Outcome::Rejected
        } else {
            Outcome::Unknown
        })
    );
    assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.url.ends_with("auth/api_key/login") || r.url.ends_with("create_order"))));
}
macro_rules! wf_status_control {
    ($name:ident,$status:expr,$code:expr) => {
        #[test]
        fn $name() {
            wf_status_case($status, $code);
        }
    };
}
wf_status_control!(wf_401_parsed_1000_resends_identical_body_once, 401, 1000);
wf_status_control!(wf_408_parsed_1000_resends_identical_body_once, 408, 1000);
wf_status_control!(wf_503_parsed_1000_resends_identical_body_once, 503, 1000);
wf_status_control!(wf_401_without_1000_never_resends, 401, 1001);
wf_status_control!(wf_408_without_1000_never_resends, 408, 1001);
wf_status_control!(wf_503_without_1000_never_resends, 503, 1001);
#[test]
fn wf_native_headers_keep_content_type_first() {
    let mut e = boot();
    let r = http(command(&mut e, wf_place(WF_OWNER)), "create_order");
    assert_eq!(
        r.headers,
        vec![
            ("Content-Type", "application/json".into()),
            ("X-Ts-Grvt-Session", "test-session".into()),
            ("Cookie", "rm=true; gravity=test-session".into()),
            ("X-Grvt-Account-Id", "test-account".into())
        ]
    );
}
#[test]
fn wf_native_read_has_six_immediate_network_attempts_and_no_seventh() {
    let mut e = boot();
    seed(&mut e);
    let mut r = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: WF_OWNER,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    for attempt in 0..6 {
        e.handle(
            NOW + 5. + f64::from(attempt),
            Event::HttpFailed {
                request: r.id,
                error: "network unavailable",
            },
        );
        let a = drain(&mut e).collect::<Vec<_>>();
        if attempt == 5 {
            assert_eq!(wf_results(&a, WF_OWNER).len(), 1);
            assert!(wf_results(&a, WF_OWNER)[0].is_err());
            assert!(
                !a.iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/order")))
            );
            assert!(!a.iter().any(|a| matches!(a, Action::ManagedHttp(x) if matches!(&**x, HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
        } else {
            assert!(wf_results(&a, WF_OWNER).is_empty());
            let next = http(a, "/order");
            assert_ne!(next.id, r.id);
            r = next;
        }
    }
}
#[test]
fn wf_native_partial_progress_strips_later_refusal_outcome() {
    let mut e = boot();
    let mut a = opts();
    a.client_order_id = Some("111".into());
    let mut b = opts();
    b.client_order_id = Some("112".into());
    let first = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: WF_OWNER,
                orders: vec![a, b],
            },
        ),
        "create_order",
    );
    let second = http(
        reply(&mut e, &first, 200, r#"{"r":{"oi":"0x00"}}"#),
        "create_order",
    );
    let a = reply(&mut e, &second, 400, r#"{"c":1001,"m":"later refusal"}"#);
    let outcomes = wf_results(&a, WF_OWNER);
    assert_eq!(outcomes.len(), 1);
    let error = outcomes[0].as_ref().unwrap_err();
    assert_eq!(error.outcome, None);
    assert!(error.message.contains("111"));
    assert!(!error.message.contains("112"));
}

#[test]
fn wf_native_auth_spend_survives_a_read_network_retry() {
    let mut e = boot();
    seed(&mut e);
    let original = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: WF_OWNER,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    let login = http(
        reply(
            &mut e,
            &original,
            503,
            r#"{"c":1000,"m":"session expired"}"#,
        ),
        "auth/api_key/login",
    );
    e.handle(
        NOW + 4.,
        Event::HttpResponse {
            request: login.id,
            response: Response {
                status: 200,
                headers: "Set-Cookie: gravity=renewed; Path=/\nX-Grvt-Account-Id: rotated",
                body: r#"{"sub_account_id":"9007199254740993"}"#,
            },
        },
    );
    let resent = http(
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 4.),
        "/order",
    );
    assert_eq!(original.body, resent.body);
    e.handle(
        NOW + 5.,
        Event::HttpFailed {
            request: resent.id,
            error: "network unavailable",
        },
    );
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 5.);
    assert!(wf_results(&a, WF_OWNER).is_empty());
    let retry = http(a, "/order");
    assert_eq!(retry.body, original.body);
    let a = wf_reply_at(
        &mut e,
        NOW + 6.,
        &retry,
        408,
        r#"{"c":1000,"m":"expired again"}"#,
    );
    assert_eq!(wf_results(&a, WF_OWNER).len(), 1);
    assert!(wf_results(&a, WF_OWNER)[0].is_err());
    assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.url.ends_with("auth/api_key/login") || r.url.ends_with("/order"))));
}
#[test]
fn wf_native_unanswered_mutation_is_unknown_and_never_resent() {
    let mut e = boot();
    let r = http(command(&mut e, wf_place(WF_OWNER)), "create_order");
    e.handle(
        NOW + 5.,
        Event::HttpFailed {
            request: r.id,
            error: "network unavailable",
        },
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    let outcomes = wf_results(&a, WF_OWNER);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.url.ends_with("auth/api_key/login") || r.url.ends_with("create_order"))));
}

fn wf_unparsed_status(status: u16) {
    let mut e = boot();
    let r = http(command(&mut e, wf_place(WF_OWNER)), "create_order");
    let a = reply(&mut e, &r, status, "unparsed upstream response");
    let outcomes = wf_results(&a, WF_OWNER);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.url.ends_with("auth/api_key/login") || r.url.ends_with("create_order"))));
}
#[test]
fn wf_unparsed_408_is_unknown_without_resend() {
    wf_unparsed_status(408);
}
#[test]
fn wf_unparsed_503_is_unknown_without_resend() {
    wf_unparsed_status(503);
}
#[test]
fn wf_native_auth_resend_does_not_reset_spent_network_budget() {
    let mut e = boot();
    seed(&mut e);
    let mut request = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: WF_OWNER,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    for n in 0..5 {
        let now = NOW + 5. + f64::from(n);
        e.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "network unavailable",
            },
        );
        let a = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        assert!(wf_results(&a, WF_OWNER).is_empty());
        request = http(a, "/order");
    }
    e.handle(
        NOW + 10.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 408,
                headers: "",
                body: r#"{"c":1000,"m":"session expired"}"#,
            },
        },
    );
    let login = http(
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 10.),
        "auth/api_key/login",
    );
    e.handle(
        NOW + 11.,
        Event::HttpResponse {
            request: login.id,
            response: Response {
                status: 200,
                headers: "Set-Cookie: gravity=renewed; Path=/\nX-Grvt-Account-Id: rotated",
                body: r#"{"sub_account_id":"9007199254740993"}"#,
            },
        },
    );
    let resent = http(
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 11.),
        "/order",
    );
    assert_eq!(resent.body, request.body);
    e.handle(
        NOW + 12.,
        Event::HttpFailed {
            request: resent.id,
            error: "network unavailable",
        },
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(wf_results(&a, WF_OWNER).len(), 1);
    assert!(wf_results(&a, WF_OWNER)[0].is_err());
    assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/order") || r.url.ends_with("auth/api_key/login"))));
}
