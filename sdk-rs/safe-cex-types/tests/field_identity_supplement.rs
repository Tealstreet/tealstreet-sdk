//! Field-identity supplement; original corpora are preserved byte-for-byte.
//! Added after symmetric roundtrip blindness was discovered; no retroactive independence claim.
use safe_cex_types::{Balance, Order, StoreDataLoaded, Ticker};
use serde_json::json;

#[test]
fn dynamic_rejected_shapes_are_not_positive_record_fixtures() {
    use safe_cex_types::OrderStatus;
    assert!(matches!(
        serde_json::from_value::<OrderStatus>(json!("open")).unwrap(),
        OrderStatus::Open
    ));
    for wrong in [json!({"open":null}), json!(["open"]), json!(null)] {
        assert!(
            serde_json::from_value::<OrderStatus>(wrong).is_err(),
            "DYNAMIC-REJECTION string union refuses non-string shapes"
        );
    }
}

#[test]
fn dynamic_operation_outputs_keep_tuple_identity_without_roundtrip() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_symbols_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let row = corpus["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["op"] == "parseBinanceSymbol" && row["symbol"] == "BTCUSDT")
        .unwrap();
    let (base, quote) = safe_cex_types::symbol_converter::parse_binance_symbol("BTCUSDT");
    assert_eq!(
        base,
        row["expected"]["base"].as_str().unwrap(),
        "DYNAMIC-OPERATION native base identity"
    );
    assert_eq!(
        quote,
        row["expected"]["quote"].as_str().unwrap(),
        "DYNAMIC-OPERATION native quote identity"
    );
    assert_ne!(base, quote);
}

#[test]
fn decoded_balance_and_ticker_numeric_fields_have_native_identity() {
    let balance: Balance =
        serde_json::from_value(json!({"used":11,"free":23,"total":37,"upnl":41})).unwrap();
    assert_eq!(balance.used, 11.0, "FIELD-IDENTITY Balance.used");
    assert_eq!(balance.free, 23.0, "FIELD-IDENTITY Balance.free");
    assert_eq!(balance.total, 37.0, "FIELD-IDENTITY Balance.total");
    assert_eq!(balance.upnl, 41.0, "FIELD-IDENTITY Balance.upnl");
    let ticker: Ticker=serde_json::from_value(json!({"id":"wire-id","symbol":"wire-symbol","bid":11,"ask":23,"last":37,"mark":41,"index":53,"percentage":67,"openInterest":79,"fundingRate":83,"fundingTime":97,"volume":101,"quoteVolume":113})).unwrap();
    assert_eq!(ticker.id, "wire-id", "FIELD-IDENTITY Ticker.id");
    assert_eq!(ticker.symbol, "wire-symbol", "FIELD-IDENTITY Ticker.symbol");
    for (field, actual, expected) in [
        ("bid", ticker.bid, 11.0_f64),
        ("ask", ticker.ask, 23.0),
        ("last", ticker.last, 37.0),
        ("mark", ticker.mark, 41.0),
        ("index", ticker.index.unwrap(), 53.0),
        ("percentage", ticker.percentage, 67.0),
        ("openInterest", ticker.open_interest, 79.0),
        ("fundingRate", ticker.funding_rate, 83.0),
        ("fundingTime", ticker.funding_time, 97.0),
        ("volume", ticker.volume, 101.0),
        ("quoteVolume", ticker.quote_volume, 113.0),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "FIELD-IDENTITY Ticker.{field}"
        );
    }
}
#[test]
fn decoded_loaded_flags_are_one_hot_and_orders_read_native_fields() {
    for selected in ["balance", "orders", "markets", "tickers", "positions"] {
        let mut raw = json!({"balance":false,"orders":false,"markets":false,"tickers":false,"positions":false});
        raw[selected] = json!(true);
        let loaded: StoreDataLoaded = serde_json::from_value(raw).unwrap();
        for (field, value) in [
            ("balance", loaded.balance),
            ("orders", loaded.orders),
            ("markets", loaded.markets),
            ("tickers", loaded.tickers),
            ("positions", loaded.positions),
        ] {
            assert_eq!(
                value,
                field == selected,
                "FIELD-IDENTITY StoreDataLoaded.{field} selected {selected}"
            );
        }
    }
    let order:Order=serde_json::from_value(json!({"id":"order-id","symbol":"order-symbol","status":"open","type":"limit","side":"buy","price":11,"amount":23,"filled":5,"remaining":18,"reduceOnly":true,"lastUpdate":41})).unwrap();
    assert_eq!(order.id, "order-id", "FIELD-IDENTITY Order.id");
    assert_eq!(order.symbol, "order-symbol", "FIELD-IDENTITY Order.symbol");
    for (field, actual, expected) in [
        ("price", order.price.to_number(), 11.0_f64),
        ("amount", order.amount, 23.0),
        ("filled", order.filled, 5.0),
        ("remaining", order.remaining, 18.0),
        ("lastUpdate", order.last_update, 41.0),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "FIELD-IDENTITY Order.{field}"
        );
    }
}

