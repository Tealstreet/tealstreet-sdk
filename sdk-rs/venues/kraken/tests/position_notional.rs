mod support;

use safe_cex_engine::{Action, Engine, Event, HttpRequest, SocketId};
use safe_cex_venue_kraken::Kraken;
use serde_json::{Value, json};
use support::*;

fn stream(engine: &mut Engine<Kraken>, socket: SocketId, feed: &str, row: Value) {
    message(
        engine,
        socket,
        NOW,
        &json!({"feed": feed, "positions": [row]}).to_string(),
    );
}

fn position(mark: Option<Value>) -> Value {
    let mut row = json!({"instrument":"PF_XBTUSD","balance":2,"entry_price":100});
    if let Some(mark) = mark {
        row["mark_price"] = mark;
    }
    row
}

fn positions_poll(engine: &mut Engine<Kraken>, socket: SocketId) -> HttpRequest {
    message(engine, socket, NOW, r#"{"feed":"fills","fills":[]}"#);
    let (timer, at) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if (NOW + 1000.0..=NOW + 1001.0).contains(&at_ms) => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("reconciliation timer");
    engine.handle(at, Event::TimerFired { timer });
    requests(engine)
        .into_iter()
        .find(|request| request.url.contains("/openpositions"))
        .expect("positions read")
}

fn rest_position(engine: &mut Engine<Kraken>, socket: SocketId, size: f64) {
    let request = positions_poll(engine, socket);
    reply(
        engine,
        &request,
        200,
        &json!({"result":"success","openPositions":[{
            "symbol":"PF_XBTUSD","side":"long","size":size,"price":100
        }]})
        .to_string(),
    );
}

fn frame_notional(frame: &[u8]) -> f64 {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 8 {
            let count = u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap()) as usize;
            assert_eq!(count, 1);
            let column = (at + 12 + count * 8 + 7) & !7;
            return f64::from_le_bytes(frame[column + 8..column + 16].try_into().unwrap());
        }
        at += 8 + size;
    }
    panic!("positions section missing");
}

#[test]
fn notional_missing_or_unusable_stream_mark_is_unknown_without_losing_coin_size() {
    for feed in ["open_positions", "open_positions_snapshot"] {
        for mark in [
            None,
            Some(Value::Null),
            Some(json!("")),
            Some(json!("N/A")),
            Some(json!("NaN")),
            Some(json!("Infinity")),
            Some(json!(0)),
            Some(json!(-1)),
            Some(json!(true)),
            Some(json!({})),
            Some(json!([])),
        ] {
            let (mut engine, socket) = ready();
            engine.store_mut().set_tickers(vec![]);
            stream(&mut engine, socket, feed, position(mark.clone()));
            let p = engine.store().position("PF_XBTUSD:long").expect("position");
            assert_eq!(p.contracts, 2.0);
            assert_eq!(p.entry_price, 100.0);
            assert!(p.notional.is_nan(), "{feed} {mark:?}: {p:?}");
            assert!(serde_json::to_value(p).unwrap()["notional"].is_null());
            assert!(frame_notional(engine.take_frame(NOW).unwrap()).is_nan());
        }
    }
}

#[test]
fn notional_rest_without_a_mark_is_unknown_and_preserves_position_size() {
    let (mut engine, socket) = ready();
    engine.store_mut().set_tickers(vec![]);
    rest_position(&mut engine, socket, 2.0);
    let p = engine.store().position("PF_XBTUSD:long").unwrap();
    assert_eq!(p.contracts, 2.0);
    assert!(p.notional.is_nan(), "{p:?}");
}

#[test]
fn notional_rest_recovers_from_an_unknown_or_invalid_same_size_cache() {
    for cached in [f64::NAN, 0.0, -1.0, f64::INFINITY] {
        let (mut engine, socket) = ready();
        stream(
            &mut engine,
            socket,
            "open_positions",
            position(Some(json!(120))),
        );
        let mut positions = engine.store().positions().to_vec();
        positions[0].notional = cached;
        engine.store_mut().set_positions(positions);
        rest_position(&mut engine, socket, 2.0);
        let p = engine.store().position("PF_XBTUSD:long").unwrap();
        assert_eq!(p.notional, 202.0, "cached {cached}: {p:?}");
    }
}

#[test]
fn notional_rest_cannot_preserve_a_cached_value_after_size_changes() {
    let (mut engine, socket) = ready();
    stream(
        &mut engine,
        socket,
        "open_positions",
        position(Some(json!(120))),
    );
    engine.store_mut().set_tickers(vec![]);
    rest_position(&mut engine, socket, 3.0);
    let p = engine.store().position("PF_XBTUSD:long").unwrap();
    assert_eq!(p.contracts, 3.0);
    assert!(p.notional.is_nan(), "{p:?}");
}

#[test]
fn notional_linear_uses_native_base_units_and_only_a_valid_mark() {
    for (mark, expected) in [
        (Some(json!(150)), 300.0),
        (Some(json!("150")), 300.0),
        (None, 202.0),
        (Some(Value::Null), 202.0),
        (Some(json!(0)), 202.0),
        (Some(json!(-1)), 202.0),
    ] {
        let (mut engine, socket) = ready();
        stream(&mut engine, socket, "open_positions", position(mark));
        let p = engine.store().position("PF_XBTUSD:long").unwrap();
        assert_eq!(p.contracts, 2.0);
        assert_eq!(p.notional, expected);
    }
    for mark in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let (mut engine, socket) = ready();
        let mut tickers = engine.store().tickers().to_vec();
        tickers.iter_mut().for_each(|ticker| ticker.mark = mark);
        engine.store_mut().set_tickers(tickers);
        stream(&mut engine, socket, "open_positions", position(None));
        assert!(engine.store().positions()[0].notional.is_nan(), "{mark}");
    }
}

#[test]
fn notional_inverse_uses_quote_contract_value_and_does_not_require_a_mark() {
    for symbol in ["PI_XBTUSD", "FI_XBTUSD_260626"] {
        for contract_size in [
            Some(1.0),
            Some(10.0),
            None,
            Some(0.0),
            Some(-1.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            for mark in [Some(json!(50_000)), None] {
                let (mut engine, socket) = ready();
                // Inverse products remain excluded from discovery; seed metadata only
                // to verify the quote-unit formula without expanding venue support.
                let mut market = engine.store().markets()[0].clone();
                market.symbol = symbol.into();
                market.is_inverse = Some(true);
                market.contract_size = contract_size;
                engine.store_mut().set_markets(vec![market]);
                engine.store_mut().set_tickers(vec![]);
                let mut row = position(mark);
                row["instrument"] = json!(symbol);
                row["balance"] = json!(-2);
                stream(&mut engine, socket, "open_positions", row);
                let p = engine.store().position(&format!("{symbol}:short")).unwrap();
                if let Some(value) = contract_size.filter(|value| value.is_finite() && *value > 0.0)
                {
                    assert_eq!(p.notional, 2.0 * value, "{symbol}");
                } else {
                    assert!(p.notional.is_nan(), "{symbol} {contract_size:?}: {p:?}");
                }
            }
        }
    }
}
