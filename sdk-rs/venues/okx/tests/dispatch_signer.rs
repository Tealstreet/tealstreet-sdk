#![allow(dead_code)]
mod support;

use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Outcome, Query, RequestId};
use safe_cex_venue_okx::Okx;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use support::{actions, envelope, reply, trading_engine_configure};

struct Gate {
    provider: Provider,
    parked: Arc<AtomicBool>,
}
impl Signer for Gate {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.provider.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.parked.load(Ordering::SeqCst) {
            None
        } else {
            self.provider.sign_ready(request)
        }
    }
}
fn ready_with_provider() -> (Engine<Okx>, Arc<AtomicBool>, Provider) {
    let mut local = None;
    let parked = Arc::new(AtomicBool::new(false));
    let engine = trading_engine_configure(|engine| {
        let provider = engine.signer().unwrap();
        local = Some(provider.clone());
        engine
            .set_signer(Some(Arc::new(Gate {
                provider,
                parked: parked.clone(),
            })))
            .unwrap();
    });
    (engine, parked, local.unwrap())
}
fn ready() -> (Engine<Okx>, Arc<AtomicBool>) {
    let (engine, parked, _) = ready_with_provider();
    (engine, parked)
}
fn place(engine: &mut Engine<Okx>, bracket: bool) -> Vec<Action> {
    let text = if bracket {
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"stopLoss":36000}"#
    } else {
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"clientOrderId":"Owned123"}"#
    };
    engine.handle(
        5.,
        Event::Command(Command::PlaceOrders {
            command: 41,
            orders: vec![serde_json::from_str(text).unwrap()],
        }),
    );
    release(engine, 5.)
}
/// Releases through the fixture host's prepare and bind, passing a parked
/// provider's `Sign` through instead of refusing it.
fn release(engine: &mut Engine<Okx>, now: f64) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    engine.prepare_dispatch(now);
    let mut out = Vec::new();
    loop {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return out;
        }
        for action in actions {
            let Action::ManagedHttp(action) = action else {
                out.push(action);
                continue;
            };
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    let id = request.id.0;
                    let prepared = Prepared {
                        id,
                        expires_at: None,
                    };
                    engine.http_prepared(now, preparation, prepared);
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    if let Ok(Claim::Released(_)) = engine.claim_http(now, permit) {
                        out.push(Action::Http(request));
                    }
                }
                _ => {}
            }
        }
    }
}
fn attempt(out: &[Action]) -> RequestId {
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "no physical HTTP before signing"
    );
    out.iter()
        .find_map(|a| match a {
            Action::Sign { attempt, .. } => Some(*attempt),
            _ => None,
        })
        .unwrap()
}

#[test]
fn first_write_provider_refusal_is_not_sent_and_completes_once() {
    let (mut engine, parked) = ready();
    parked.store(true, Ordering::SeqCst);
    let out = place(&mut engine, false);
    let id = attempt(&out);
    engine.handle(
        6.,
        Event::Signed {
            attempt: id,
            result: Err(SignError::Refused),
        },
    );
    let out = release(&mut engine, 6.);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let failures: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 41, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
    engine.handle(
        7.,
        Event::Signed {
            attempt: id,
            result: Err(SignError::Refused),
        },
    );
    assert!(!release(&mut engine, 7.).iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
}

#[test]
fn later_bracket_sign_failure_preserves_the_accepted_entry_effect() {
    let (mut engine, parked) = ready();
    let entry = place(&mut engine, true)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    parked.store(true, Ordering::SeqCst);
    reply(
        &mut engine,
        &entry,
        &envelope(r#"{"ordId":"accepted-entry","sCode":"0","sMsg":""}"#),
        6.,
    );
    let id = attempt(&release(&mut engine, 6.));
    engine.handle(
        7.,
        Event::Signed {
            attempt: id,
            result: Err(SignError::Refused),
        },
    );
    let out = release(&mut engine, 7.);
    let failure = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 41, error }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        failure.outcome, None,
        "existing partial-effect contract, never wholly NotSent"
    );
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn primary_lookup_provider_refusal_never_becomes_definitive_absence() {
    let (mut engine, parked) = ready();
    parked.store(true, Ordering::SeqCst);
    engine.handle(
        5.,
        Event::Command(Command::Query {
            command: 42,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "Missing123".into(),
            },
        }),
    );
    let preparation = actions(&mut engine)
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                safe_cex_engine::request_context::HttpAction::Prepare { preparation, .. } => {
                    Some(preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("lookup route preparation before deferred provider");
    engine.http_prepared(
        5.,
        preparation,
        safe_cex_engine::request_context::Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let id = attempt(&actions(&mut engine));
    engine.handle(
        6.,
        Event::Signed {
            attempt: id,
            result: Err(SignError::Refused),
        },
    );
    let out = release(&mut engine, 6.);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, error }) if error.outcome == Some(Outcome::NotSent))));
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { command: 42, .. })
    )));
}

