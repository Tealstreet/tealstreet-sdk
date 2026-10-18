use safe_cex_venue_lighter::sdk_common::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
mod support;
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-common-types-oracle.cjs"
    )
}
// Native field projection deliberately never serializes the enclosing record:
// symmetric serde renames must not be able to undo a wrong decoded mapping.
trait NativeIdentity {
    fn native_identity(&self) -> Value;
}
impl NativeIdentity for Balance {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"tokenAddress": self.token_address,"available": self.available,"locked": self.locked,"total": self.total,"decimals": self.decimals})
    }
}
impl NativeIdentity for Ticker {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"lastPrice": self.last_price,"priceChange": self.price_change,"priceChangePercent": self.price_change_percent,"high24h": self.high24h,"low24h": self.low24h,"volume24h": self.volume24h,"quoteVolume24h": self.quote_volume24h,"bidPrice": self.bid_price,"bidQuantity": self.bid_quantity,"askPrice": self.ask_price,"askQuantity": self.ask_quantity,"timestamp": self.timestamp})
    }
}
impl NativeIdentity for OrderBookLevel {
    fn native_identity(&self) -> Value {
        json!({"price": self.price,"quantity": self.quantity})
    }
}
impl NativeIdentity for PaginationParams {
    fn native_identity(&self) -> Value {
        json!({"page": self.page,"pageSize": self.page_size,"startTime": self.start_time,"endTime": self.end_time})
    }
}
impl NativeIdentity for LighterConfig {
    fn native_identity(&self) -> Value {
        json!({"baseUrl": self.base_url,"wsUrl": self.ws_url,"apiKey": self.api_key,"apiSecret": self.api_secret,"privateKey": self.private_key,"chainId": self.chain_id,"timeout": self.timeout,"debug": self.debug,"maxRetries": self.max_retries,"retryDelay": self.retry_delay})
    }
}
impl NativeIdentity for Account {
    fn native_identity(&self) -> Value {
        json!({"address": self.address,"accountIndex": self.account_index,"publicKey": self.public_key,"nonce": self.nonce,"balances": self.balances.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"createdAt": self.created_at,"updatedAt": self.updated_at})
    }
}
impl NativeIdentity for Market {
    fn native_identity(&self) -> Value {
        json!({"marketId": self.market_id,"symbol": self.symbol,"baseToken": self.base_token,"quoteToken": self.quote_token,"baseTokenAddress": self.base_token_address,"quoteTokenAddress": self.quote_token_address,"minOrderSize": self.min_order_size,"maxOrderSize": self.max_order_size,"priceTickSize": self.price_tick_size,"quantityTickSize": self.quantity_tick_size,"makerFee": self.maker_fee,"takerFee": self.taker_fee,"status": self.status,"baseDecimals": self.base_decimals,"quoteDecimals": self.quote_decimals})
    }
}
impl NativeIdentity for Position {
    fn native_identity(&self) -> Value {
        json!({"marketId": self.market_id,"symbol": self.symbol,"side": self.side,"size": self.size,"entryPrice": self.entry_price,"markPrice": self.mark_price,"liquidationPrice": self.liquidation_price,"unrealizedPnl": self.unrealized_pnl,"realizedPnl": self.realized_pnl,"margin": self.margin,"leverage": self.leverage,"notional": self.notional,"createdAt": self.created_at,"updatedAt": self.updated_at})
    }
}
impl NativeIdentity for Trade {
    fn native_identity(&self) -> Value {
        json!({"tradeId": self.trade_id,"symbol": self.symbol,"price": self.price,"quantity": self.quantity,"quoteQuantity": self.quote_quantity,"side": self.side,"timestamp": self.timestamp,"isBuyerMaker": self.is_buyer_maker})
    }
}
impl NativeIdentity for OrderBook {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"bids": self.bids.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"asks": self.asks.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"lastUpdateId": self.last_update_id,"timestamp": self.timestamp})
    }
}
fn record<T: DeserializeOwned + Serialize + NativeIdentity>(
    oracle: &Value,
    name: &str,
    complete: Value,
) {
    let schema = oracle["schemas"][name].as_array().unwrap();
    let object = complete.as_object().unwrap();
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<_>>(),
        schema
            .iter()
            .map(|f| f["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        "{name} serialized field inventory"
    );
    let parsed = T::deserialize(complete.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        complete,
        "{name} complete roundtrip"
    );
    // Edge-case corpora may deliberately repeat values; independently pin each
    // leaf's identity through this outer decoder, including nested records.
    fn identity_cases(
        schema: &Value,
        name: &str,
        value: &Value,
        path: &str,
    ) -> Vec<(String, Value)> {
        let mut cases = Vec::new();
        for field in schema[name].as_array().unwrap() {
            let key = field["name"].as_str().unwrap();
            let kind = field["type"].as_str().unwrap();
            let label = format!("{path}.{key}");
            let alternatives = match kind {
                "string" => vec![json!(format!("identity:{label}"))],
                "number" => vec![json!(1234567.75)],
                "boolean" => vec![json!(false), json!(true)],
                _ => Vec::new(),
            };
            for alternative in alternatives {
                let mut changed = value.clone();
                changed[key] = alternative;
                cases.push((label.clone(), changed));
            }
            if let Some(child) = kind.strip_suffix("[]") {
                for (i, item) in value[key].as_array().unwrap().iter().enumerate() {
                    for (label, changed_child) in
                        identity_cases(schema, child, item, &format!("{label}[{i}]"))
                    {
                        let mut changed = value.clone();
                        changed[key][i] = changed_child;
                        cases.push((label, changed));
                    }
                }
            }
        }
        cases
    }
    for (field, input) in identity_cases(&oracle["schemas"], name, &complete, name) {
        let decoded = T::deserialize(input.clone()).unwrap();
        assert_eq!(
            decoded.native_identity(),
            input,
            "{field} native field identity"
        );
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            input,
            "{field} independent field identity roundtrip"
        );
    }
    for field in schema {
        let key = field["name"].as_str().unwrap();
        let mut missing = object.clone();
        missing.shift_remove(key);
        assert_eq!(
            T::deserialize(Value::Object(missing)).is_ok(),
            field["optional"].as_bool().unwrap(),
            "{name}.{key} required/optional"
        );
        let kind = field["type"].as_str().unwrap();
        let mut invalid = vec![Value::Null, json!({})];
        if kind != "boolean" {
            invalid.push(json!(false));
        }
        if !kind.ends_with("[]") {
            invalid.push(json!([]));
        }
        if kind != "number" {
            invalid.push(json!(16777217.25));
        }
        if kind != "string" {
            invalid.push(json!("not-a-valid-member"));
        }
        for bad in invalid {
            let mut altered = object.clone();
            altered.insert(key.into(), bad);
            assert!(
                T::deserialize(Value::Object(altered)).is_err(),
                "{name}.{key} rejects null/wrong shape"
            );
        }
    }
    for field in schema
        .iter()
        .filter(|f| f["type"].as_str().unwrap().ends_with("[]"))
    {
        let key = field["name"].as_str().unwrap();
        let nested = object[key][0].as_object().unwrap();
        for child in nested.keys() {
            let mut value = complete.clone();
            value[key][0].as_object_mut().unwrap().shift_remove(child);
            assert!(
                T::deserialize(value).is_err(),
                "{name}.{key}.{child} nested required field"
            );
        }
        for bad in [json!(null), json!([]), json!(false), json!(1), json!("bad")] {
            let mut value = complete.clone();
            value[key][0] = bad;
            assert!(
                T::deserialize(value).is_err(),
                "{name}.{key} nested record shape"
            );
        }
    }
    for field in schema
        .iter()
        .filter(|f| f["type"].as_str().unwrap().contains(" | ") || f["type"] == "MarketStatus")
    {
        let key = field["name"].as_str().unwrap();
        let kind = field["type"].as_str().unwrap();
        let accepted: Vec<Value> = if kind == "MarketStatus" {
            oracle["statuses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry[1].clone())
                .collect()
        } else {
            kind.split(" | ")
                .map(|s| serde_json::from_str(s).unwrap())
                .collect()
        };
        let metadata = match kind {
            "MarketStatus" => support::enum_inventory::<MarketStatus>(),
            "\"long\" | \"short\"" => support::enum_inventory::<safe_cex_types::PositionSide>(),
            "\"buy\" | \"sell\"" => support::enum_inventory::<safe_cex_types::OrderSide>(),
            _ => panic!("unknown literal domain"),
        };
        assert_eq!(
            json!(metadata),
            json!(accepted),
            "{name}.{key} accepted metadata"
        );
        for literal in &accepted {
            let mut value = complete.clone();
            value[key] = literal.clone();
            let parsed = T::deserialize(value.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap(),
                value,
                "{name}.{key} literal roundtrip"
            );
            for payload in [json!(null), json!({}), json!([])] {
                let mut value = complete.clone();
                value[key] = json!({literal.as_str().unwrap():payload});
                assert!(
                    T::deserialize(value).is_err(),
                    "{name}.{key} tagged enum rejected"
                );
            }
        }
    }
    let positional = Value::Array(object.values().cloned().collect());
    for bad in [
        positional,
        json!([]),
        json!(null),
        json!(false),
        json!(1),
        json!("record"),
    ] {
        let error = T::deserialize(bad.clone()).err().unwrap().to_string();
        assert!(
            error.contains("invalid type") && (error.contains("map") || error.contains("struct")),
            "{name} shape {bad}: {error}"
        );
    }
    let expected_units = if schema.iter().any(|field| field["type"] == "string") {
        vec![
            vec![112, 108, 97, 105, 110],
            vec![233],
            vec![0xd83d, 0xdc69],
            vec![0xfeff],
            vec![0x85],
            vec![0xd800],
            vec![0xdfff],
            vec![120, 0xd800, 121],
        ]
    } else {
        vec![]
    };
    assert_eq!(
        json!(expected_units),
        json!(
            oracle["witnesses"][name]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w["units"].clone())
                .collect::<Vec<_>>()
        ),
        "{name} UTF16 witness inventory"
    );
    for witness in oracle["witnesses"][name].as_array().unwrap() {
        let encoded = witness["encoded"].as_str().unwrap();
        let result = serde_json::from_str::<Value>(encoded).and_then(serde_json::from_value::<T>);
        if witness["representable"].as_bool().unwrap() {
            let parsed = serde_json::to_value(result.unwrap()).unwrap();
            for field in schema.iter().filter(|f| f["type"] == "string") {
                let units: Vec<_> = parsed[field["name"].as_str().unwrap()]
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .collect();
                assert_eq!(
                    json!(units),
                    witness["units"],
                    "{name} scalar/astral UTF16 retention"
                );
            }
        } else {
            assert!(result.is_err(), "{name} upstream Value refusal: {encoded}");
            let _: T = typed_utf16::decode(encoded);
            assert!(
                encoded.contains("\\ud"),
                "escaped valid JSON surrogate witness"
            );
        }
    }
}
fn overflow<T: DeserializeOwned>(oracle: &Value, name: &str, read: impl Fn(&T, &str) -> f64) {
    let witnesses = oracle["overflow"][name].as_array().unwrap();
    let count = oracle["schemas"][name]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["type"] == "number")
        .count();
    assert_eq!(
        witnesses.len(),
        5 * count,
        "{name} numeric overflow witness inventory"
    );
    for witness in witnesses {
        let raw = witness["raw"].as_str().unwrap();
        // Diagnostic: direct typed parsing bypasses canonical Value-first ingress.
        // Flip this control when the separately routed shared serde repair lands.
        assert!(
            !raw.contains("e400") || serde_json::from_str::<T>(raw).is_err(),
            "{name} raw parser bypass behavior changed"
        );
        let value: Value = serde_json::from_str(raw).unwrap();
        let parsed = serde_json::from_value::<T>(value);
        assert!(
            parsed.is_ok(),
            "{name} canonical Value-first overflow ingress"
        );
        let parsed = parsed.unwrap();
        let field = witness["field"].as_str().unwrap();
        let mut existing: Value = serde_json::from_str(raw).unwrap();
        existing[field] = json!(-0.0);
        let existing = serde_json::from_value::<T>(existing).unwrap();
        assert_eq!(
            read(&existing, field).to_bits(),
            (-0.0_f64).to_bits(),
            "{name}.{field} existing Value signed zero"
        );
        let actual = read(&parsed, witness["field"].as_str().unwrap());
        assert_eq!(
            format!("{:016x}", actual.to_bits()),
            witness["bits"].as_str().unwrap(),
            "{name} Value-first numeric bits match Node: {raw}"
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_common_types_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let value = Balance {
        symbol: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        token_address: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        available: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        locked: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        total: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        decimals: 16777217.25,
    };
    let _: &String = &value.symbol;
    let _: &String = &value.token_address;
    let _: &String = &value.available;
    let _: &String = &value.locked;
    let _: &String = &value.total;
    let _: &f64 = &value.decimals;
    assert_eq!(
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "tokenAddress", "type": "string", "optional": false}, {"name": "available", "type": "string", "optional": false}, {"name": "locked", "type": "string", "optional": false}, {"name": "total", "type": "string", "optional": false}, {"name": "decimals", "type": "number", "optional": false}]),
        oracle["schemas"]["Balance"],
        "Balance field schema"
    );
    record::<Balance>(&oracle, "Balance", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        Balance::deserialize(positional).is_err(),
        "Balance public qualified shape gate"
    );
    let value = Ticker {
        symbol: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        last_price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        price_change: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        price_change_percent: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        high24h: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        low24h: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        volume24h: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quote_volume24h: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        bid_price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        bid_quantity: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        ask_price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        ask_quantity: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        timestamp: 16777217.25,
    };
    let _: &String = &value.symbol;
    let _: &String = &value.last_price;
    let _: &String = &value.price_change;
    let _: &String = &value.price_change_percent;
    let _: &String = &value.high24h;
    let _: &String = &value.low24h;
    let _: &String = &value.volume24h;
    let _: &String = &value.quote_volume24h;
    let _: &String = &value.bid_price;
    let _: &String = &value.bid_quantity;
    let _: &String = &value.ask_price;
    let _: &String = &value.ask_quantity;
    let _: &f64 = &value.timestamp;
    assert_eq!(
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "lastPrice", "type": "string", "optional": false}, {"name": "priceChange", "type": "string", "optional": false}, {"name": "priceChangePercent", "type": "string", "optional": false}, {"name": "high24h", "type": "string", "optional": false}, {"name": "low24h", "type": "string", "optional": false}, {"name": "volume24h", "type": "string", "optional": false}, {"name": "quoteVolume24h", "type": "string", "optional": false}, {"name": "bidPrice", "type": "string", "optional": false}, {"name": "bidQuantity", "type": "string", "optional": false}, {"name": "askPrice", "type": "string", "optional": false}, {"name": "askQuantity", "type": "string", "optional": false}, {"name": "timestamp", "type": "number", "optional": false}]),
        oracle["schemas"]["Ticker"],
        "Ticker field schema"
    );
    record::<Ticker>(&oracle, "Ticker", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        Ticker::deserialize(positional).is_err(),
        "Ticker public qualified shape gate"
    );
    let value = OrderBookLevel {
        price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quantity: "fixture 👩 é \u{feff} \u{85}".to_owned(),
    };
    let _: &String = &value.price;
    let _: &String = &value.quantity;
    assert_eq!(
        json!([{"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}]),
        oracle["schemas"]["OrderBookLevel"],
        "OrderBookLevel field schema"
    );
    record::<OrderBookLevel>(
        &oracle,
        "OrderBookLevel",
        serde_json::to_value(&value).unwrap(),
    );
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        OrderBookLevel::deserialize(positional).is_err(),
        "OrderBookLevel public qualified shape gate"
    );
    let value = PaginationParams {
        page: Some(16777217.25),
        page_size: Some(16777217.25),
        start_time: Some(16777217.25),
        end_time: Some(16777217.25),
    };
    let _: &Option<f64> = &value.page;
    let _: &Option<f64> = &value.page_size;
    let _: &Option<f64> = &value.start_time;
    let _: &Option<f64> = &value.end_time;
    assert_eq!(
        json!([{"name": "page", "type": "number", "optional": true}, {"name": "pageSize", "type": "number", "optional": true}, {"name": "startTime", "type": "number", "optional": true}, {"name": "endTime", "type": "number", "optional": true}]),
        oracle["schemas"]["PaginationParams"],
        "PaginationParams field schema"
    );
    record::<PaginationParams>(
        &oracle,
        "PaginationParams",
        serde_json::to_value(&value).unwrap(),
    );
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        PaginationParams::deserialize(positional).is_err(),
        "PaginationParams public qualified shape gate"
    );
    overflow::<Balance>(&oracle, "Balance", |v, field| {
        assert_eq!(field, "decimals");
        v.decimals
    });
    overflow::<Ticker>(&oracle, "Ticker", |v, field| {
        assert_eq!(field, "timestamp");
        v.timestamp
    });
    overflow::<PaginationParams>(&oracle, "PaginationParams", |v, field| match field {
        "page" => v.page.unwrap(),
        "pageSize" => v.page_size.unwrap(),
        "startTime" => v.start_time.unwrap(),
        "endTime" => v.end_time.unwrap(),
        _ => panic!("unknown numeric field"),
    });
    let value = LighterConfig {
        base_url: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        ws_url: Some("fixture 👩 é \u{feff} \u{85}".to_owned()),
        api_key: Some("fixture 👩 é \u{feff} \u{85}".to_owned()),
        api_secret: Some("fixture 👩 é \u{feff} \u{85}".to_owned()),
        private_key: Some("fixture 👩 é \u{feff} \u{85}".to_owned()),
        chain_id: Some(16777217.25),
        timeout: Some(16777217.25),
        debug: Some(true),
        max_retries: Some(16777217.25),
        retry_delay: Some(16777217.25),
    };
    let _: &String = &value.base_url;
    let _: &Option<String> = &value.ws_url;
    let _: &Option<String> = &value.api_key;
    let _: &Option<String> = &value.api_secret;
    let _: &Option<String> = &value.private_key;
    let _: &Option<f64> = &value.chain_id;
    let _: &Option<f64> = &value.timeout;
    let _: &Option<bool> = &value.debug;
    let _: &Option<f64> = &value.max_retries;
    let _: &Option<f64> = &value.retry_delay;
    assert_eq!(
        json!([{"name": "baseUrl", "type": "string", "optional": false}, {"name": "wsUrl", "type": "string", "optional": true}, {"name": "apiKey", "type": "string", "optional": true}, {"name": "apiSecret", "type": "string", "optional": true}, {"name": "privateKey", "type": "string", "optional": true}, {"name": "chainId", "type": "number", "optional": true}, {"name": "timeout", "type": "number", "optional": true}, {"name": "debug", "type": "boolean", "optional": true}, {"name": "maxRetries", "type": "number", "optional": true}, {"name": "retryDelay", "type": "number", "optional": true}]),
        oracle["schemas"]["LighterConfig"],
        "LighterConfig field schema"
    );
    record::<LighterConfig>(
        &oracle,
        "LighterConfig",
        serde_json::to_value(&value).unwrap(),
    );
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        LighterConfig::deserialize(positional).is_err(),
        "LighterConfig public qualified shape gate"
    );
    overflow::<LighterConfig>(&oracle, "LighterConfig", |v, field| match field {
        "chainId" => v.chain_id.unwrap(),
        "timeout" => v.timeout.unwrap(),
        "maxRetries" => v.max_retries.unwrap(),
        "retryDelay" => v.retry_delay.unwrap(),
        _ => panic!("unknown numeric field"),
    });
    let value = Account {
        address: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        account_index: 16777217.25,
        public_key: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        nonce: 16777217.25,
        balances: vec![Balance {
            symbol: "nested".into(),
            token_address: "nested".into(),
            available: "nested".into(),
            locked: "nested".into(),
            total: "nested".into(),
            decimals: 16777217.25,
        }],
        created_at: 16777217.25,
        updated_at: 16777217.25,
    };
    let _: &String = &value.address;
    let _: &f64 = &value.account_index;
    let _: &String = &value.public_key;
    let _: &f64 = &value.nonce;
    let _: &Vec<Balance> = &value.balances;
    let _: &f64 = &value.created_at;
    let _: &f64 = &value.updated_at;
    assert_eq!(
        json!([{"name": "address", "type": "string", "optional": false}, {"name": "accountIndex", "type": "number", "optional": false}, {"name": "publicKey", "type": "string", "optional": false}, {"name": "nonce", "type": "number", "optional": false}, {"name": "balances", "type": "Balance[]", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "updatedAt", "type": "number", "optional": false}]),
        oracle["schemas"]["Account"],
        "Account field schema"
    );
    record::<Account>(&oracle, "Account", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        Account::deserialize(positional).is_err(),
        "Account public qualified shape gate"
    );
    overflow::<Account>(&oracle, "Account", |v, field| match field {
        "accountIndex" => v.account_index,
        "nonce" => v.nonce,
        "createdAt" => v.created_at,
        "updatedAt" => v.updated_at,
        _ => panic!("unknown numeric field"),
    });
    let value = Market {
        market_id: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        symbol: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        base_token: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quote_token: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        base_token_address: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quote_token_address: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        min_order_size: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        max_order_size: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        price_tick_size: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quantity_tick_size: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        maker_fee: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        taker_fee: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        status: MarketStatus::Active,
        base_decimals: 16777217.25,
        quote_decimals: 16777217.25,
    };
    let _: &String = &value.market_id;
    let _: &String = &value.symbol;
    let _: &String = &value.base_token;
    let _: &String = &value.quote_token;
    let _: &String = &value.base_token_address;
    let _: &String = &value.quote_token_address;
    let _: &String = &value.min_order_size;
    let _: &String = &value.max_order_size;
    let _: &String = &value.price_tick_size;
    let _: &String = &value.quantity_tick_size;
    let _: &String = &value.maker_fee;
    let _: &String = &value.taker_fee;
    let _: &MarketStatus = &value.status;
    let _: &f64 = &value.base_decimals;
    let _: &f64 = &value.quote_decimals;
    assert_eq!(
        json!([{"name": "marketId", "type": "string", "optional": false}, {"name": "symbol", "type": "string", "optional": false}, {"name": "baseToken", "type": "string", "optional": false}, {"name": "quoteToken", "type": "string", "optional": false}, {"name": "baseTokenAddress", "type": "string", "optional": false}, {"name": "quoteTokenAddress", "type": "string", "optional": false}, {"name": "minOrderSize", "type": "string", "optional": false}, {"name": "maxOrderSize", "type": "string", "optional": false}, {"name": "priceTickSize", "type": "string", "optional": false}, {"name": "quantityTickSize", "type": "string", "optional": false}, {"name": "makerFee", "type": "string", "optional": false}, {"name": "takerFee", "type": "string", "optional": false}, {"name": "status", "type": "MarketStatus", "optional": false}, {"name": "baseDecimals", "type": "number", "optional": false}, {"name": "quoteDecimals", "type": "number", "optional": false}]),
        oracle["schemas"]["Market"],
        "Market field schema"
    );
    record::<Market>(&oracle, "Market", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        Market::deserialize(positional).is_err(),
        "Market public qualified shape gate"
    );
    overflow::<Market>(&oracle, "Market", |v, field| match field {
        "baseDecimals" => v.base_decimals,
        "quoteDecimals" => v.quote_decimals,
        _ => panic!("unknown numeric field"),
    });
    let value = Position {
        market_id: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        symbol: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        side: safe_cex_types::PositionSide::Long,
        size: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        entry_price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        mark_price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        liquidation_price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        unrealized_pnl: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        realized_pnl: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        margin: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        leverage: 16777217.25,
        notional: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        created_at: 16777217.25,
        updated_at: 16777217.25,
    };
    let _: &String = &value.market_id;
    let _: &String = &value.symbol;
    let _: &safe_cex_types::PositionSide = &value.side;
    let _: &String = &value.size;
    let _: &String = &value.entry_price;
    let _: &String = &value.mark_price;
    let _: &String = &value.liquidation_price;
    let _: &String = &value.unrealized_pnl;
    let _: &String = &value.realized_pnl;
    let _: &String = &value.margin;
    let _: &f64 = &value.leverage;
    let _: &String = &value.notional;
    let _: &f64 = &value.created_at;
    let _: &f64 = &value.updated_at;
    assert_eq!(
        json!([{"name": "marketId", "type": "string", "optional": false}, {"name": "symbol", "type": "string", "optional": false}, {"name": "side", "type": "\"long\" | \"short\"", "optional": false}, {"name": "size", "type": "string", "optional": false}, {"name": "entryPrice", "type": "string", "optional": false}, {"name": "markPrice", "type": "string", "optional": false}, {"name": "liquidationPrice", "type": "string", "optional": false}, {"name": "unrealizedPnl", "type": "string", "optional": false}, {"name": "realizedPnl", "type": "string", "optional": false}, {"name": "margin", "type": "string", "optional": false}, {"name": "leverage", "type": "number", "optional": false}, {"name": "notional", "type": "string", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "updatedAt", "type": "number", "optional": false}]),
        oracle["schemas"]["Position"],
        "Position field schema"
    );
    record::<Position>(&oracle, "Position", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        Position::deserialize(positional).is_err(),
        "Position public qualified shape gate"
    );
    overflow::<Position>(&oracle, "Position", |v, field| match field {
        "leverage" => v.leverage,
        "createdAt" => v.created_at,
        "updatedAt" => v.updated_at,
        _ => panic!("unknown numeric field"),
    });
    let value = Trade {
        trade_id: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        symbol: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        price: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quantity: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        quote_quantity: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        side: safe_cex_types::OrderSide::Buy,
        timestamp: 16777217.25,
        is_buyer_maker: true,
    };
    let _: &String = &value.trade_id;
    let _: &String = &value.symbol;
    let _: &String = &value.price;
    let _: &String = &value.quantity;
    let _: &String = &value.quote_quantity;
    let _: &safe_cex_types::OrderSide = &value.side;
    let _: &f64 = &value.timestamp;
    let _: &bool = &value.is_buyer_maker;
    assert_eq!(
        json!([{"name": "tradeId", "type": "string", "optional": false}, {"name": "symbol", "type": "string", "optional": false}, {"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "quoteQuantity", "type": "string", "optional": false}, {"name": "side", "type": "\"buy\" | \"sell\"", "optional": false}, {"name": "timestamp", "type": "number", "optional": false}, {"name": "isBuyerMaker", "type": "boolean", "optional": false}]),
        oracle["schemas"]["Trade"],
        "Trade field schema"
    );
    record::<Trade>(&oracle, "Trade", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        Trade::deserialize(positional).is_err(),
        "Trade public qualified shape gate"
    );
    overflow::<Trade>(&oracle, "Trade", |v, field| match field {
        "timestamp" => v.timestamp,
        _ => panic!("unknown numeric field"),
    });
    let value = OrderBook {
        symbol: "fixture 👩 é \u{feff} \u{85}".to_owned(),
        bids: vec![OrderBookLevel {
            price: "nested".into(),
            quantity: "nested".into(),
        }],
        asks: vec![OrderBookLevel {
            price: "nested".into(),
            quantity: "nested".into(),
        }],
        last_update_id: 16777217.25,
        timestamp: 16777217.25,
    };
    let _: &String = &value.symbol;
    let _: &Vec<OrderBookLevel> = &value.bids;
    let _: &Vec<OrderBookLevel> = &value.asks;
    let _: &f64 = &value.last_update_id;
    let _: &f64 = &value.timestamp;
    assert_eq!(
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "bids", "type": "OrderBookLevel[]", "optional": false}, {"name": "asks", "type": "OrderBookLevel[]", "optional": false}, {"name": "lastUpdateId", "type": "number", "optional": false}, {"name": "timestamp", "type": "number", "optional": false}]),
        oracle["schemas"]["OrderBook"],
        "OrderBook field schema"
    );
    record::<OrderBook>(&oracle, "OrderBook", serde_json::to_value(&value).unwrap());
    let positional = Value::Array(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect(),
    );
    assert!(
        OrderBook::deserialize(positional).is_err(),
        "OrderBook public qualified shape gate"
    );
    overflow::<OrderBook>(&oracle, "OrderBook", |v, field| match field {
        "lastUpdateId" => v.last_update_id,
        "timestamp" => v.timestamp,
        _ => panic!("unknown numeric field"),
    });
    let expected = json!(
        oracle["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry[1].clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        json!(support::enum_inventory::<MarketStatus>()),
        expected,
        "MarketStatus accepted metadata"
    );
    support::assert_string_only::<MarketStatus>(&expected, "MarketStatus");
    for value in expected.as_array().unwrap() {
        let parsed = MarketStatus::deserialize(value.clone()).unwrap();
        assert_eq!(json!(parsed), *value, "MarketStatus literal roundtrip");
        assert!(
            MarketStatus::deserialize(json!({value.as_str().unwrap():null})).is_err(),
            "MarketStatus public qualified gate"
        );
    }
    let variants = [
        MarketStatus::Active,
        MarketStatus::Paused,
        MarketStatus::Closed,
        MarketStatus::PreLaunch,
    ];
    let members = variants.map(|value| match value {
        MarketStatus::Active => json!(["Active", value]),
        MarketStatus::Paused => json!(["Paused", value]),
        MarketStatus::Closed => json!(["Closed", value]),
        MarketStatus::PreLaunch => json!(["PreLaunch", value]),
    });
    assert_eq!(
        json!(members),
        oracle["statuses"],
        "MarketStatus exhaustive members"
    );
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_common_source_controls_fail_by_name() {
    for (mutation, witness) in [
        ("inherited-number", "Balance unsupported interface heritage"),
        ("field", "Balance field schema"),
        ("optional", "PaginationParams field schema"),
        ("enum", "Market.status accepted metadata"),
        ("config-optional", "LighterConfig field schema"),
        ("account-array", "Account field schema"),
        ("market-status", "Market field schema"),
        ("position-side", "Position field schema"),
        ("trade-maker", "Trade field schema"),
        ("book-levels", "OrderBook field schema"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_common_types_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_COMMON_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(witness), "{log}");
    }
}
#[test]
#[ignore = "requires Node"]
fn sdk_common_requires_selected_compiler() {
    let output = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/absent-common-compiler"
            ),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
