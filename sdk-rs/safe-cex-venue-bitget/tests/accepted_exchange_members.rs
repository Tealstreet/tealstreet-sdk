//! Actual selected Exchange methods over scripted native HTTP; no venue replacement.
use safe_cex_venue_bitget::BitgetExchange;

#[test]
fn accepted_fetch_positions() {
    {
        let value = serde_json::json!(false);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let rows = h
                .drive(
                    <BitgetExchange as safe_cex_exchange::exchange::Exchange>::fetch_positions(
                        venue, None,
                    ),
                )
                .unwrap();
            serde_json::json!({"length":rows.len(),"id":rows[0].id,"symbol":rows[0].symbol,"contractsBits":format!("{:016x}",rows[0].contracts.to_bits()),"entryBits":format!("{:016x}",rows[0].entry_price.to_bits())})
        };
        let expected:serde_json::Value=serde_json::from_str(r#"{"length":1,"id":"BTCUSDT","symbol":"BTCUSDT","contractsBits":"4000000000000000","entryBits":"4059600000000000"}"#).unwrap();
        assert_eq!(actual, expected);
    }
    {
        let value = serde_json::json!(true);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let rows = h
                .drive(
                    <BitgetExchange as safe_cex_exchange::exchange::Exchange>::fetch_positions(
                        venue, None,
                    ),
                )
                .unwrap();
            serde_json::json!({"length":rows.len(),"id":rows[0].id,"symbol":rows[0].symbol,"contractsBits":format!("{:016x}",rows[0].contracts.to_bits()),"entryBits":format!("{:016x}",rows[0].entry_price.to_bits())})
        };
        let expected:serde_json::Value=serde_json::from_str(r#"{"length":1,"id":"ETHUSDT","symbol":"ETHUSDT","contractsBits":"4008000000000000","entryBits":"4069500000000000"}"#).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn accepted_fetch_orders() {
    {
        let value = serde_json::json!(false);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let rows = h
                .drive(
                    <BitgetExchange as safe_cex_exchange::exchange::Exchange>::fetch_orders(
                        venue, None,
                    ),
                )
                .unwrap();
            serde_json::json!({"length":rows.len(),"id":rows[0].id,"symbol":rows[0].symbol,"amountBits":format!("{:016x}",rows[0].amount.to_bits()),"priceBits":format!("{:016x}",rows[0].price.to_number().to_bits())})
        };
        let expected:serde_json::Value=serde_json::from_str(r#"{"length":1,"id":"native-a","symbol":"BTCUSDT","amountBits":"4000000000000000","priceBits":"4059600000000000"}"#).unwrap();
        assert_eq!(actual, expected);
    }
    {
        let value = serde_json::json!(true);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let rows = h
                .drive(
                    <BitgetExchange as safe_cex_exchange::exchange::Exchange>::fetch_orders(
                        venue, None,
                    ),
                )
                .unwrap();
            serde_json::json!({"length":rows.len(),"id":rows[0].id,"symbol":rows[0].symbol,"amountBits":format!("{:016x}",rows[0].amount.to_bits()),"priceBits":format!("{:016x}",rows[0].price.to_number().to_bits())})
        };
        let expected:serde_json::Value=serde_json::from_str(r#"{"length":1,"id":"native-b","symbol":"ETHUSDT","amountBits":"4008000000000000","priceBits":"4069500000000000"}"#).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn accepted_set_leverage() {
    {
        let value = serde_json::json!(false);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let symbol = if alternate { "ETHUSDT" } else { "BTCUSDT" };
            h.drive(
                <BitgetExchange as safe_cex_exchange::exchange::Exchange>::set_leverage(
                    venue,
                    symbol.into(),
                    if alternate { 13.0 } else { 7.0 },
                    None,
                ),
            )
            .unwrap();
            let posted = fx::posted(&h, "/api/v2/mix/account/set-leverage");
            serde_json::json!({"symbol":posted["symbol"],"leverageBits":format!("{:016x}",posted["leverage"].as_f64().unwrap().to_bits()),"posts":h.http.calls("/api/v2/mix/account/set-leverage").len(),"positionReads":h.http.calls("/api/v2/mix/position/all-position").len()})
        };
        let expected: serde_json::Value = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","leverageBits":"401c000000000000","posts":1,"positionReads":1}"#,
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    {
        let value = serde_json::json!(true);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let symbol = if alternate { "ETHUSDT" } else { "BTCUSDT" };
            h.drive(
                <BitgetExchange as safe_cex_exchange::exchange::Exchange>::set_leverage(
                    venue,
                    symbol.into(),
                    if alternate { 13.0 } else { 7.0 },
                    None,
                ),
            )
            .unwrap();
            let posted = fx::posted(&h, "/api/v2/mix/account/set-leverage");
            serde_json::json!({"symbol":posted["symbol"],"leverageBits":format!("{:016x}",posted["leverage"].as_f64().unwrap().to_bits()),"posts":h.http.calls("/api/v2/mix/account/set-leverage").len(),"positionReads":h.http.calls("/api/v2/mix/position/all-position").len()})
        };
        let expected: serde_json::Value = serde_json::from_str(
            r#"{"symbol":"ETHUSDT","leverageBits":"402a000000000000","posts":1,"positionReads":1}"#,
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn accepted_change_position_mode() {
    {
        let value = serde_json::json!(false);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            h.drive(
                <BitgetExchange as safe_cex_exchange::exchange::Exchange>::change_position_mode(
                    venue, alternate, None,
                ),
            )
            .unwrap();
            let posted = fx::posted(&h, "/api/v2/mix/account/set-position-mode");
            serde_json::json!({"mode":posted["posMode"],"product":posted["productType"],"posts":h.http.calls("/api/v2/mix/account/set-position-mode").len()})
        };
        let expected: serde_json::Value =
            serde_json::from_str(r#"{"mode":"one_way_mode","product":"usdt-futures","posts":1}"#)
                .unwrap();
        assert_eq!(actual, expected);
    }
    {
        let value = serde_json::json!(true);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            h.drive(
                <BitgetExchange as safe_cex_exchange::exchange::Exchange>::change_position_mode(
                    venue, alternate, None,
                ),
            )
            .unwrap();
            let posted = fx::posted(&h, "/api/v2/mix/account/set-position-mode");
            serde_json::json!({"mode":posted["posMode"],"product":posted["productType"],"posts":h.http.calls("/api/v2/mix/account/set-position-mode").len()})
        };
        let expected: serde_json::Value =
            serde_json::from_str(r#"{"mode":"hedge_mode","product":"usdt-futures","posts":1}"#)
                .unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn accepted_change_margin_type() {
    {
        let value = serde_json::json!(false);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let symbol = if alternate { "ETHUSDT" } else { "BTCUSDT" };
            h.drive(
                <BitgetExchange as safe_cex_exchange::exchange::Exchange>::change_margin_type(
                    venue,
                    alternate,
                    Some(symbol.into()),
                ),
            )
            .unwrap();
            let posted = fx::posted(&h, "/api/v2/mix/account/set-margin-mode");
            serde_json::json!({"symbol":posted["symbol"],"marginMode":posted["marginMode"],"posts":h.http.calls("/api/v2/mix/account/set-margin-mode").len()})
        };
        let expected: serde_json::Value =
            serde_json::from_str(r#"{"symbol":"BTCUSDT","marginMode":"crossed","posts":1}"#)
                .unwrap();
        assert_eq!(actual, expected);
    }
    {
        let value = serde_json::json!(true);
        let actual = {
            #[allow(dead_code)]
            mod fx {
                use safe_cex_venue_bitget as venue_crate;
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../safe-cex-parity/accepted-set/bitget-exchange-fixture.rs"
                ));
            }
            let alternate = value == true;
            let h = fx::setup(alternate);
            let venue: &BitgetExchange = h.exchange.as_ref();
            let symbol = if alternate { "ETHUSDT" } else { "BTCUSDT" };
            h.drive(
                <BitgetExchange as safe_cex_exchange::exchange::Exchange>::change_margin_type(
                    venue,
                    alternate,
                    Some(symbol.into()),
                ),
            )
            .unwrap();
            let posted = fx::posted(&h, "/api/v2/mix/account/set-margin-mode");
            serde_json::json!({"symbol":posted["symbol"],"marginMode":posted["marginMode"],"posts":h.http.calls("/api/v2/mix/account/set-margin-mode").len()})
        };
        let expected: serde_json::Value =
            serde_json::from_str(r#"{"symbol":"ETHUSDT","marginMode":"isolated","posts":1}"#)
                .unwrap();
        assert_eq!(actual, expected);
    }
}
