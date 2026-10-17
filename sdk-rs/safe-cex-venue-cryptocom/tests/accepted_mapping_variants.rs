#[test]
fn native_map_order_type() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":null,"output":"Limit"},{"input":[109,97,114,107,101,116],"output":"Market"},{"input":[108,105,109,105,116],"output":"Limit"},{"input":[115,116,111,112,95,109,97,114,107,101,116],"output":"StopLoss"},{"input":[116,97,107,101,95,112,114,111,102,105,116,95,109,97,114,107,101,116],"output":"TakeProfit"},{"input":[99,111,110,100,105,116,105,111,110,97,108],"output":"Limit"},{"input":[116,114,97,105,108,105,110,103,95,115,116,111,112,95,109,97,114,107,101,116],"output":"Limit"},{"input":[],"output":"Limit"},{"input":[77,65,82,75,69,84],"output":"Limit"},{"input":[32,109,97,114,107,101,116],"output":"Limit"},{"input":[109,97,114,107,101,116,0],"output":"Limit"},{"input":[109,97,114,107,101,116,55296],"output":"Limit"},{"input":[56320,109,97,114,107,101,116],"output":"Limit"},{"input":[55296],"output":"Limit"},{"input":[56320],"output":"Limit"},{"input":[55357,56898],"output":"Limit"},{"input":[76,73,77,73,84],"output":"Limit"},{"input":[116,97,107,101,95,112,114,111,102,105,116,95,109,97,114,107,101,116,88],"output":"Limit"}]"###).unwrap();
    assert_eq!(cases.len(), 18, "map_order_type independent fixture count");
    let mut executed = 0usize;
    for case in cases {
        let value = &case["input"];
        let observed = {
            let units: Option<Vec<u16>> = serde_json::from_value(value.clone()).unwrap();
            serde_json::json!(
                match safe_cex_venue_cryptocom::order_mappings::map_order_type(units.as_deref()) {
                    safe_cex_venue_cryptocom::wire::CryptocomOrderType::Limit => "Limit",
                    safe_cex_venue_cryptocom::wire::CryptocomOrderType::Market => "Market",
                    safe_cex_venue_cryptocom::wire::CryptocomOrderType::StopLoss => "StopLoss",
                    safe_cex_venue_cryptocom::wire::CryptocomOrderType::StopLimit => "StopLimit",
                    safe_cex_venue_cryptocom::wire::CryptocomOrderType::TakeProfit => "TakeProfit",
                    safe_cex_venue_cryptocom::wire::CryptocomOrderType::TakeProfitLimit =>
                        "TakeProfitLimit",
                }
            )
        };
        assert_eq!(
            observed, case["output"],
            "map_order_type native variant input {value}"
        );
        executed += 1;
    }
    assert_eq!(executed, 18, "map_order_type executed fixture count");
}

