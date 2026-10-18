//! The vendored TDG schemas used by the venue, in full:lite:child form.
//! Mapping runs over live Js values; arrays and own undefined fields are retained.
use safe_cex_exchange::js_runtime::value::{Js, Thrown, utf16};

fn fields(name: &str) -> &'static str {
    match name {
        "scope" => "sub_account_id:sa kind:k base:b quote:q",
        "cancel" => "sub_account_id:sa order_id:oi client_order_id:co time_to_live_ms:tt",
        "account" => "sub_account_id:sa",
        "set_leverage" => "sub_account_id:sa instrument:i leverage:l",
        "create_request" => "order:o:order",
        "order_response" => "result:r:order",
        "orders_response" => "result:r:order",
        "positions_response" => "result:r:position",
        "leverage_response" => "results:r:leverage",
        "success_response" => "success:s",
        "cancel_all_response" => "result:r",
        "funding_response" => "result:r:funding tier:t:tier",
        "error" => "code:c message:m",
        "order" => {
            "order_id:oi sub_account_id:sa is_market:im time_in_force:ti post_only:po reduce_only:ro legs:l:leg signature:s:signature metadata:m:metadata state:s1:state is_liquidation:il is_derisk:id builder:b builder_fee:bf is_spot_dedust:is"
        }
        "leg" => "instrument:i size:s limit_price:lp is_buying_asset:ib",
        "signature" => "signer:s r:r s:s1 v:v expiration:e nonce:n chain_id:ci",
        "metadata" => {
            "client_order_id:co create_time:ct trigger:t:trigger broker:b source:s is_ecn:ie is_position_transfer:ip allow_crossing:ac"
        }
        "trigger" => "trigger_type:tt tpsl:t:tpsl",
        "tpsl" => "trigger_by:tb trigger_price:tp close_position:cp is_split_position:is",
        "state" => {
            "status:s reject_reason:rr book_size:bs traded_size:ts update_time:ut avg_fill_price:af"
        }
        "position" => {
            "event_time:et sub_account_id:sa instrument:i size:s notional:n entry_price:ep exit_price:ep1 mark_price:mp unrealized_pnl:up realized_pnl:rp total_pnl:tp roi:r quote_index_price:qi est_liquidation_price:el leverage:l cumulative_fee:cf cumulative_realized_funding_payment:cr margin_type:mt isolated_balance:ib isolated_im:ii isolated_mm:im"
        }
        "leverage" => "instrument:i leverage:l min_leverage:ml max_leverage:ml1 margin_type:mt",
        "funding" => {
            "main_account_id:ma total_equity:te spot_balances:sb:spot vault_investments:vi:vault total_cash_balance:tc total_spot_asset_balance:ts"
        }
        "spot" => {
            "currency:c balance:b index_price:ip entry_price:ep realized_pnl:rp unrealized_pnl:up available_to_transfer:at"
        }
        "vault" => "vault_id:vi num_lp_tokens:nl share_price:sp usd_notional_invested:un",
        "tier" => {
            "tier:t futures_taker_fee:ft futures_maker_fee:fm options_taker_fee:ot options_maker_fee:om spot_taker_fee:st spot_maker_fee:sm"
        }
        _ => "",
    }
}

fn cover_bigint(field: &str, value: Js) -> Js {
    let Js::BigInt(number) = &value else {
        return value;
    };
    let hex = [
        "ask_depth_scores",
        "bid_depth_scores",
        "brokered_trading_volume",
        "builder",
        "builder_account_id",
        "builder_api_key_signer",
        "builder_main_account_id",
        "direct_invite_trading_volume",
        "from_account_id",
        "from_address",
        "group_id",
        "indirect_invite_trading_volume",
        "instrument_hash",
        "l_1_hash",
        "l_2_hash",
        "liquidity_score",
        "lp_address",
        "main_account_i_ds",
        "main_account_id",
        "maker_trading_volume",
        "order_i_ds",
        "order_id",
        "r",
        "referrer_main_account_id",
        "s",
        "signer",
        "to_account_id",
        "to_eth_address",
        "total_trade_volume",
        "tx_hash",
        "vault_liquidity_score",
    ]
    .contains(&field);
    if hex {
        let digits = number.to_str_radix(16);
        Js::str(&format!(
            "0x{}{}",
            if digits.len() % 2 == 0 { "" } else { "0" },
            digits
        ))
    } else {
        Js::str(&number.to_string())
    }
}