// Capture boundary for this later supplement: canonicalize NaN only.
fn capture_bits(number: f64) -> u64 {
    if number.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        number.to_bits()
    }
}
#[test]
fn supplement_nan_capture_keeps_signed_zero_and_finite_bits() {
    assert_eq!(
        capture_bits(f64::from_bits(0xfff8_0000_0000_0000)),
        0x7ff8_0000_0000_0000
    );
    assert_eq!(
        capture_bits(f64::from_bits(0x7ff8_0000_0000_0001)),
        0x7ff8_0000_0000_0000
    );
    assert_eq!(
        capture_bits(-0.0),
        0x8000_0000_0000_0000,
        "CAPTURE-BITS signed zero remains observable"
    );
    assert_eq!(
        capture_bits(11.0),
        0x4026_0000_0000_0000,
        "CAPTURE-BITS finite payload remains exact"
    );
    assert_eq!(capture_bits(f64::INFINITY), 0x7ff0_0000_0000_0000);
    assert_eq!(capture_bits(f64::NEG_INFINITY), 0xfff0_0000_0000_0000);
}

#[test]
fn decoded_nested_and_history_models_pin_native_field_identity() {
    use safe_cex_types::{
        Candle, Execution, Market, Position, PositionRecord, RequestOutcomeCounts,
    };
    macro_rules! numbers {
        ($model:ident; $($field:ident = $value:expr),+ $(,)?) => {
            $(assert_eq!($model.$field.to_bits(), ($value as f64).to_bits(),
                concat!("FIELD-IDENTITY ", stringify!($model), ".", stringify!($field)));)+
        };
    }
    let position: Position = serde_json::from_value(json!({"id":"position-id","symbol":"position-symbol","side":"long","entryPrice":11,"notional":23,"leverage":37,"unrealizedPnl":41,"contracts":53,"liquidationPrice":67,"realizedPnl":79,"marginSize":83,"lastUpdate":97})).unwrap();
    assert_eq!(position.id, "position-id");
    assert_eq!(position.symbol, "position-symbol");
    numbers!(position; entry_price=11, notional=23, leverage=37, unrealized_pnl=41, contracts=53, liquidation_price=67);
    assert_eq!(position.realized_pnl, Some(79.0));
    assert_eq!(position.margin_size, Some(83.0));
    assert_eq!(position.last_update, Some(97.0));
    let market: Market = serde_json::from_value(json!({"id":"market-id","symbol":"market-symbol","base":"base-value","quote":"quote-value","active":true,"precision":{"amount":11,"price":23},"limits":{"amount":{"min":37,"max":41},"leverage":{"min":53,"max":67}}})).unwrap();
    assert_eq!(
        (&*market.id, &*market.symbol, &*market.base, &*market.quote),
        ("market-id", "market-symbol", "base-value", "quote-value")
    );
    assert_eq!(
        (market.precision.amount, market.precision.price),
        (11.0, 23.0),
        "FIELD-IDENTITY nested precision"
    );
    assert_eq!(
        (market.limits.amount.min, market.limits.amount.max),
        (37.0, safe_cex_types::NumericValue::Number(41.0)),
        "FIELD-IDENTITY nested amount limits"
    );
    assert_eq!(
        (market.limits.leverage.min, market.limits.leverage.max),
        (53.0, 67.0),
        "FIELD-IDENTITY nested leverage limits"
    );
    let candle: Candle = serde_json::from_value(
        json!({"timestamp":11,"open":23,"high":37,"low":41,"close":53,"volume":67}),
    )
    .unwrap();
    numbers!(candle; timestamp=11, open=23, high=37, low=41, close=53, volume=67);
    let execution: Execution = serde_json::from_value(json!({"timestamp":11,"symbol":"execution-symbol","price":23,"qty":37,"side":"buy","realizedPnl":41,"commission":53,"venueReportedRevenue":67})).unwrap();
    numbers!(execution; timestamp=11, price=23, qty=37);
    assert_eq!(execution.realized_pnl, Some(41.0));
    assert_eq!(execution.commission, Some(53.0));
    assert_eq!(execution.venue_reported_revenue, Some(67.0));
    let counters: RequestOutcomeCounts =
        serde_json::from_value(json!({"ok":11,"rateLimit":23,"auth":37,"network":41,"other":53}))
            .unwrap();
    numbers!(counters; ok=11, rate_limit=23, auth=37, network=41, other=53);
    let history: PositionRecord = serde_json::from_value(json!({"key":"record-key","account":"record-account","symbol":"record-symbol","settleCurrency":"record-currency","side":"long","positionSide":"both","state":"closed","openedAt":11,"closedAt":23,"closedSize":37,"maxSize":41,"leverage":53,"closedNotional":67,"avgEntryPrice":79,"avgExitPrice":83,"realizedPnl":97,"fees":101,"funding":113,"fundingAttributed":false,"provenance":"reported","sourceAxis":"closed-positions","closeCount":127})).unwrap();
    assert_eq!(
        (
            &*history.key,
            &*history.account,
            &*history.symbol,
            &*history.settle_currency
        ),
        (
            "record-key",
            "record-account",
            "record-symbol",
            "record-currency"
        )
    );
    numbers!(history; closed_size=37, avg_entry_price=79, realized_pnl=97, fees=101, funding=113, close_count=127);
    assert_eq!(
        (
            history.opened_at,
            history.closed_at,
            history.max_size,
            history.leverage,
            history.closed_notional,
            history.avg_exit_price
        ),
        (
            Some(11.0),
            Some(23.0),
            Some(41.0),
            Some(53.0),
            Some(67.0),
            Some(83.0)
        ),
        "FIELD-IDENTITY PositionRecord optional numbers"
    );
}

