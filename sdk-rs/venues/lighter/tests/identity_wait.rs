//! A private history read issued while the account index lookup runs waits for it.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query};
use safe_cex_venue_lighter::Lighter;
use support::*;

const IDENTITY_UNAVAILABLE: &str = "Lighter private account identity is unavailable";

/// Starts the engine and answers everything but the account index lookup, which it returns.
fn started_with_lookup_pending(options: Options) -> (Engine<Lighter>, Option<HttpRequest>) {
    let mut e = engine(options);
    e.start(NOW);
    let mut lookup = None;
    for _ in 0..30 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let http: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        if http.is_empty() {
            break;
        }
        for r in http {
            if r.url.contains("orderBookDetails") || r.url.contains("orderBooks") {
                reply(&mut e, &r, 200, META);
            } else if r.url.contains("exchangeStats") {
                reply(
                    &mut e,
                    &r,
                    200,
                    r#"{"code":200,"order_book_stats":[{"market_id":1,"last_trade_price":"100","mark_price":"100","index_price":"100"}]}"#,
                );
            } else if r.url.contains("/account?") && lookup.is_none() {
                lookup = Some(r);
            } else {
                panic!("unexpected request before the lookup settles: {}", r.url);
            }
        }
    }
    (e, lookup)
}

fn fills(e: &mut Engine<Lighter>, command: u32) {
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command,
            query: Query::Fills {
                symbol: Some("ETH".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(10),
            },
        }),
    );
}

fn outcome(actions: &[Action], command: u32) -> Option<Result<String, String>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
            Some(Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
            Some(Err(error.message.clone()))
        }
        _ => None,
    })
}

#[test]
fn a_read_issued_during_the_lookup_is_answered_after_it() {
    let (mut e, lookup) = started_with_lookup_pending(options());
    let lookup = lookup.expect("the account index lookup is pending");
    fills(&mut e, 7);
    let waiting = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(outcome(&waiting, 7).is_none(), "{waiting:?}");
    assert!(
        !waiting
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("trades")))
    );

    reply(&mut e, &lookup, 200, ACCOUNT);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let trades = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/trades?") => Some(r.clone()),
            _ => None,
        })
        .expect("the parked read goes out once the index is known");
    assert!(
        trades.url.contains("account_index=718895"),
        "{}",
        trades.url
    );
    reply(&mut e, &trades, 200, r#"{"code":200,"trades":[]}"#);
    let done = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert_eq!(outcome(&done, 7), Some(Ok("[]".into())), "{done:?}");
}

#[test]
fn a_read_that_waited_is_refused_when_the_lookup_gives_up() {
    let (mut e, lookup) = started_with_lookup_pending(options());
    let lookup = lookup.expect("the account index lookup is pending");
    fills(&mut e, 8);
    safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    // The venue names no such account: every candidate is answered and none matches.
    reply(
        &mut e,
        &lookup,
        404,
        r#"{"code":404,"message":"account not found"}"#,
    );
    let mut seen = Vec::new();
    for _ in 0..10 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        if let Some(result) = outcome(&actions, 8) {
            assert_eq!(result, Err(IDENTITY_UNAVAILABLE.into()));
            return;
        }
        for action in &actions {
            if let Action::SetTimer { timer, at_ms } = action
                && *at_ms <= NOW
            {
                e.handle(NOW, Event::TimerFired { timer: *timer });
            }
        }
        let next: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r.clone()),
                _ => None,
            })
            .collect();
        for r in next {
            seen.push(r.url.clone());
            reply(
                &mut e,
                &r,
                404,
                r#"{"code":404,"message":"account not found"}"#,
            );
        }
    }
    panic!("the waiting read was never refused; requests {seen:?}");
}

#[test]
fn an_account_without_signing_credentials_still_answers_at_once() {
    let mut e = engine(Options {
        application_id: Some("718895".into()),
        ..Options::default()
    });
    e.start(NOW);
    safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    fills(&mut e, 9);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(outcome(&actions, 9).is_some(), "{actions:?}");
}

#[test]
fn a_read_that_waited_expires_at_its_own_deadline_if_the_lookup_never_answers() {
    let (mut e, lookup) = started_with_lookup_pending(options());
    assert!(lookup.is_some());
    fills(&mut e, 10);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let deadline = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > NOW => Some((*timer, *at_ms)),
            _ => None,
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("the parked read keeps its root deadline");
    e.handle(deadline.1, Event::TimerFired { timer: deadline.0 });
    let expired = safe_cex_engine::test_support::prepared_actions(&mut e, deadline.1);
    assert!(matches!(outcome(&expired, 10), Some(Err(_))), "{expired:?}");
}