#[test]
fn native_map_time_in_force() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":null,"output":"GoodTillCancel"},{"input":[71,111,111,100,84,105,108,108,67,97,110,99,101,108],"output":"GoodTillCancel"},{"input":[73,109,109,101,100,105,97,116,101,79,114,67,97,110,99,101,108],"output":"ImmediateOrCancel"},{"input":[70,105,108,108,79,114,75,105,108,108],"output":"FillOrKill"},{"input":[80,111,115,116,79,110,108,121],"output":"PostOnly"},{"input":[],"output":"GoodTillCancel"},{"input":[80,79,83,84,95,79,78,76,89],"output":"GoodTillCancel"},{"input":[102,105,108,108,111,114,107,105,108,108],"output":"GoodTillCancel"},{"input":[32,70,105,108,108,79,114,75,105,108,108],"output":"GoodTillCancel"},{"input":[70,105,108,108,79,114,75,105,108,108,0],"output":"GoodTillCancel"},{"input":[70,105,108,108,79,114,75,105,108,108,55296],"output":"GoodTillCancel"},{"input":[56320,80,111,115,116,79,110,108,121],"output":"GoodTillCancel"},{"input":[55296],"output":"GoodTillCancel"},{"input":[56320],"output":"GoodTillCancel"},{"input":[55357,56898],"output":"GoodTillCancel"},{"input":[71,79,79,68,95,84,73,76,76,95,67,65,78,67,69,76],"output":"GoodTillCancel"}]"###).unwrap();
    assert_eq!(
        cases.len(),
        16,
        "map_time_in_force independent fixture count"
    );
    let mut executed = 0usize;
    for case in cases {
        let value = &case["input"];
        let observed = {
            let units: Option<Vec<u16>> = serde_json::from_value(value.clone()).unwrap();
            serde_json::json!(
                match safe_cex_venue_cryptocom::order_mappings::map_time_in_force(units.as_deref())
                {
                    safe_cex_venue_cryptocom::wire::CryptocomTimeInForce::GoodTillCancel =>
                        "GoodTillCancel",
                    safe_cex_venue_cryptocom::wire::CryptocomTimeInForce::ImmediateOrCancel =>
                        "ImmediateOrCancel",
                    safe_cex_venue_cryptocom::wire::CryptocomTimeInForce::FillOrKill =>
                        "FillOrKill",
                    safe_cex_venue_cryptocom::wire::CryptocomTimeInForce::PostOnly => "PostOnly",
                }
            )
        };
        assert_eq!(
            observed, case["output"],
            "map_time_in_force native variant input {value}"
        );
        executed += 1;
    }
    assert_eq!(executed, 16, "map_time_in_force executed fixture count");
}

#[test]
fn native_map_stop_trigger_type() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":null,"output":"Mark"},{"input":[109,97,114,107,95,112,114,105,99,101],"output":"Mark"},{"input":[108,97,115,116,95,112,114,105,99,101],"output":"Last"},{"input":[105,110,100,101,120,95,112,114,105,99,101],"output":"Index"},{"input":[],"output":"Mark"},{"input":[73,78,68,69,88,95,80,82,73,67,69],"output":"Mark"},{"input":[77,97,114,107,80,114,105,99,101],"output":"Mark"},{"input":[32,105,110,100,101,120,95,112,114,105,99,101],"output":"Mark"},{"input":[108,97,115,116,95,112,114,105,99,101,0],"output":"Mark"},{"input":[105,110,100,101,120,95,112,114,105,99,101,55296],"output":"Mark"},{"input":[56320,108,97,115,116,95,112,114,105,99,101],"output":"Mark"},{"input":[55296],"output":"Mark"},{"input":[56320],"output":"Mark"},{"input":[55357,56898],"output":"Mark"},{"input":[109,97,114,107,101,116],"output":"Mark"},{"input":[105,110,100,101,120,95,112,114,105,99,101,88],"output":"Mark"}]"###).unwrap();
    assert_eq!(
        cases.len(),
        16,
        "map_stop_trigger_type independent fixture count"
    );
    let mut executed = 0usize;
    for case in cases {
        let value = &case["input"];
        let observed = {
            let units: Option<Vec<u16>> = serde_json::from_value(value.clone()).unwrap();
            serde_json::json!(
                match safe_cex_venue_cryptocom::order_mappings::map_stop_trigger_type(
                    units.as_deref()
                ) {
                    safe_cex_venue_cryptocom::wire::CryptocomRefPriceType::Mark => "Mark",
                    safe_cex_venue_cryptocom::wire::CryptocomRefPriceType::Index => "Index",
                    safe_cex_venue_cryptocom::wire::CryptocomRefPriceType::Last => "Last",
                }
            )
        };
        assert_eq!(
            observed, case["output"],
            "map_stop_trigger_type native variant input {value}"
        );
        executed += 1;
    }
    assert_eq!(executed, 16, "map_stop_trigger_type executed fixture count");
}