#[test]
fn decoded_task_snapshots_use_distinct_dimensions_and_one_hot_flags() {
    use safe_cex_types::{ActiveChaser, ActiveTWAP, DisplayedMargin};
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../../safe-cex-store/src/store_stub.json")).unwrap();
    let mut chaser = original["activeChasers"][0].clone();
    for (field, value) in [
        ("id", json!("chaser-id")),
        ("symbol", json!("chaser-symbol")),
        ("stopReason", json!("chaser-reason")),
        ("size", json!(11)),
        ("maxAttempts", json!(23)),
        ("attempts", json!(37)),
    ] {
        chaser[field] = value;
    }
    let native: ActiveChaser = serde_json::from_value(chaser).unwrap();
    assert_eq!(
        (&*native.id, &*native.symbol, &*native.stop_reason),
        ("chaser-id", "chaser-symbol", "chaser-reason")
    );
    assert_eq!(
        (native.size, native.max_attempts, native.attempts),
        (11.0, 23.0, 37.0),
        "FIELD-IDENTITY ActiveChaser dimensions"
    );
    let dimensions = [
        ("size", 11),
        ("duration", 23),
        ("ordersCount", 37),
        ("chaserDelay", 41),
        ("randomnessPercentage", 53),
        ("ordersExecuted", 67),
        ("nextOrderAt", 79),
    ];
    for selected in ["reduceOnly", "useLimitOrders", "useChaserOrders"] {
        let mut twap = original["activeTWAPs"][0].clone();
        for (field, value) in dimensions {
            twap[field] = json!(value);
        }
        for flag in ["reduceOnly", "useLimitOrders", "useChaserOrders"] {
            twap[flag] = json!(flag == selected);
        }
        twap["id"] = json!("twap-id");
        twap["symbol"] = json!("twap-symbol");
        twap["accountName"] = json!("twap-account");
        let native: ActiveTWAP = serde_json::from_value(twap).unwrap();
        assert_eq!(
            (&*native.id, &*native.symbol, &*native.account_name),
            ("twap-id", "twap-symbol", "twap-account")
        );
        assert_eq!(
            (
                native.size,
                native.duration,
                native.orders_count,
                native.chaser_delay,
                native.randomness_percentage,
                native.orders_executed,
                native.next_order_at
            ),
            (11.0, 23.0, 37.0, 41.0, 53.0, 67.0, 79.0),
            "FIELD-IDENTITY ActiveTWAP dimensions"
        );
        assert_eq!(
            (
                native.reduce_only,
                native.use_limit_orders,
                native.use_chaser_orders
            ),
            (
                selected == "reduceOnly",
                selected == "useLimitOrders",
                selected == "useChaserOrders"
            ),
            "FIELD-IDENTITY ActiveTWAP one-hot flags"
        );
    }
    let margin: DisplayedMargin =
        serde_json::from_value(json!({"longExposure":11,"shortExposure":23,"totalNotional":37}))
            .unwrap();
    assert_eq!(
        (
            margin.long_exposure,
            margin.short_exposure,
            margin.total_notional
        ),
        (11.0, 23.0, 37.0),
        "FIELD-IDENTITY DisplayedMargin dimensions"
    );
}