#[test]
fn delayed_real_write_reprepares_stamp_without_changing_intent_or_route() {
    let (mut engine, parked, local) = ready_with_provider();
    parked.store(true, Ordering::SeqCst);
    let first = place(&mut engine, false);
    let (old_attempt, old_work) = first
        .into_iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    let old = local.sign_ready(&old_work).unwrap().unwrap();
    engine.handle(
        6006.,
        Event::Signed {
            attempt: old_attempt,
            result: Ok(old.clone()),
        },
    );
    let out = release(&mut engine, 6006.);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let (new_attempt, new_work) = out
        .into_iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    assert_ne!(old_attempt, new_attempt);
    assert_eq!(new_work.receipt.stamp_ms, Some(6006.));
    assert_eq!(old_work.context, new_work.context);
    engine.handle(
        6007.,
        Event::Signed {
            attempt: old_attempt,
            result: Ok(old),
        },
    );
    assert!(
        !release(&mut engine, 6007.)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    engine.handle(
        6007.,
        Event::Signed {
            attempt: new_attempt,
            result: local.sign_ready(&new_work).unwrap(),
        },
    );
    let out = release(&mut engine, 6007.);
    let http = out
        .into_iter()
        .find_map(|a| match a {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .unwrap();
    let header = |name| &http.headers.iter().find(|(key, _)| *key == name).unwrap().1;
    assert_eq!(header("OK-ACCESS-KEY"), "test-key");
    assert_eq!(header("OK-ACCESS-TIMESTAMP"), "1970-01-01T00:00:06.006Z");
    assert_eq!(
        http.route, None,
        "ordered writes retain default primary route"
    );
    let body = http.body.as_deref().unwrap();
    let rows: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(rows[0]["clOrdId"], "Owned123");
    assert_eq!(rows[0]["sz"], "2");
    assert_eq!(rows[0]["px"], "38000");
    assert_eq!(
        *header("OK-ACCESS-SIGN"),
        safe_cex_crypto::hmac_sha256(
            b"test-secret",
            format!("1970-01-01T00:00:06.006ZPOST/api/v5/trade/batch-orders{body}").as_bytes(),
            safe_cex_crypto::Encoding::Base64
        )
    );
}

/// Refuses synchronously, so both legacy and managed preparation see it.
struct Refusing {
    provider: Provider,
    refuse: Arc<AtomicBool>,
}
impl Signer for Refusing {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.provider.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.refuse.load(Ordering::SeqCst) {
            Some(Err(SignError::Refused))
        } else {
            self.provider.sign_ready(request)
        }
    }
}

#[test]
fn leverage_read_provider_refusal_is_one_not_sent_failure_without_http() {
    let refuse = Arc::new(AtomicBool::new(false));
    let mut engine = trading_engine_configure(|engine| {
        let provider = engine.signer().unwrap();
        engine
            .set_signer(Some(Arc::new(Refusing {
                provider,
                refuse: refuse.clone(),
            })))
            .unwrap();
    });
    refuse.store(true, Ordering::SeqCst);
    engine.handle(
        5.,
        Event::Command(Command::FetchLeverage {
            command: 43,
            symbol: "BTCUSDT".into(),
        }),
    );
    let out = release(&mut engine, 5.);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let terminals: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 43, error }) => Some(Some(error)),
            Action::Notice(Notice::CommandDone { command: 43, .. }) => Some(None),
            _ => None,
        })
        .collect();
    assert_eq!(terminals.len(), 1);
    assert_eq!(
        terminals[0].expect("refusal is a failure").outcome,
        Some(Outcome::NotSent)
    );
}
