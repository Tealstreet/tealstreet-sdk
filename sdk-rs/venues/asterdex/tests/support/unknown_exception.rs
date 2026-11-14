use super::legacy_parity::{batch, configured, ordinary, write_sequence};
use super::*;
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{Outcome, VenueError};

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
const HTTP_UNKNOWN_TEXT: &str =
    "Response does not prove the write outcome. The outcome is unknown. Check Open Orders";
const UNKNOWN_AFTER_REFUSAL: &str = "The outcome of this order change is unknown: a later request was not confirmed. Check Open Orders";

#[derive(Clone, Copy, Debug)]
enum Path {
    Split,
    Batch,
    Bracket,
    Replace,
    Capacity,
    Cancels,
    Runtime,
    Conditional,
    Market,
    PriorRefusal,
}

#[derive(Clone, Copy, Debug)]
enum Uncertain {
    NativeTimeout,
    RateLimit,
    Gateway,
    Transport,
    MissingIdentity,
}

const VENUE_FAILURES: &[Uncertain] = &[
    Uncertain::NativeTimeout,
    Uncertain::RateLimit,
    Uncertain::Transport,
    Uncertain::MissingIdentity,
];

fn check(path: Path, cases: &[Uncertain]) {
    for &uncertain in cases {
        if matches!(
            (path, uncertain),
            (Path::Cancels, Uncertain::MissingIdentity)
        ) {
            continue; // Ordinary DELETE success does not require a placement identity.
        }
        let mut h = configured(
            1.0,
            5.0,
            1.0,
            0.5,
            if matches!(path, Path::Capacity) {
                1
            } else {
                10
            },
        );
        let (total, pending, method) = match path {
            Path::Split | Path::Runtime => {
                h.place(ordinary(12.0, 1.0));
                (3, 1, Method::Post)
            }
            Path::PriorRefusal => {
                batch(
                    &mut h,
                    vec![
                        ordinary(1.0, 1.0),
                        ordinary(1.0, 1.0),
                        ordinary(2.0, 1.0),
                        ordinary(3.0, 1.0),
                    ],
                );
                let refused = h.take(Method::Post);
                h.reply(refused, r#"{"code":-2010,"msg":"insufficient margin"}"#);
                (4, 2, Method::Post)
            }
            Path::Batch | Path::Conditional | Path::Market => {
                let mut second = ordinary(2.0, 1.0);
                if matches!(path, Path::Conditional) {
                    second["type"] = json!("stop_market");
                    second["amount"] = json!(7.0);
                    second["triggerPrice"] = json!(0.5);
                } else if matches!(path, Path::Market) {
                    second["type"] = json!("market");
                    h.engine.store_mut().set_tickers(vec![]);
                }
                batch(&mut h, vec![ordinary(1.0, 1.0), second, ordinary(3.0, 1.0)]);
                (3, 1, Method::Post)
            }
            Path::Bracket | Path::Capacity => {
                let mut bracket = ordinary(1.0, 1.0);
                bracket["stopLoss"] = json!(0.5);
                if matches!(path, Path::Bracket) {
                    bracket["takeProfit"] = json!(2.0);
                    h.place(bracket);
                    (3, 1, Method::Post)
                } else {
                    batch(&mut h, vec![bracket.clone(), bracket]);
                    (4, 1, Method::Post)
                }
            }
            Path::Replace => {
                h.engine
                    .store_mut()
                    .set_orders(vec![order("10", OrderType::Limit)]);
                h.command(Command::AmendOrder {
                    command: 1,
                    id: "10".into(),
                    price: Some(1.0),
                    amount: Some(12.0),
                });
                let cancel = h.take(Method::Delete);
                h.reply(cancel, r#"{"orderId":10,"status":"CANCELED"}"#);
                (4, 2, Method::Post)
            }
            Path::Cancels => {
                h.engine.store_mut().set_orders(vec![
                    order("10", OrderType::Limit),
                    order("11", OrderType::Limit),
                    order("12", OrderType::Limit),
                ]);
                h.command(Command::CancelOrders {
                    command: 1,
                    ids: vec!["10".into(), "11".into(), "12".into()],
                });
                (3, 1, Method::Delete)
            }
        };
        let first = h.take(method);
        h.reply(
            first,
            if method == Method::Delete {
                r#"{"orderId":10,"status":"CANCELED"}"#
            } else {
                r#"{"orderId":801}"#
            },
        );
        let next = h.take(method);
        let unknown_request = next.id;
        match uncertain {
            Uncertain::NativeTimeout => h.reply(next, r#"{"code":-1007,"msg":"Timeout waiting for response from backend server. Send status unknown; execution status unknown."}"#),
            Uncertain::RateLimit => h.reply(next, r#"{"code":-1003,"msg":"Too many requests."}"#),
            Uncertain::Gateway => h.reply_status(next, 503, "gateway lost acknowledgement"),
            Uncertain::MissingIdentity => h.reply(next, "{}"),
            Uncertain::Transport => {
                h.engine.handle(1700000000000.0, Event::HttpFailed { request: next.id, error: "connection reset after release" });
                h.collect();
            }
        }
        let label = format!("{path:?}/{uncertain:?}");
        assert_eq!(
            h.sent_writes,
            pending + 1,
            "{label}: {:?}",
            write_sequence(&h)
        );
        assert!(
            !h.requests.iter().any(|r| r.method != Method::Get),
            "{label}"
        );
        let error: &VenueError = &h.failures.last().expect(&label).1;
        assert_eq!(error.outcome, Some(Outcome::Unknown), "{label}: {error:?}");
        let rows = error.elements.as_ref().expect(&label);
        assert_eq!(rows.len(), total, "{label}");
        assert!(
            matches!(rows[pending].result, ElementResult::Unknown { .. }),
            "{label}: {rows:?}"
        );
        assert!(
            rows[pending + 1..]
                .iter()
                .all(|row| row.result == ElementResult::NotSent { error: None }),
            "{label}"
        );
        if matches!(path, Path::Replace) {
            assert!(
                matches!(&rows[0].result, ElementResult::Applied {
                effect: safe_cex_engine::write_plan::AppliedEffect::Cancel,
                target_order_id: Some(id), ..
            } if id == "10"),
                "{label}"
            );
        }
        if method == Method::Delete {
            assert!(
                matches!(rows[0].result, ElementResult::Applied { .. }),
                "{label}"
            );
        } else {
            assert!(
                matches!(&rows[pending - 1].result, ElementResult::Placed { order_ids } if order_ids == &["801"]),
                "{label}"
            );
            assert!(
                error
                    .partial
                    .as_ref()
                    .unwrap()
                    .order_ids
                    .iter()
                    .any(|id| id == "801"),
                "{label}"
            );
        }
        let engine_sealed = matches!(uncertain, Uncertain::Gateway);
        let terminal_text = if engine_sealed {
            if matches!(path, Path::PriorRefusal) {
                UNKNOWN_AFTER_REFUSAL
            } else {
                HTTP_UNKNOWN_TEXT
            }
        } else {
            ENGINE_UNKNOWN_TEXT
        };
        assert!(
            error.message.contains(terminal_text),
            "{label}: {}",
            error.message
        );
        if let ElementResult::Unknown { error } = &rows[pending].result {
            let row_text = if engine_sealed {
                HTTP_UNKNOWN_TEXT
            } else {
                ENGINE_UNKNOWN_TEXT
            };
            assert!(
                error.message.contains(row_text),
                "{label}: {}",
                error.message
            );
        }
        // Engine-sealed telemetry awaits the later engine hook; other pins stay.
        if !engine_sealed {
            let events: Vec<_> = h
                .events
                .iter()
                .filter(|event| event.code == "write.unknown")
                .collect();
            assert_eq!(events.len(), 1, "{label}: {:?}", h.events);
            assert!(events[0].message.contains(ENGINE_UNKNOWN_TEXT), "{label}");
            assert!(
                events[0]
                    .fields
                    .iter()
                    .any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown")),
                "{label}"
            );
            assert!(
                events[0].fields.iter().any(|(key, value)| *key == "command"
                    && matches!(value, safe_cex_engine::log::Value::Num(id) if *id == 1.0)),
                "{label}"
            );
        }
        let before = h.sent_writes;
        h.engine.handle(
            1700000000000.0,
            Event::HttpResponse {
                request: unknown_request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"orderId":999,"status":"CANCELED"}"#,
                },
            },
        );
        h.collect();
        assert_eq!(
            h.sent_writes, before,
            "{label}: no retry or continuation after late ACK"
        );
        assert_eq!(h.failures.len(), 1, "{label}: one terminal");
        if !engine_sealed {
            assert_eq!(
                h.events
                    .iter()
                    .filter(|event| event.code == "write.unknown")
                    .count(),
                1,
                "{label}: one telemetry event"
            );
        }
    }
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:867,975-997.
#[test]
fn unknown_exception_max_quantity_split() {
    check(Path::Split, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:975-997.
#[test]
fn unknown_exception_serial_batch() {
    check(Path::Batch, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:812-814,975-997.
#[test]
fn unknown_exception_protection_children() {
    check(Path::Bracket, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:1040,867,975-997.
#[test]
fn unknown_exception_split_replacement() {
    check(Path::Replace, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:773-774,975-997.
#[test]
fn unknown_exception_admitted_algo_intents() {
    check(Path::Capacity, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:1051-1076.
#[test]
fn unknown_exception_cancel_fanout() {
    check(Path::Cancels, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:975-997.
#[test]
fn unknown_exception_runtime_placement() {
    check(Path::Runtime, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:955-966,975-997.
#[test]
fn unknown_exception_conditional_batch() {
    check(Path::Conditional, VENUE_FAILURES);
}
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:955-966,975-997.
#[test]
fn unknown_exception_market_without_mark() {
    check(Path::Market, VENUE_FAILURES);
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts:975-997.
#[test]
fn unknown_exception_replaces_prior_refusal_text() {
    check(Path::PriorRefusal, VENUE_FAILURES);
}

macro_rules! gateway_pin {
    ($name:ident, $path:ident) => {
        #[test]
        fn $name() {
            check(Path::$path, &[Uncertain::Gateway]);
        }
    };
}

// The same exception pins also cover the engine-owned HTTP seal independently.
gateway_pin!(unknown_exception_gateway_split, Split);
gateway_pin!(unknown_exception_gateway_batch, Batch);
gateway_pin!(unknown_exception_gateway_bracket, Bracket);
gateway_pin!(unknown_exception_gateway_replace, Replace);
gateway_pin!(unknown_exception_gateway_capacity, Capacity);
gateway_pin!(unknown_exception_gateway_cancels, Cancels);
gateway_pin!(unknown_exception_gateway_runtime, Runtime);
gateway_pin!(unknown_exception_gateway_conditional, Conditional);
gateway_pin!(unknown_exception_gateway_market, Market);
gateway_pin!(unknown_exception_gateway_prior_refusal, PriorRefusal);
