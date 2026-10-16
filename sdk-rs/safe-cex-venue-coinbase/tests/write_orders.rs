use safe_cex_venue_coinbase::starbase_orders::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/schema.rs"]
mod schema;
fn sort_schema_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(Value::Array(fields)) = object.get_mut("fields") {
                for field in fields.iter_mut() {
                    sort_schema_fields(field);
                }
                fields.sort_by_key(|field| field["name"].as_str().unwrap().to_owned());
            }
            for child in object.values_mut() {
                sort_schema_fields(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sort_schema_fields),
        _ => {}
    }
}
fn sort_value_keys(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = std::mem::take(object).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, child) in &mut entries {
                sort_value_keys(child);
            }
            *object = entries.into_iter().collect();
        }
        Value::Array(items) => items.iter_mut().for_each(sort_value_keys),
        _ => {}
    }
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/write-orders.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
#[test]
fn write_protocol_union_domains_and_values() {
    let source = include_str!("../src/starbase_orders.rs");
    let parsed = syn::parse_file(source).unwrap();
    let names: std::collections::BTreeSet<_> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Enum(e)
                if e.variants
                    .iter()
                    .all(|v| matches!(v.fields, syn::Fields::Unit)) =>
            {
                Some(e.ident.to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        oracle()["schemas"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    );
    let actual = json!({
        "CoinbaseStarbasePlaceOrderMethod":[CoinbaseStarbasePlaceOrderMethod::Buy,CoinbaseStarbasePlaceOrderMethod::Sell],
        "CoinbaseStarbaseEditMethod":[CoinbaseStarbaseEditMethod::Edit],
        "CoinbaseStarbaseOtocoLegType":[CoinbaseStarbaseOtocoLegType::TakeLimit,CoinbaseStarbaseOtocoLegType::TakeMarket,CoinbaseStarbaseOtocoLegType::StopLimit,CoinbaseStarbaseOtocoLegType::StopMarket],
        "CoinbaseStarbaseClosePositionType":[CoinbaseStarbaseClosePositionType::Limit,CoinbaseStarbaseClosePositionType::Market],
        "CoinbaseStarbaseBaseAmountSource":[CoinbaseStarbaseBaseAmountSource::TechnicalMigrationGuide],
        "CoinbaseStarbaseBaseAmountUnit":[CoinbaseStarbaseBaseAmountUnit::Base],
        "CoinbaseStarbaseContractAmountSource":[CoinbaseStarbaseContractAmountSource::GatewayContractCount],
        "CoinbaseStarbaseContractAmountUnit":[CoinbaseStarbaseContractAmountUnit::Contracts],
        "CoinbaseStarbaseUnresolvedTimestampUnit":[CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified],
        "CoinbaseStarbaseWriteOrderType":[CoinbaseStarbaseWriteOrderType::Limit,CoinbaseStarbaseWriteOrderType::StopLimit,CoinbaseStarbaseWriteOrderType::TakeLimit,CoinbaseStarbaseWriteOrderType::Market,CoinbaseStarbaseWriteOrderType::StopMarket,CoinbaseStarbaseWriteOrderType::TakeMarket,CoinbaseStarbaseWriteOrderType::MarketLimit,CoinbaseStarbaseWriteOrderType::TrailingStop],
        "CoinbaseStarbaseLinkedOrderType":[CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther],
        "CoinbaseStarbaseTriggerFillCondition":[CoinbaseStarbaseTriggerFillCondition::FirstHit,CoinbaseStarbaseTriggerFillCondition::CompleteFill,CoinbaseStarbaseTriggerFillCondition::Incremental],
        "CoinbaseStarbaseWriteSide":[CoinbaseStarbaseWriteSide::Buy,CoinbaseStarbaseWriteSide::Sell],
        "CoinbaseStarbaseWriteParityBlocker":[CoinbaseStarbaseWriteParityBlocker::ContractUnitConversionUnverified,CoinbaseStarbaseWriteParityBlocker::EditTotalVsRemainingAmountUnresolved,CoinbaseStarbaseWriteParityBlocker::MarketLimitResponseSchemaMissing,CoinbaseStarbaseWriteParityBlocker::ResponseFilledAmountUnitConflict,CoinbaseStarbaseWriteParityBlocker::ResponseTradeAmountUnitConflict,CoinbaseStarbaseWriteParityBlocker::ValidUntilUnitUnpublished],
    });
    assert_eq!(actual.as_object().unwrap().len(), names.len());
    for (name, expected) in oracle()["schemas"].as_object().unwrap() {
        assert_eq!(schema::wire_schema(source, name), *expected, "{name}");
        assert_eq!(actual[name], expected["values"], "{name}");
    }
}

fn base_amount() -> CoinbaseStarbaseBaseAmountIntent {
    CoinbaseStarbaseBaseAmountIntent {
        total_base_amount: 12.25,
        source: CoinbaseStarbaseBaseAmountSource::TechnicalMigrationGuide,
        unit: CoinbaseStarbaseBaseAmountUnit::Base,
    }
}
fn contracts() -> CoinbaseStarbaseContractAmountIntent {
    CoinbaseStarbaseContractAmountIntent {
        total_contracts: 12.25,
        source: CoinbaseStarbaseContractAmountSource::GatewayContractCount,
        unit: CoinbaseStarbaseContractAmountUnit::Contracts,
    }
}
#[test]
fn write_intent_records_and_gate_branches() {
    let sources = [
        include_str!("../src/starbase_orders.rs"),
        include_str!("../src/starbase_private.rs"),
        include_str!("../../safe-cex-types/src/lib.rs"),
    ];
    for (name, expected) in oracle()["recordSchemas"].as_object().unwrap() {
        let mut actual = schema::wire_schema_from_sources(&sources, name);
        let mut expected = expected.clone();
        if name == "CoinbaseStarbaseWriteResult"
            || name == "CoinbaseStarbaseEditParams"
            || name == "CoinbaseStarbasePlaceOrderParams"
            || name == "CoinbaseStarbaseOtocoParams"
        {
            // Structural TS declaration order differs from factory insertion order.
            // write_results.rs and write_edit.rs pin actual runtime insertion order.
            for schema in [&mut actual, &mut expected] {
                schema["fields"]
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|f| f["name"].as_str().unwrap().to_owned());
            }
        }
        if name == "CoinbaseStarbasePlaceOrderParams"
            || name == "CoinbaseStarbaseOtocoParams"
            || name == "CoinbaseStarbasePlaceOrderRequest"
        {
            sort_schema_fields(&mut actual);
            sort_schema_fields(&mut expected);
        }
        if name == "CoinbaseStarbaseEditRequest" || name == "CoinbaseStarbasePlaceOrderRequest" {
            for schema in [&mut actual, &mut expected] {
                let params = schema["fields"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|field| field["name"] == "params")
                    .unwrap();
                params["schema"]["fields"]
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|field| field["name"].as_str().unwrap().to_owned());
            }
        }
        assert_eq!(actual, expected, "{name}");
    }
    for case in oracle()["recordCases"].as_array().unwrap() {
        let mask = case["mask"].as_u64().unwrap();
        let present = |bit: u32| mask & (1u64 << bit) != 0u64;
        let choice = case["choice"].as_u64().unwrap();
        let actual=match case["name"].as_str().unwrap() {
            "CoinbaseStarbasePlaceOrderRequest"=>json!(CoinbaseStarbasePlaceOrderRequest {method:if choice==0 {CoinbaseStarbasePlaceOrderMethod::Buy}else{CoinbaseStarbasePlaceOrderMethod::Sell},params:place_params(0)}),
            "CoinbaseStarbaseEditRequest"=>json!(CoinbaseStarbaseEditRequest {method:CoinbaseStarbaseEditMethod::Edit,params:CoinbaseStarbaseEditParams {order_id:"sample".into(),post_only:None,price:None,reduce_only:None,reject_post_only:None,trigger_offset:None,trigger_price:None}}),
            "CoinbaseStarbasePlaceOrderIntent"=>json!(place_intent(mask,choice)),
            "CoinbaseStarbaseSideOrderIntent"=>json!(side_intent(mask,choice)),
            "CoinbaseStarbasePlaceOrderParams"=>json!(place_params(mask)),
            "CoinbaseStarbaseOtocoParams"=>json!(otoco_params(mask)),
            "CoinbaseStarbaseWriteResult"=>json!(CoinbaseStarbaseWriteResult {identity:CoinbaseStarbaseOrderIdentity {canonical_order_id:"sample".into(),non_canonical_label:"sample".into(),pre_trigger_order_id:None,otoco_child_order_ids:vec!["sample".into(),"sample".into()],otoco_primary_order_id:None},order:{let mut order=write_result_order();if choice==2 {order.price=safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseOrderPrice::Market(safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseMarketPrice::MarketPrice);}order},trades:(0..choice).map(|index|{let mut trade=write_result_trade();trade.id=format!("trade-{index}");trade}).collect()}),
            "CoinbaseStarbaseOrderIdentity"=>json!(CoinbaseStarbaseOrderIdentity {canonical_order_id:"canonical".into(),non_canonical_label:"noncanonical".into(),pre_trigger_order_id:present(0).then(||"sample".into()),otoco_child_order_ids:match choice {0=>vec![],1=>vec!["first".into()],2=>vec!["second".into(),"first".into(),"second".into()],_=>panic!("unclassified identity array")},otoco_primary_order_id:present(1).then(||"sample".into())}),
            "CoinbaseStarbaseOtocoLegIntent"=>json!(otoco_leg(mask)),
            "CoinbaseStarbaseOtocoIntent"=>json!(CoinbaseStarbaseOtocoIntent {legs:[otoco_leg(0),otoco_leg(0)],linked_order_type:CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther,trigger_fill_condition:present(0).then_some(CoinbaseStarbaseTriggerFillCondition::FirstHit)}),
            "CoinbaseStarbaseEditIntent"=>json!(CoinbaseStarbaseEditIntent {
                order_id:"sample".into(), price:present(0).then_some(12.25),
                post_only:present(1).then_some(false), reduce_only:present(2).then_some(false),
                reject_post_only:present(3).then_some(false),
                size:present(4).then(||if choice==0 {CoinbaseStarbaseWriteAmountIntent::Base(base_amount())}else{CoinbaseStarbaseWriteAmountIntent::Contracts(contracts())}),
                trigger_offset:present(5).then_some(12.25),trigger_price:present(6).then_some(12.25),
                valid_until:present(7).then_some(CoinbaseStarbaseUnresolvedValidUntilIntent {gateway_integer_timestamp:12.25,unit:CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified}),
            }),
            "CoinbaseStarbaseEditParams"=>json!(CoinbaseStarbaseEditParams {
                order_id:"sample".into(),post_only:present(0).then_some(false),price:present(1).then_some(12.25),
                reduce_only:present(2).then_some(false),reject_post_only:present(3).then_some(false),
                trigger_offset:present(4).then_some(12.25),trigger_price:present(5).then_some(12.25),
            }),
            "CoinbaseStarbaseClosePositionIntent"=>json!(CoinbaseStarbaseClosePositionIntent {
                price:present(0).then_some(12.25),saved_symbol:"sample".into(),
                order_type:if choice==0 {CoinbaseStarbaseClosePositionType::Limit}else{CoinbaseStarbaseClosePositionType::Market},
            }),
            "CoinbaseStarbaseBaseAmountIntent"=>json!(base_amount()),
            "CoinbaseStarbaseContractAmountIntent"=>json!(contracts()),
            "CoinbaseStarbaseWriteAmountIntent"=>match case["choice"].as_u64().unwrap() {
                0=>json!(CoinbaseStarbaseWriteAmountIntent::Base(base_amount())),
                1=>json!(CoinbaseStarbaseWriteAmountIntent::Contracts(contracts())),
                other=>panic!("unclassified amount branch {other}"),
            },
            "CoinbaseStarbaseWriteGate"=>{
                let gate:CoinbaseStarbaseWriteGate<Value,Value>=match case["choice"].as_u64().unwrap() {
                    0=>CoinbaseStarbaseWriteGate::Ready {value:json!({"marker":"T"})},
                    1=>CoinbaseStarbaseWriteGate::Blocked {blocker:CoinbaseStarbaseWriteParityBlocker::ContractUnitConversionUnverified,intent:json!({"marker":"Intent"})},
                    other=>panic!("unclassified write gate branch {other}"),
                };json!(gate)
            },
            "CoinbaseStarbaseUnresolvedValidUntilIntent"=>json!(CoinbaseStarbaseUnresolvedValidUntilIntent {gateway_integer_timestamp:12.25,unit:CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified}),
            "CoinbaseStarbaseTifParams"=>json!(CoinbaseStarbaseTifParams {post_only:false,time_in_force:safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTimeInForce::GoodTilCancelled}),
            other=>panic!("unclassified record {other}"),
        };
        if matches!(
            case["name"].as_str().unwrap(),
            "CoinbaseStarbasePlaceOrderParams"
                | "CoinbaseStarbaseOtocoParams"
                | "CoinbaseStarbasePlaceOrderRequest"
        ) {
            let mut actual = actual;
            let mut expected = case["value"].clone();
            sort_value_keys(&mut actual);
            sort_value_keys(&mut expected);
            assert_eq!(actual, expected, "{}", case["name"]);
            continue;
        }
        assert_eq!(actual, case["value"], "{}", case["name"]);
        if case["name"] == "CoinbaseStarbaseWriteResult"
            || case["name"] == "CoinbaseStarbaseEditParams"
        {
            continue;
        }
        assert_eq!(
            actual.as_object().unwrap().keys().collect::<Vec<_>>(),
            case["value"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
    }
    use safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTimeInForce as Tif;
    for case in oracle()["tifCases"].as_array().unwrap() {
        let tif = [
            Tif::GoodTilCancelled,
            Tif::GoodTilDay,
            Tif::FillOrKill,
            Tif::ImmediateOrCancel,
        ]
        .into_iter()
        .find(|tif| json!(tif) == case["time_in_force"])
        .unwrap();
        assert_eq!(
            json!(CoinbaseStarbaseTifParams {
                post_only: case["post_only"].as_bool().unwrap(),
                time_in_force: tif
            }),
            *case
        );
    }
}

fn otoco_leg(mask: u64) -> CoinbaseStarbaseOtocoLegIntent {
    let present = |bit: u32| mask & (1u64 << bit) != 0u64;
    CoinbaseStarbaseOtocoLegIntent {
        direction: CoinbaseStarbaseWriteSide::Buy,
        label: present(0).then(|| "sample".into()),
        post_only: present(1).then_some(false),
        price: present(2).then_some(12.25),
        reduce_only: safe_cex_types::True,
        reject_post_only: present(3).then_some(false),
        size: base_amount(),
        time_in_force: present(4).then_some(safe_cex_types::OrderTimeInForce::GoodTillCancel),
        trigger: safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType::IndexPrice,
        trigger_price: present(5).then_some(12.25),
        order_type: CoinbaseStarbaseOtocoLegType::TakeLimit,
    }
}

#[test]
fn write_otoco_nested_enum_values_and_leg_order() {
    use safe_cex_types::OrderTimeInForce as Tif;
    use safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType as Trigger;
    for case in oracle()["otocoCases"].as_array().unwrap() {
        let mut first = otoco_leg(63);
        let mut second = otoco_leg(0);
        first.label = Some("first".into());
        second.label = Some("second".into());
        first.direction = if case["side"] == "buy" {
            CoinbaseStarbaseWriteSide::Buy
        } else {
            CoinbaseStarbaseWriteSide::Sell
        };
        first.time_in_force = Some(
            [
                Tif::GoodTillCancel,
                Tif::ImmediateOrCancel,
                Tif::FillOrKill,
                Tif::PostOnly,
            ]
            .into_iter()
            .find(|v| json!(v) == case["tif"])
            .unwrap(),
        );
        first.trigger = [Trigger::IndexPrice, Trigger::MarkPrice, Trigger::LastPrice]
            .into_iter()
            .find(|v| json!(v) == case["trigger"])
            .unwrap();
        first.order_type = [
            CoinbaseStarbaseOtocoLegType::TakeLimit,
            CoinbaseStarbaseOtocoLegType::TakeMarket,
            CoinbaseStarbaseOtocoLegType::StopLimit,
            CoinbaseStarbaseOtocoLegType::StopMarket,
        ]
        .into_iter()
        .find(|v| json!(v) == case["type"])
        .unwrap();
        let condition = [
            CoinbaseStarbaseTriggerFillCondition::FirstHit,
            CoinbaseStarbaseTriggerFillCondition::CompleteFill,
            CoinbaseStarbaseTriggerFillCondition::Incremental,
        ]
        .into_iter()
        .find(|v| json!(v) == case["condition"])
        .unwrap();
        assert_eq!(
            json!(CoinbaseStarbaseOtocoIntent {
                legs: [first, second],
                linked_order_type: CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther,
                trigger_fill_condition: Some(condition)
            }),
            case["value"]
        );
    }
}

fn write_result_order() -> safe_cex_venue_coinbase::starbase_private::CoinbaseStarbasePrivateOrder {
    use safe_cex_venue_coinbase::starbase_private::*;
    CoinbaseStarbasePrivateOrder {
        id: "sample".into(),
        instrument_name: "sample".into(),
        legacy_saved_symbol: "sample".into(),
        state: CoinbaseStarbaseOrderState::Open,
        r#type: CoinbaseStarbaseOrderType::Market,
        time_in_force: CoinbaseStarbaseTimeInForce::GoodTilCancelled,
        direction: CoinbaseStarbaseOrderDirection::Buy,
        price: CoinbaseStarbaseOrderPrice::Number(12.25),
        average_price: None,
        label: "sample".into(),
        post_only: false,
        reduce_only: None,
        creation_timestamp_ms: 12.25,
        last_update_timestamp_ms: 12.25,
        gateway_amount: None,
        gateway_filled_amount: None,
        gateway_contracts: None,
        trigger: None,
        trigger_price: None,
        trigger_order_id: None,
    }
}
fn write_result_trade() -> safe_cex_venue_coinbase::starbase_private::CoinbaseStarbasePrivateTrade {
    use safe_cex_venue_coinbase::starbase_private::*;
    CoinbaseStarbasePrivateTrade {
        id: "sample".into(),
        sequence: 12.25,
        order_id: "sample".into(),
        instrument_name: "sample".into(),
        legacy_saved_symbol: "sample".into(),
        timestamp_ms: 12.25,
        direction: CoinbaseStarbaseOrderDirection::Buy,
        r#type: CoinbaseStarbaseTradeType::Limit,
        state: CoinbaseStarbaseTradeState::Open,
        price_usd: 12.25,
        mark_price_usd: 12.25,
        index_price_usd: 12.25,
        gateway_amount: 12.25,
        gateway_contracts: None,
        gateway_profit_loss: None,
        fee: 12.25,
        fee_currency: CoinbaseStarbaseFeeCurrency::Btc,
        liquidity: None,
        label: None,
    }
}

fn place_intent(mask: u64, choice: u64) -> CoinbaseStarbasePlaceOrderIntent {
    let present = |bit: u32| mask & (1u64 << bit) != 0u64;
    CoinbaseStarbasePlaceOrderIntent {
        label: present(0).then(|| "sample".into()),
        otoco: present(1).then(|| CoinbaseStarbaseOtocoIntent {
            legs: [otoco_leg(0), otoco_leg(0)],
            linked_order_type: CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther,
            trigger_fill_condition: None,
        }),
        post_only: present(2).then_some(false),
        price: present(3).then_some(12.25),
        reduce_only: present(4).then_some(false),
        reject_post_only: present(5).then_some(false),
        saved_symbol: "sample".into(),
        side: CoinbaseStarbaseWriteSide::Buy,
        size: if choice == 0 {
            CoinbaseStarbaseWriteAmountIntent::Base(base_amount())
        } else {
            CoinbaseStarbaseWriteAmountIntent::Contracts(contracts())
        },
        time_in_force: present(6).then_some(safe_cex_types::OrderTimeInForce::GoodTillCancel),
        trigger: present(7).then_some(
            safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType::IndexPrice,
        ),
        trigger_offset: present(8).then_some(12.25),
        trigger_price: present(9).then_some(12.25),
        r#type: CoinbaseStarbaseWriteOrderType::Limit,
        valid_until: present(10).then_some(CoinbaseStarbaseUnresolvedValidUntilIntent {
            gateway_integer_timestamp: 12.25,
            unit: CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified,
        }),
    }
}
fn side_intent(mask: u64, choice: u64) -> CoinbaseStarbaseSideOrderIntent {
    let p = place_intent(mask, choice);
    CoinbaseStarbaseSideOrderIntent {
        label: p.label,
        otoco: p.otoco,
        post_only: p.post_only,
        price: p.price,
        reduce_only: p.reduce_only,
        reject_post_only: p.reject_post_only,
        saved_symbol: p.saved_symbol,
        size: p.size,
        time_in_force: p.time_in_force,
        trigger: p.trigger,
        trigger_offset: p.trigger_offset,
        trigger_price: p.trigger_price,
        r#type: p.r#type,
        valid_until: p.valid_until,
    }
}
fn otoco_params(mask: u64) -> CoinbaseStarbaseOtocoParams {
    let present = |bit: u32| mask & (1u64 << bit) != 0u64;
    CoinbaseStarbaseOtocoParams {
        amount:12.25,direction:CoinbaseStarbaseWriteSide::Buy,label:present(0).then(||"sample".into()),post_only:present(1).then_some(false),price:present(2).then_some(12.25),
        reduce_only:safe_cex_types::True,reject_post_only:present(3).then_some(false),time_in_force:present(4).then_some(safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTimeInForce::GoodTilCancelled),
        trigger:safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType::IndexPrice,trigger_price:present(5).then_some(12.25),r#type:CoinbaseStarbaseOtocoLegType::TakeLimit,
    }
}
fn place_params(mask: u64) -> CoinbaseStarbasePlaceOrderParams {
    let present = |bit: u32| mask & (1u64 << bit) != 0u64;
    CoinbaseStarbasePlaceOrderParams {
        amount:12.25,instrument_name:"sample".into(),label:present(0).then(||"sample".into()),linked_order_type:present(1).then_some(CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther),
        otoco_config:present(2).then(||[otoco_params(0),otoco_params(0)]),post_only:present(3).then_some(false),price:present(4).then_some(12.25),reduce_only:present(5).then_some(false),reject_post_only:present(6).then_some(false),
        time_in_force:present(7).then_some(safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTimeInForce::GoodTilCancelled),trigger:present(8).then_some(safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType::IndexPrice),
        trigger_fill_condition:present(9).then_some(CoinbaseStarbaseTriggerFillCondition::FirstHit),trigger_offset:present(10).then_some(12.25),trigger_price:present(11).then_some(12.25),r#type:CoinbaseStarbaseWriteOrderType::Limit,
    }
}