pub(super) fn map(input: &Js, schema: &str, to_lite: bool) -> Result<Js, Thrown> {
    if input.is_nullish() || !input.is_object() || input.is_callable() {
        return Ok(input.clone());
    }
    if let Some(items) = input.array_values()? {
        return Ok(Js::array(
            items
                .iter()
                .map(|item| map(item, schema, to_lite))
                .collect::<Result<_, _>>()?,
        ));
    }
    let result = Js::object(vec![]);
    for (key, mut field) in input.own_entries()? {
        let mut target = "undefined";
        let mut child = "";
        for entry in fields(schema).split_whitespace() {
            let mut names = entry.split(':');
            let full = names.next().unwrap_or("");
            let lite = names.next().unwrap_or("");
            if key.as_ref() == utf16(if to_lite { full } else { lite }).as_ref() {
                target = if to_lite { lite } else { full };
                child = names.next().unwrap_or("");
                break;
            }
        }
        if to_lite {
            field = cover_bigint(&String::from_utf16_lossy(&key), field);
        }
        // The source assigns result[_key], including an unknown schema key's
        // literal "undefined" property. Do not silently preserve unknown keys.
        result.set(target, map(&field, child, to_lite)?)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn decoded(value: &Value) -> Js {
        match value {
            Value::Object(object) if object.contains_key("$bigint") => Js::BigInt(
                std::rc::Rc::new(object["$bigint"].as_str().unwrap().parse().unwrap()),
            ),
            Value::Object(object) => Js::object(
                object
                    .iter()
                    .map(|(key, value)| (key.as_str(), decoded(value)))
                    .collect(),
            ),
            Value::Array(values) => Js::array(values.iter().map(decoded).collect()),
            _ => safe_cex_exchange::js_runtime::value::from_json(value),
        }
    }
    fn compare(actual: &Js, expected: &Value, label: &str) {
        match expected {
            Value::Object(fields) => {
                let keys: Vec<_> = actual
                    .own_keys()
                    .iter()
                    .map(|key| String::from_utf16_lossy(key))
                    .collect();
                assert_eq!(
                    keys,
                    fields.keys().cloned().collect::<Vec<_>>(),
                    "{label} own key order"
                );
                for (key, value) in fields {
                    compare(&actual.get(key).unwrap(), value, &format!("{label}.{key}"));
                }
            }
            Value::Array(values) => {
                let actual = actual.array_values()?.expect("array");
                assert_eq!(actual.len(), values.len(), "{label} array count");
                for (index, (actual, value)) in actual.iter().zip(values).enumerate() {
                    compare(actual, value, &format!("{label}[{index}]"));
                }
            }
            _ => assert!(
                actual.strict_equals(&decoded(expected)),
                "{label} primitive identity: {actual:?} vs {expected}"
            ),
        }
    }
    #[test]
    #[ignore = "requires Node and maintained TypeScript"]
    fn tdg_schema_mapping_matches_actual_sdk_source() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/binding-schema-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rows.len(), 53, "independent source-map case floor");
        for (index, row) in rows.iter().enumerate() {
            let actual = map(
                &decoded(&row["input"]),
                row["schema"].as_str().unwrap(),
                row["lite"].as_bool().unwrap(),
            )
            .unwrap();
            compare(
                &actual,
                &row["output"],
                &format!("case {index} {}", row["schema"]),
            );
        }
    }
}
