#[path = "support/reference_signer.rs"]
mod reference_signer;
use reference_signer::Signer;
// This corpus was captured from the TS adapter and real Go-WASM before this port.
use safe_cex_crypto::lighter::SchnorrNonce;
use safe_cex_venue_lighter::sign::{Builder, Order, Transaction};
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
        "getCancelOrderTransaction" => Transaction::Cancel {
            market_id: integer(&value["marketId"]) as u32,
            order_id: integer(&value["orderId"]),
        },
        "getUpdateLeverageTransaction" => Transaction::Leverage {
            market_id: integer(&value["marketId"]) as u32,
            leverage: value["leverage"].as_f64().unwrap(),
            margin_mode: integer(&value["marginMode"]) as u8,
        },
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
        if name.contains("create-grouped-") || name.contains("/approve-") {
            continue;
        }
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
    assert_eq!((transactions, authentications, batches), (64, 4, 4));
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
    for (market_id, order_id) in [(255, 1), (0, 0), (0, u64::MAX)] {
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
fn host_signing_draws_fresh_nonce_and_signatures_verify() {
    let signer = signer();
    let tx = Transaction::Create {
        order: simple_order(),
        builder: None,
    };
    let first = signer
        .sign(&tx, 7, 1_700_000_000_000, |bytes| {
            bytes.fill(1);
            Ok(())
        })
        .unwrap();
    let second = signer
        .sign(&tx, 7, 1_700_000_000_000, |bytes| {
            bytes.fill(2);
            Ok(())
        })
        .unwrap();
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

#[test]
fn current_int16_markets_match_original_sdk_and_independent_wasm_signatures() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/high-market-signing.json")).unwrap();
    let decode = |text: &str| {
        text.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let public_key = decode(fixture["publicKey"].as_str().unwrap());
    let signer = signer();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let market_id = case["marketId"].as_u64().unwrap() as u32;
        let tx = match case["name"].as_str().unwrap() {
            "create" => {
                let mut order = simple_order();
                order.market_id = market_id;
                order.order_id = 123;
                order.price = 10_000;
                order.amount = 250;
                Transaction::Create {
                    order,
                    builder: None,
                }
            }
            "cancel" => Transaction::Cancel {
                market_id,
                order_id: 123,
            },
            "leverage" => Transaction::Leverage {
                market_id,
                leverage: 5.0,
                margin_mode: 0,
            },
            other => panic!("uncaptured transaction {other}"),
        };
        let signed = signer
            .sign_with_nonce(&tx, 7, fixture["now"].as_u64().unwrap(), fixed_nonce())
            .unwrap();
        assert_eq!(
            safe_cex_crypto::hex(&signed.data),
            case["payload"].as_str().unwrap()
        );
        assert!(
            safe_cex_crypto::lighter::schnorr_verify(
                &signed.data,
                &public_key,
                &decode(case["signature"].as_str().unwrap())
            )
            .unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 6);
    for market_id in [255, 32768, u32::MAX] {
        let mut order = simple_order();
        order.market_id = market_id;
        for tx in [
            Transaction::Create {
                order,
                builder: None,
            },
            Transaction::Cancel {
                market_id,
                order_id: 123,
            },
            Transaction::Leverage {
                market_id,
                leverage: 5.0,
                margin_mode: 0,
            },
        ] {
            assert!(
                signer
                    .sign_with_nonce(&tx, 7, 1_700_000_000_000, fixed_nonce())
                    .is_err()
            );
        }
    }
}

#[test]
fn official_account_order_amount_and_timestamp_bounds_are_enforced() {
    const MAX_ACCOUNT: u64 = (1 << 48) - 2;
    const MAX_VALUE: u64 = (1 << 48) - 1;
    const MAX_INDEX: u64 = (1 << 60) - 1;
    let key = format!("01{}", "00".repeat(39));
    assert!(Signer::new(&key, 304, MAX_ACCOUNT, 254).is_ok());
    assert!(Signer::new(&key, 304, MAX_ACCOUNT + 1, 254).is_err());
    let signer = signer();
    let mut valid = simple_order();
    valid.order_id = MAX_VALUE;
    valid.amount = MAX_VALUE;
    valid.order_expiry = Some((1 << 53) - 1);
    assert!(
        signer
            .sign_with_nonce(
                &Transaction::Create {
                    order: valid.clone(),
                    builder: None
                },
                7,
                1_700_000_000_000,
                fixed_nonce()
            )
            .is_ok()
    );
    for invalid in [
        {
            let mut o = valid.clone();
            o.order_id = MAX_VALUE + 1;
            o
        },
        {
            let mut o = valid.clone();
            o.amount = MAX_VALUE + 1;
            o
        },
        {
            let mut o = valid.clone();
            o.order_expiry = Some(1 << 53);
            o
        },
    ] {
        assert!(
            signer
                .sign_with_nonce(
                    &Transaction::Create {
                        order: invalid,
                        builder: None
                    },
                    7,
                    1_700_000_000_000,
                    fixed_nonce()
                )
                .is_err()
        );
    }
    assert!(
        signer
            .sign_with_nonce(
                &Transaction::Cancel {
                    market_id: 1,
                    order_id: MAX_INDEX
                },
                7,
                MAX_VALUE - 600_000,
                fixed_nonce()
            )
            .is_ok()
    );
    assert!(
        signer
            .sign_with_nonce(
                &Transaction::Cancel {
                    market_id: 1,
                    order_id: MAX_INDEX + 1
                },
                7,
                1_700_000_000_000,
                fixed_nonce()
            )
            .is_err()
    );
    assert!(
        signer
            .sign_with_nonce(
                &Transaction::Cancel {
                    market_id: 1,
                    order_id: 1
                },
                7,
                MAX_VALUE - 600_000 + 1,
                fixed_nonce()
            )
            .is_err()
    );
    assert!(
        signer
            .sign_with_nonce(
                &Transaction::Create {
                    order: simple_order(),
                    builder: Some(Builder {
                        account_index: MAX_ACCOUNT + 1,
                        maker_fee: 0,
                        taker_fee: 250
                    })
                },
                7,
                1_700_000_000_000,
                fixed_nonce()
            )
            .is_err()
    );
}
