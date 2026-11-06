//! TS fetchAccountLimits(): coalesced reads, 5m cache, 60s failure cooldown.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, TimerId};
use safe_cex_venue_lighter::Lighter;
use serde_json::json;
use support::Options;
use support::*;

struct Client {
    engine: Engine<Lighter>,
    timers: Vec<TimerId>,
}
impl Client {
    fn new(options: Options) -> Self {
        let mut client = Self {
            engine: support::engine(options),
            timers: vec![],
        };
        client.engine.start(NOW);
        for _ in 0..30 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut client.engine, NOW);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    Action::SetTimer { timer, at_ms } if at_ms == NOW + 2_000.0 => {
                        client.timers.push(timer)
                    }
                    Action::WsConnect { socket, .. } => {
                        client.engine.handle(NOW, Event::WsOpened { socket })
                    }
                    Action::Http(request) => {
                        assert!(
                            !request.url.contains("accountLimits"),
                            "startup skips limits"
                        );
                        client.answer(&request, NOW);
                    }
                    _ => {}
                }
            }
        }
        client
    }
    fn answer(&mut self, request: &HttpRequest, now: f64) {
        let body = if request.url.contains("orderBookDetails") {
            META
        } else if request.url.contains("accountActiveOrders") {
            OPEN
        } else if request.url.contains("exchangeStats") {
            r#"{"code":200,"order_book_stats":[]}"#
        } else if request.url.contains("/account?") {
            ACCOUNT
        } else {
            panic!("unexpected background read {}", request.url)
        };
        reply_at(&mut self.engine, now, request, 200, body);
    }
    fn tick(&mut self, offset: f64) -> Vec<HttpRequest> {
        let now = NOW + offset;
        for timer in std::mem::take(&mut self.timers) {
            self.engine.handle(now, Event::TimerFired { timer });
        }
        let mut limits = vec![];
        for _ in 0..10 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut self.engine, now);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    Action::SetTimer { timer, at_ms } if at_ms == now + 2_000.0 => {
                        self.timers.push(timer)
                    }
                    Action::Http(request) if request.url.contains("accountLimits") => {
                        limits.push(request)
                    }
                    Action::Http(request) => self.answer(&request, now),
                    _ => {}
                }
            }
        }
        limits
    }
    fn place(&mut self, offset: f64) -> Vec<Action> {
        self.engine.handle(
            NOW + offset,
            Event::Command(Command::PlaceOrders {
                command: 51,
                orders: vec![
                    serde_json::from_value(
                        json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}),
                    )
                    .unwrap(),
                ],
            }),
        );
        safe_cex_engine::test_support::prepared_actions(&mut self.engine, NOW + offset)
    }
}
#[test]
fn local_signers_poll_authenticated_limits_on_every_deployment_but_remote_only_never_does() {
    for (robinhood, testnet, host) in [
        (false, false, "https://mainnet.zklighter.elliot.ai"),
        (false, true, "https://testnet-api.lighter.xyz"),
        (true, false, "https://api.rh.lighter.xyz"),
        (true, true, "https://api.rh-testnet.lighter.xyz"),
    ] {
        let mut client = Client::new(Options {
            robinhood,
            testnet,
            ..options()
        });
        let reads = client.tick(2_000.0);
        assert_eq!(reads.len(), 1);
        assert_eq!(
            reads[0].url,
            format!("{host}/api/v1/accountLimits?account_index=718895")
        );
        assert!(
            reads[0]
                .headers
                .iter()
                .any(|(key, token)| *key == "authorization" && !token.is_empty())
        );
        assert!(client.tick(4_000.0).is_empty(), "coalesces held limit read");
        let mut remote = Client::new(Options {
            robinhood,
            testnet,
            remote_signer: true,
            secret: None,
            ..options()
        });
        assert!(remote.tick(2_000.0).is_empty());
        assert!(remote.tick(64_000.0).is_empty());
    }
}
#[test]
fn successful_cache_expires_from_request_start_and_failures_keep_tier_for_sixty_seconds() {
    let mut client = Client::new(options());
    let read = client.tick(2_000.0).pop().unwrap();
    reply_at(
        &mut client.engine,
        NOW + 6_000.0,
        &read,
        200,
        r#"{"code":200,"data":{"account":{"userTierName":" STD "}}}"#,
    );
    assert!(client.tick(300_000.0).is_empty());
    let refresh = client.tick(302_000.0).pop().unwrap();
    client.engine.handle(
        NOW + 302_000.0,
        Event::HttpFailed {
            request: refresh.id,
            error: "transport lost",
        },
    );
    assert!(client.tick(304_000.0).is_empty());
    let actions = client.place(304_000.0);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 51, error }) if error.message.contains("Plus or Premium"))));
    assert_eq!(client.tick(362_000.0).len(), 1);
}
#[test]
fn trading_waits_for_periodic_limits_and_consumes_that_reply_without_a_duplicate_read() {
    let mut client = Client::new(options());
    let read = client.tick(2_000.0).pop().unwrap();
    assert!(
        !client
            .place(2_001.0)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    reply_at(
        &mut client.engine,
        NOW + 2_002.0,
        &read,
        200,
        r#"{"code":200,"user_tier_name":"premium"}"#,
    );
    let reads = requests(&mut client.engine);
    assert_eq!(reads.len(), 1);
    assert!(
        reads[0].url.contains("/account?"),
        "cached eligible tier proceeds to approval"
    );
}
#[test]
fn malformed_or_rejected_limit_replies_retry_after_one_minute_without_repeated_polling() {
    for (status, body) in [
        (500, r#"{"code":200,"user_tier_name":"premium"}"#),
        (200, r#"{"code":401}"#),
        (200, "malformed"),
    ] {
        let mut client = Client::new(options());
        let read = client.tick(2_000.0).pop().unwrap();
        reply_at(&mut client.engine, NOW + 3_000.0, &read, status, body);
        assert!(client.tick(60_000.0).is_empty());
        assert_eq!(client.tick(62_000.0).len(), 1);
    }
}

#[test]
fn root_standard_tier_overrides_malformed_nested_payloads_and_replaces_cached_premium() {
    for payload in [
        r#"{"code":200,"user_tier_name":"standard","data":{"code":"200"}}"#,
        r#"{"code":200,"user_tier_name":"standard","data":"malformed","account":42}"#,
        r#"{"code":200,"data":{"user_tier_name":"standard","account":{"code":"malformed"}}}"#,
        r#"{"code":200,"account":{"user_tier_name":"standard","code":"malformed"}}"#,
    ] {
        let mut client = Client::new(options());
        let read = client.tick(2_000.0).pop().unwrap();
        reply_at(
            &mut client.engine,
            NOW + 3_000.0,
            &read,
            200,
            r#"{"code":200,"user_tier_name":"premium"}"#,
        );
        let refresh = client.tick(302_000.0).pop().unwrap();
        reply_at(&mut client.engine, NOW + 303_000.0, &refresh, 200, payload);
        let actions = client.place(304_000.0);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "{payload}"
        );
        assert!(actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: 51, error }) if error.message.contains("Plus or Premium"))), "{payload}");
    }
}

