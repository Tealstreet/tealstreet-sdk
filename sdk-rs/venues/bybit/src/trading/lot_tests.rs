use super::*;
use crate::{Options, Variant};
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::test_support::{ProbeVenue, seeded_engine};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateLot {
    category: Option<String>,
    qty: String,
    take_profit: Option<String>,
    stop_loss: Option<String>,
    tp_order_type: Option<String>,
    sl_order_type: Option<String>,
}

fn check_lots(variant: Variant, single: bool) {
    for (with_stop_loss, with_take_profit) in [(false, true), (true, false), (true, true)] {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            let stop_loss =
                with_stop_loss.then_some(if side == OrderSide::Buy { 90. } else { 110. });
            let take_profit =
                with_take_profit.then_some(if side == OrderSide::Buy { 110. } else { 90. });
            let venue = BybitLinear::new(Options {
                variant,
                ..Options::default()
            });
            let probe = ProbeVenue::new(
                venue,
                |venue: &mut BybitLinear, cx: &mut Cx<'_, Pending>| {
                    let symbol = if variant.is_inverse() {
                        "BTCUSD"
                    } else {
                        "BTCUSDT"
                    };
                    cx.store.set_markets(vec![Market {
                        id: symbol.into(),
                        symbol: symbol.into(),
                        base: "BTC".into(),
                        quote: if variant.is_inverse() { "USD" } else { "USDT" }.into(),
                        active: true,
                        precision: Precision {
                            amount: 1.,
                            price: 0.1,
                        },
                        limits: Limits {
                            amount: MinMax { min: 1., max: 2. },
                            leverage: MinMax { min: 1., max: 100. },
                            price: None,
                        },
                        pretty_symbol: None,
                        contract_size: None,
                        is_expiring: None,
                        is_inverse: Some(variant.is_inverse()),
                        is_spot: Some(variant.is_spot()),
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    }]);
                    let input = r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":5,"price":100}"#;
                    let mut opts: PlaceOrderOpts = serde_json::from_str(input).unwrap();
                    opts.symbol = symbol.into();
                    opts.side = side;
                    opts.take_profit = take_profit;
                    opts.stop_loss = stop_loss;
                    let lots = venue.format_lots(cx, &opts).unwrap();
                    let quantities = if variant.is_inverse() {
                        // KNOWN-ISSUE: legacy TS under-allocates (sum 2 of requested 5); kept for exact TS parity per Sam's decision
                        &["2"][..]
                    } else {
                        // KNOWN-ISSUE: legacy TS under-allocates (sum 4 of requested 5); kept for exact TS parity per Sam's decision
                        &["1", "1", "1", "1"][..]
                    };
                    assert_eq!(lots.len(), quantities.len());
                    let mut total = 0.;
                    for (index, lot) in lots.into_iter().enumerate() {
                        let body = if single { lot.single } else { lot.item }.finish();
                        let sent: CreateLot = serde_json::from_str(&body).unwrap();
                        assert_eq!(sent.qty, quantities[index]);
                        total += sent.qty.parse::<f64>().unwrap();
                        assert_eq!(
                            sent.category.as_deref(),
                            single.then_some(venue.category_for(symbol).as_str()),
                        );
                        let protected = index == 0 || index == quantities.len() - 1;
                        assert_eq!(
                            sent.take_profit,
                            take_profit.filter(|_| protected).map(number),
                            "{variant:?} lot {index}, single={single}"
                        );
                        assert_eq!(
                            sent.stop_loss,
                            stop_loss.filter(|_| protected).map(number),
                            "{variant:?} lot {index}, single={single}"
                        );
                        assert_eq!(
                            sent.tp_order_type.as_deref(),
                            (variant.is_spot() && protected && take_profit.is_some())
                                .then_some("Market")
                        );
                        assert_eq!(
                            sent.sl_order_type.as_deref(),
                            (variant.is_spot() && protected && stop_loss.is_some())
                                .then_some("Market")
                        );
                    }
                    assert_eq!(total, if variant.is_inverse() { 2. } else { 4. });
                },
            );
            seeded_engine(probe).start(1000.);
        }
    }
}

#[test]
fn split_spot_limit_protects_first_main_and_remainder_single_bodies() {
    check_lots(Variant::Spot, true);
}

#[test]
fn split_spot_limit_protects_first_main_and_remainder_batch_bodies() {
    check_lots(Variant::Spot, false);
}

#[test]
fn split_derivative_limits_keep_ts_first_and_remainder_protection() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Demo,
        Variant::Hyrotrader,
    ] {
        for single in [false, true] {
            check_lots(variant, single);
        }
    }
}
