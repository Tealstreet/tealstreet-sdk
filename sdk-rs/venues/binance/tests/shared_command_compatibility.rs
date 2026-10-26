//! Shared engine additions must not silently change Binance execution policy.
use support::Options;
mod support;
use safe_cex_engine::model::{CancelOrderRef, PlaceOrderOpts, StpMode};
use safe_cex_engine::{Command, Event};
use safe_cex_venue_binance::Variant;
use support::Harness;

fn ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let prefix = match variant {
        Variant::Spot => "/api/v3",
        Variant::CoinM => "/dapi/v1",
        _ => "/fapi/v1",
    };
    let inverse = variant == Variant::CoinM;
    let adapt = |text: &str| {
        if inverse {
            text.replace("BTCUSDT", "BTCUSD_PERP")
                .replace("\"quoteAsset\":\"USDT\"", "\"quoteAsset\":\"USD\"")
                .replace("\"marginAsset\":\"USDT\"", "\"marginAsset\":\"BTC\"")
        } else {
            text.to_owned()
        }
    };
    h.respond(
        &format!("{prefix}/exchangeInfo"),
        200,
        &adapt(support::EXCHANGE_INFO),
    );
    h.respond(
        &format!("{prefix}/ticker/24hr"),
        200,
        &adapt(support::DAILY),
    );
    if variant != Variant::Spot {
        h.respond(
            &format!("{prefix}/ticker/bookTicker"),
            200,
            &adapt(support::BOOK_TICKERS),
        );
        h.respond(
            &format!("{prefix}/premiumIndex"),
            200,
            &adapt(support::PREMIUM),
        );
    }
    h.requests.clear();
    h
}
fn order(variant: Variant, stp_mode: Option<StpMode>) -> PlaceOrderOpts {
    let mut order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
    )
    .unwrap();
    if variant == Variant::CoinM {
        order.symbol = "BTCUSD_PERP".into();
    }
    order.stp_mode = stp_mode;
    order
}
#[test]
fn stp_intent_in_a_later_order_rejects_the_whole_batch_on_every_binance_variant() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::CoinM,
        Variant::Spot,
    ] {
        for mode in [
            StpMode::ExpireTaker,
            StpMode::ExpireMaker,
            StpMode::ExpireBoth,
        ] {
            let mut h = ready(variant);
            let before = h.engine.snapshot_frame(h.now).to_vec();
            h.event(Event::Command(Command::PlaceOrders {
                command: 101,
                orders: vec![order(variant, None), order(variant, Some(mode))],
            }));
            assert!(
                h.result(101)
                    .unwrap()
                    .as_ref()
                    .unwrap_err()
                    .contains("self-trade prevention"),
                "{variant:?} {mode:?}"
            );
            assert!(h.requests.is_empty());
            assert!(h.sent.is_empty());
            assert_eq!(h.engine.snapshot_frame(h.now), before);
        }
    }
}
#[test]
fn newly_shared_unsupported_commands_complete_without_reads_writes_or_store_mutation() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::CoinM,
        Variant::Spot,
    ] {
        for command in [
            Command::FetchStpMode { command: 102 },
            Command::AmendOrderWithSymbol {
                command: 102,
                id: "unseen".into(),
                symbol: "BTCUSDT".into(),
                price: Some(100.0),
                amount: None,
            },
            Command::SetStpMode {
                command: 102,
                mode: StpMode::ExpireBoth,
            },
            Command::CancelOrdersWithSymbols {
                command: 102,
                orders: vec![CancelOrderRef {
                    id: "unseen".into(),
                    symbol: "BTCUSDT".into(),
                }],
            },
            Command::FetchAccount { command: 102 },
            Command::FetchOrderBook {
                command: 102,
                symbol: "BTCUSDT".into(),
                limit: Some(5),
            },
        ] {
            // USD-M/PM now have real identity reads, covered in identity.rs.
            // Spot/Coin-M retain the no-I/O unsupported contract here.
            if matches!(command, Command::FetchAccount { .. })
                && matches!(variant, Variant::UsdM | Variant::PortfolioMargin)
            {
                continue;
            }
            let mut h = ready(variant);
            let before = h.engine.snapshot_frame(h.now).to_vec();
            h.event(Event::Command(command));
            assert!(h.result(102).unwrap().is_err(), "{variant:?}");
            assert!(h.requests.is_empty());
            assert_eq!(h.engine.snapshot_frame(h.now), before);
        }
    }
}