#[test]
fn singleton_and_nested_standard_labels_block_attributed_placement_before_any_read_or_write() {
    for label in [
        r#"["standard"]"#,
        r#"[[" STD "]]"#,
        r#"[[[["StAnDaRd"]]]]"#,
        r#"["\ufeffstandard\ufeff"]"#,
    ] {
        let mut client = Client::new(options());
        let read = client.tick(2_000.0).pop().unwrap();
        reply_at(
            &mut client.engine,
            NOW + 3_000.0,
            &read,
            200,
            &format!(r#"{{"code":200,"user_tier_name":{label}}}"#),
        );
        let actions = client.place(4_000.0);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "no approval, nonce or POST for {label}"
        );
        assert!(actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: 51, error }) if error.message.contains("Plus or Premium"))), "{label}");
    }
}

#[test]
fn unknown_and_nonlabel_values_do_not_become_standard_or_skip_builder_approval() {
    for label in [
        r#"["standard","standard"]"#,
        r#"["standard",null]"#,
        r#"[["standard",""]]"#,
        r#"[]"#,
        r#"null"#,
        r#"{}"#,
        r#"false"#,
        r#"42"#,
        r#"[["unknown"]]"#,
        r#"["\u0085standard\u0085"]"#,
        r#"{"toString":"standard"}"#,
        r#"[["premium"]]"#,
        r#"["plus"]"#,
    ] {
        let mut client = Client::new(options());
        let read = client.tick(2_000.0).pop().unwrap();
        reply_at(
            &mut client.engine,
            NOW + 3_000.0,
            &read,
            200,
            &format!(r#"{{"code":200,"user_tier_name":{label}}}"#),
        );
        let actions = client.place(4_000.0);
        let reads = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(read) => Some(read),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(reads.len(), 1, "{label}");
        assert_eq!(reads[0].method, safe_cex_engine::Method::Get);
        assert!(
            reads[0].url.contains("/account?"),
            "approval still required for {label}"
        );
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 51, .. })
            )),
            "{label}"
        );
    }
}
impl Client {
    fn action(
        &mut self,
        now: f64,
        command: u32,
        action: safe_cex_engine::VenueAction,
    ) -> Vec<Action> {
        self.engine
            .handle(now, Event::Command(Command::Venue { command, action }));
        safe_cex_engine::test_support::prepared_actions(&mut self.engine, now)
    }
}
fn done(actions: &[Action], id: u32) -> Option<Result<String, String>> {
    actions.iter().find_map(|action| match action {
        Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
            Some(Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
            Some(Err(error.legacy_text()))
        }
        _ => None,
    })
}
#[test]
fn account_tier_reads_the_cached_tier_and_a_change_posts_then_rereads_limits() {
    let mut client = Client::new(options());
    let read = client.tick(2_000.0).pop().unwrap();
    reply_at(
        &mut client.engine,
        NOW + 2_001.0,
        &read,
        200,
        r#"{"code":200,"user_tier_name":"standard"}"#,
    );
    client.engine.drain_actions().count();
    let actions = client.action(
        NOW + 2_002.0,
        61,
        safe_cex_engine::VenueAction::FetchAccountTier,
    );
    assert_eq!(done(&actions, 61), Some(Ok("\"standard\"".into())));
    let actions = client.action(
        NOW + 2_003.0,
        62,
        safe_cex_engine::VenueAction::SetAccountTier {
            tier: "premium".into(),
        },
    );
    let post = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/api/v1/changeAccountTier") => {
                Some(request.clone())
            }
            _ => None,
        })
        .expect("tier change request");
    assert_eq!(post.method, safe_cex_engine::Method::Post);
    assert_eq!(
        post.body.as_deref(),
        Some("account_index=718895&new_tier=premium")
    );
    assert!(
        post.headers
            .iter()
            .any(|(k, v)| *k == "authorization" && !v.is_empty())
    );
    reply_at(
        &mut client.engine,
        NOW + 2_004.0,
        &post,
        200,
        r#"{"code":200}"#,
    );
    let actions =
        safe_cex_engine::test_support::prepared_actions(&mut client.engine, NOW + 2_004.0);
    assert_eq!(done(&actions, 62), Some(Ok("null".into())));
    assert!(actions.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.contains("accountLimits"))
    ));
}
#[test]
fn rejected_or_invalid_tier_changes_report_the_reason() {
    let mut client = Client::new(options());
    let actions = client.action(
        NOW,
        63,
        safe_cex_engine::VenueAction::SetAccountTier {
            tier: "gold".into(),
        },
    );
    assert!(
        done(&actions, 63)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("standard, premium or plus")
    );
    let actions = client.action(
        NOW,
        64,
        safe_cex_engine::VenueAction::SetAccountTier {
            tier: "plus".into(),
        },
    );
    let post = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    reply_at(
        &mut client.engine,
        NOW + 1.0,
        &post,
        200,
        r#"{"code":21100,"message":"tier change cooldown"}"#,
    );
    let actions = client.engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        done(&actions, 64),
        Some(Err(
            "Lighter account tier change failed: tier change cooldown".into()
        ))
    );
}
#[test]
fn reading_the_cached_tier_never_starts_a_limits_read() {
    let mut client = Client::new(options());
    let actions = client.action(NOW, 65, safe_cex_engine::VenueAction::FetchAccountTier);
    assert_eq!(done(&actions, 65), Some(Ok("null".into())));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}
