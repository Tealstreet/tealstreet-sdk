//! This corpus was captured from the TS adapter and real Go-WASM before this port.
use safe_cex_crypto::lighter::SchnorrNonce;
use safe_cex_venue_lighter::sign::{Approval, Builder, Order, Signer, Transaction};
use serde_json::Value;

const CORPUS: &str =
    include_str!("../../../crypto/tests/fixtures/lighter/venue-signing-preimages.json");

fn integer(value: &Value) -> u64 {
    value
        .as_u64()
        .unwrap_or_else(|| value["$bigint"].as_str().unwrap().parse().unwrap())
}

fn optional_integer(value: &Value) -> Option<u64> {
    if value.is_null() || value["$undefined"] == true {
        None
    } else {
        Some(integer(value))
    }
}

fn builder(value: &Value) -> Option<Builder> {
    Some(Builder {
        account_index: optional_integer(&value["integratorAccountIndex"])?,
        maker_fee: optional_integer(&value["integratorMakerFee"])? as u32,
        taker_fee: optional_integer(&value["integratorTakerFee"])? as u32,
    })
}

fn order(value: &Value) -> Order {
    let order_type = match value["orderType"].as_str().unwrap_or("limit") {
        "limit" => 0,
        "market" => 1,
        "stop_loss" => 2,
        "stop_loss_limit" => 3,
        "take_profit" => 4,
        "take_profit_limit" => 5,
        other => panic!("uncaptured order type {other}"),
    };
    let time_in_force = if order_type == 1 {
        0
    } else if value["postOnly"] == true || value["timeInForce"] == "PostOnly" {
        2
    } else if matches!(
        value["timeInForce"].as_str(),
        Some("ImmediateOrCancel" | "FillOrKill")
    ) {
        0
    } else {
        1
    };
    Order {
        order_id: integer(&value["orderId"]),
        market_id: integer(&value["marketId"]) as u32,
        is_buy: value["isBuy"].as_bool().unwrap(),
        price: integer(&value["price"]) as u32,
        amount: integer(&value["amount"]),
        order_type,
        time_in_force,
        reduce_only: value["reduceOnly"] == true,
        trigger_price: optional_integer(&value["triggerPrice"]).unwrap_or(0) as u32,
        order_expiry: optional_integer(&value["orderExpiry"]),
    }
}

fn transaction(method: &str, value: &Value) -> Transaction {
    match method {
        "getCreateOrderTransaction" => Transaction::Create {
            order: order(value),
            builder: builder(value),
        },
        "getCreateGroupedOrdersTransaction" => Transaction::Grouped {
            grouping_type: integer(&value["groupingType"]) as u8,
            orders: value["orders"]
                .as_array()
                .unwrap()
                .iter()
                .map(order)
                .collect(),
            builder: builder(value),
        },
        "getCancelOrderTransaction" => Transaction::Cancel {
            market_id: integer(&value["marketId"]) as u32,
            order_id: integer(&value["orderId"]),
        },
        "getUpdateLeverageTransaction" => Transaction::Leverage {
            market_id: integer(&value["marketId"]) as u32,
            leverage: value["leverage"].as_f64().unwrap(),
            margin_mode: integer(&value["marginMode"]) as u8,
        },
        "getApproveIntegratorTransaction" => Transaction::ApproveIntegrator(Approval {
            account_index: integer(&value["integratorAccountIndex"]),
            max_perps_taker_fee: integer(&value["maxPerpsTakerFee"]) as u32,
            max_perps_maker_fee: integer(&value["maxPerpsMakerFee"]) as u32,
            max_spot_taker_fee: integer(&value["maxSpotTakerFee"]) as u32,
            max_spot_maker_fee: integer(&value["maxSpotMakerFee"]) as u32,
            expiry: integer(&value["approvalExpiry"]),
            l1_signature: value["l1Signature"].as_str().map(str::to_owned),
        }),
        other => panic!("uncaptured transaction method {other}"),
    }
}

