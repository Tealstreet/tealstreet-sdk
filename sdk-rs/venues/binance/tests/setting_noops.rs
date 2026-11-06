//! Idempotent setting acknowledgements retain transport and operation boundaries.
mod support;
use safe_cex_engine::model::MarginType;
use safe_cex_engine::{Command, Event, Outcome};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

fn ready(variant: Variant) -> (Harness, &'static str, &'static str) {
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let (prefix, symbol) = if variant == Variant::CoinM {
        ("/dapi/v1", "BTCUSD_PERP")
    } else {
        ("/fapi/v1", "BTCUSDT")
    };
    let body = |source: &str| source.replace("BTCUSDT", symbol);
    h.respond(
        &format!("{prefix}/exchangeInfo"),
        200,
        &body(support::EXCHANGE_INFO),
    );
    let brackets = if variant == Variant::CoinM {
        "/dapi/v2/leverageBracket"
    } else {
        "/fapi/v1/leverageBracket"
    };
    h.respond(brackets, 200, &format!(r#"[{{"symbol":"{symbol}","brackets":[{{"bracket":1,"initialLeverage":125,"notionalCap":50000}}]}}]"#));
    h.respond(&format!("{prefix}/ticker/24hr"), 200, &body(support::DAILY));
    h.respond(
        &format!("{prefix}/ticker/bookTicker"),
        200,
        &body(support::BOOK_TICKERS),
    );
    h.respond(
        &format!("{prefix}/premiumIndex"),
        200,
        &body(support::PREMIUM),
    );
    h.requests.clear();
    (h, prefix, symbol)
}

#[test]
fn already_set_margin_type_completes_once_and_updates_the_requested_setting() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        for isolated in [false, true] {
            let (mut h, prefix, symbol) = ready(variant);
            h.event(Event::Command(Command::SetMarginType {
                command: 81,
                symbol: symbol.into(),
                isolated,
            }));
            let request = h.take(&format!("{prefix}/marginType"));
            h.reply(
                request.id,
                400,
                "",
                r#"{"code":-4046,"msg":"No need to change margin type."}"#,
            );
            assert_eq!(h.result(81), Some(&Ok("[]".into())), "{variant:?}");
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 81).count(), 1);
            assert_eq!(
                h.engine.store().setting(symbol).unwrap().margin_type,
                Some(if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                })
            );
            assert!(h.errors.is_empty(), "{:?}", h.errors);
            assert!(!h.requests.iter().any(|r| r.url.contains("/marginType")));
        }
    }
}

#[test]
fn already_set_position_mode_and_ordinary_leverage_ack_remain_successes() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let (mut h, prefix, symbol) = ready(variant);
        h.event(Event::Command(Command::SetPositionMode {
            command: 82,
            hedged: true,
            symbol: None,
        }));
        let request = h.take(&format!("{prefix}/positionSide/dual"));
        h.reply(
            request.id,
            400,
            "",
            r#"{"code":-4059,"msg":"No need to change position side."}"#,
        );
        assert_eq!(h.result(82), Some(&Ok("[]".into())));
        assert!(h.engine.store().hedged());
        h.event(Event::Command(Command::SetLeverage {
            command: 83,
            symbol: symbol.into(),
            leverage: 5.,
            side: None,
        }));
        let request = h.take(&format!("{prefix}/leverage"));
        h.reply(
            request.id,
            200,
            "",
            &format!(r#"{{"leverage":5,"symbol":"{symbol}"}}"#),
        );
        assert_eq!(h.result(83), Some(&Ok("[]".into())));
        assert_eq!(h.engine.store().setting(symbol).unwrap().leverage, 5.);
        h.event(Event::Command(Command::SetLeverage {
            command: 84,
            symbol: symbol.into(),
            leverage: 5.,
            side: None,
        }));
        assert_eq!(h.result(84), Some(&Ok("[]".into())));
        assert!(!h.requests.iter().any(|r| r.url.contains("/leverage")));
    }
}

#[test]
fn ambiguous_leverage_codes_and_wrong_operation_noops_are_not_successes() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        for code in [-4028, -4046, -4059] {
            let (mut h, prefix, symbol) = ready(variant);
            h.event(Event::Command(Command::SetLeverage {
                command: 85,
                symbol: symbol.into(),
                leverage: 5.,
                side: None,
            }));
            let request = h.take(&format!("{prefix}/leverage"));
            h.reply(
                request.id,
                400,
                "",
                &format!(r#"{{"code":{code},"msg":"Invalid leverage"}}"#),
            );
            assert!(h.result(85).unwrap().is_err());
            assert_ne!(
                h.engine.store().setting(symbol).map(|s| s.leverage),
                Some(5.)
            );
        }
    }
}

#[test]
fn gateway_noop_code_is_unknown_and_cannot_update_margin_type() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let (mut h, prefix, symbol) = ready(variant);
        let before = h.engine.store().setting(symbol);
        h.event(Event::Command(Command::SetMarginType {
            command: 86,
            symbol: symbol.into(),
            isolated: true,
        }));
        let request = h.take(&format!("{prefix}/marginType"));
        h.reply(
            request.id,
            503,
            "",
            r#"{"code":-4046,"msg":"No need to change margin type."}"#,
        );
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 86)
                .unwrap()
                .1
                .outcome,
            Some(Outcome::Unknown)
        );
        assert_eq!(h.engine.store().setting(symbol), before);
    }
}