#[test]
fn decoded_nested_extras_fills_and_book_levels_keep_native_identity() {
    use safe_cex_types::{OrderBook, OrderFillEvent, PositionExtra, UpdateOrderOptsUpdate};
    let extra: PositionExtra =
        serde_json::from_value(json!({"contractsNative":11,"unrealizedPnlNative":23})).unwrap();
    assert_eq!(
        (extra.contracts_native, extra.unrealized_pnl_native),
        (Some(11.0), Some(23.0)),
        "FIELD-IDENTITY PositionExtra dimensions"
    );
    let fill: OrderFillEvent=serde_json::from_value(json!({"id":"fill-id","symbol":"fill-symbol","orderId":"fill-order","amount":11,"price":23,"fee":37,"realizedPnl":41,"side":"buy"})).unwrap();
    assert_eq!(
        (&*fill.id, &*fill.symbol, fill.order_id.as_deref()),
        ("fill-id", "fill-symbol", Some("fill-order"))
    );
    assert_eq!(
        (fill.amount, fill.price, fill.fee, fill.realized_pnl),
        (11.0, 23.0, Some(37.0), Some(41.0)),
        "FIELD-IDENTITY OrderFillEvent dimensions"
    );
    let book: OrderBook=serde_json::from_value(json!({"bids":[{"price":11,"amount":23,"total":37}],"asks":[{"price":41,"amount":53,"total":67}]})).unwrap();
    assert_eq!(
        (book.bids[0].price, book.bids[0].amount, book.bids[0].total),
        (11.0, 23.0, 37.0),
        "FIELD-IDENTITY book bid dimensions"
    );
    assert_eq!(
        (book.asks[0].price, book.asks[0].amount, book.asks[0].total),
        (41.0, 53.0, 67.0),
        "FIELD-IDENTITY book ask dimensions"
    );
    let update: UpdateOrderOptsUpdate =
        serde_json::from_value(json!({"amount":11,"price":23})).unwrap();
    match update {
        UpdateOrderOptsUpdate::Variant0(value) => assert_eq!(
            (value.amount, value.price),
            (11.0, Some(23.0)),
            "FIELD-IDENTITY update union amount/price"
        ),
        UpdateOrderOptsUpdate::Variant1(value) => assert_eq!(
            (value.amount, value.price),
            (Some(11.0), 23.0),
            "FIELD-IDENTITY update union price/amount"
        ),
    }
    let update: UpdateOrderOptsUpdate = serde_json::from_value(json!({"price":37})).unwrap();
    match update {
        UpdateOrderOptsUpdate::Variant1(value) => assert_eq!(
            (value.amount, value.price),
            (None, 37.0),
            "FIELD-IDENTITY update union second arm price"
        ),
        UpdateOrderOptsUpdate::Variant0(_) => panic!("FIELD-IDENTITY update union second arm"),
    }
}