fn decode_form(body: &str, field: &str) -> String {
    let encoded = body
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{field}=")))
        .unwrap();
    let mut decoded = Vec::new();
    let mut bytes = encoded.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            b'%' => {
                let hex = [bytes.next().unwrap(), bytes.next().unwrap()];
                decoded.push(u8::from_str_radix(std::str::from_utf8(&hex).unwrap(), 16).unwrap());
            }
            b'+' => decoded.push(b' '),
            other => decoded.push(other),
        }
    }
    String::from_utf8(decoded).unwrap()
}

fn fixed_nonce() -> SchnorrNonce {
    // Fixed public offline vector entropy; production uses fresh rejection-sampled CSPRNG.
    SchnorrNonce::from_bytes(&[66; 40]).unwrap()
}

#[test]
fn independently_captured_ts_transactions_match_preimages_signatures_and_wire() {
    let corpus: Value = serde_json::from_str(CORPUS).unwrap();
    let now = corpus["fixedNow"].as_u64().unwrap();
    let secret = corpus["secret"].as_str().unwrap();
    let mut transactions = 0;
    let mut authentications = 0;
    let mut batches = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let config = &case["outer"]["adapterConfig"];
        let chain_id = if name.starts_with("lighter-robinhood/livenet/") {
            466_324
        } else if config["testnet"] == true {
            300
        } else {
            304
        };
        let signer = Signer::new(
            secret,
            chain_id,
            integer(&config["accountIndex"]),
            integer(&config["apiKeyIndex"]) as u8,
        )
        .unwrap();
        let mut actual_wire = Vec::new();
        let mut actual_types = Vec::new();
        for (call, result) in case["sdkCalls"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["sdkResults"].as_array().unwrap())
        {
            let method = call["method"].as_str().unwrap();
            if method == "getAuthToken" {
                let token = signer
                    .auth_token_with_nonce(integer(&call["args"][0]), fixed_nonce())
                    .unwrap();
                assert_eq!(
                    token,
                    result["result"].as_str().unwrap(),
                    "{name} authentication"
                );
                authentications += 1;
                continue;
            }
            let (method, requests, results): (&str, Vec<&Value>, Vec<&Value>) =
                if method == "getCreateOrderTransactions" {
                    batches += 1;
                    (
                        "getCreateOrderTransaction",
                        call["args"][0].as_array().unwrap().iter().collect(),
                        result["result"].as_array().unwrap().iter().collect(),
                    )
                } else {
                    (method, vec![&call["args"][0]], vec![&result["result"]])
                };
            assert_eq!(requests.len(), results.len());
            for (request, expected) in requests.into_iter().zip(results) {
                let signed = signer
                    .sign_with_nonce(
                        &transaction(method, request),
                        integer(&expected["nonce"]),
                        now,
                        fixed_nonce(),
                    )
                    .unwrap();
                assert_eq!(
                    safe_cex_crypto::hex(&signed.data),
                    expected["data"].as_str().unwrap(),
                    "{name} raw transaction fields"
                );
                assert_eq!(
                    signed.signature,
                    expected["signature"].as_str().unwrap(),
                    "{name} Schnorr signature"
                );
                actual_types.push(signed.tx_type);
                actual_wire.push(signed.tx_info);
                transactions += 1;
            }
        }
        if actual_wire.is_empty() {
            continue;
        }
        let wire = &case["wire"][0];
        let body = wire["body"].as_str().unwrap();
        if wire["url"] == "/api/v1/sendTxBatch" {
            let expected: Vec<String> =
                serde_json::from_str(&decode_form(body, "tx_infos")).unwrap();
            let expected_types: Vec<u8> =
                serde_json::from_str(&decode_form(body, "tx_types")).unwrap();
            assert_eq!(actual_wire, expected, "{name} batch wire JSON");
            assert_eq!(
                actual_types, expected_types,
                "{name} batch transaction types"
            );
        } else {
            assert_eq!(
                actual_wire,
                vec![decode_form(body, "tx_info")],
                "{name} wire JSON"
            );
            assert_eq!(
                actual_types[0].to_string(),
                decode_form(body, "tx_type"),
                "{name} transaction type"
            );
        }
    }
    assert_eq!((transactions, authentications, batches), (88, 4, 4));
}

fn simple_order() -> Order {
    Order {
        order_id: 1,
        market_id: 0,
        is_buy: true,
        price: 1_000,
        amount: 100,
        order_type: 0,
        time_in_force: 1,
        reduce_only: false,
        trigger_price: 0,
        order_expiry: None,
    }
}

fn signer() -> Signer {
    Signer::new(&format!("01{}", "00".repeat(39)), 304, 42, 3).unwrap()
}

#[test]
fn malformed_signer_inputs_and_transaction_bounds_fail_closed() {
    for key in [
        String::new(),
        "00".repeat(40),
        "ff".repeat(39),
        "xy".repeat(40),
    ] {
        assert!(Signer::new(&key, 304, 42, 3).is_err());
    }
    let key = format!("01{}", "00".repeat(39));
    assert!(Signer::new(&key, 304, 1 << 53, 3).is_err());
    assert!(Signer::new(&key, 304, 42, 255).is_err());
    let signer = signer();
    for (nonce, now) in [(1 << 53, 1_700_000_000_000), (7, u64::MAX)] {
        assert!(
            signer
                .sign_with_nonce(
                    &Transaction::Create {
                        order: simple_order(),
                        builder: None
                    },
                    nonce,
                    now,
                    fixed_nonce()
                )
                .is_err()
        );
    }
    for (market_id, order_id) in [(255, 1), (0, u64::MAX)] {
        assert!(
            signer
                .sign_with_nonce(
                    &Transaction::Cancel {
                        market_id,
                        order_id
                    },
                    7,
                    1_700_000_000_000,
                    fixed_nonce()
                )
                .is_err()
        );
    }
    for leverage in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.5, 10_001.0] {
        assert!(
            signer
                .sign_with_nonce(
                    &Transaction::Leverage {
                        market_id: 0,
                        leverage,
                        margin_mode: 0
                    },
                    7,
                    1_700_000_000_000,
                    fixed_nonce()
                )
                .is_err()
        );
    }
}

#[test]
fn malformed_groups_reject_before_any_transaction_can_be_submitted() {
    let signer = signer();
    let parent = simple_order();
    let mut stop = simple_order();
    stop.order_id = 2;
    stop.is_buy = false;
    stop.order_type = 2;
    stop.amount = 0;
    let valid = vec![parent.clone(), stop.clone()];
    assert!(
        signer
            .sign_with_nonce(
                &Transaction::Grouped {
                    grouping_type: 1,
                    orders: valid.clone(),
                    builder: None
                },
                7,
                1_700_000_000_000,
                fixed_nonce()
            )
            .is_ok()
    );
    let mut wrong_side = stop.clone();
    wrong_side.is_buy = true;
    let mut wrong_market = stop.clone();
    wrong_market.market_id = 2;
    let mut duplicate = stop.clone();
    duplicate.order_id = parent.order_id;
    let mut wrong_amount = stop;
    wrong_amount.amount = 10;
    for (grouping_type, orders) in [
        (0, valid.clone()),
        (3, valid),
        (1, vec![]),
        (1, vec![parent.clone()]),
        (1, vec![parent.clone(), wrong_side]),
        (1, vec![parent.clone(), wrong_market]),
        (1, vec![parent.clone(), duplicate]),
        (1, vec![parent, wrong_amount]),
    ] {
        assert!(
            signer
                .sign_with_nonce(
                    &Transaction::Grouped {
                        grouping_type,
                        orders,
                        builder: None
                    },
                    7,
                    1_700_000_000_000,
                    fixed_nonce()
                )
                .is_err()
        );
    }
}

#[test]
fn live_signing_draws_fresh_nonce_and_signatures_verify() {
    let signer = signer();
    let tx = Transaction::Create {
        order: simple_order(),
        builder: None,
    };
    let first = signer.sign(&tx, 7, 1_700_000_000_000).unwrap();
    let second = signer.sign(&tx, 7, 1_700_000_000_000).unwrap();
    assert_ne!(first.signature, second.signature);
    assert_eq!(first.data, second.data);
    let key = [1u8].into_iter().chain([0; 39]).collect::<Vec<_>>();
    let public_key = safe_cex_crypto::lighter::public_key(&key).unwrap();
    for signed in [first, second] {
        let signature = signed
            .signature
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        assert!(
            safe_cex_crypto::lighter::schnorr_verify(&signed.data, &public_key, &signature)
                .unwrap()
        );
    }
}
