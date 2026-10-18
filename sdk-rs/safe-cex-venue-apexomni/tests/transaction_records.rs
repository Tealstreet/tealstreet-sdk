use safe_cex_num::numeric_ingress::{JsNumberOutput, js_from_str};
use safe_cex_venue_apexomni::signer_records::{EthSignatureKind, TxEthSignature};
use safe_cex_venue_apexomni::transaction_records::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn corpus() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/transaction-records.cjs"
            ))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .output()
            .expect("execute actual transaction declarations");
        assert!(
            output.status.success(),
            "actual transaction declarations: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn bits(actual: f64, expected: &Value, label: &str) {
    let actual = if actual.is_nan() {
        0x7ff8000000000000
    } else {
        actual.to_bits()
    };
    assert_eq!(&expected["type"], "number", "{label} source native type");
    assert_eq!(
        actual,
        u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap(),
        "{label}"
    );
}

fn check_tx_signature(native: &TxSignature, expected: &Value) {
    assert_eq!(
        native.pub_key,
        expected["pubKey"].as_str().unwrap(),
        "TxSignature native pubKey"
    );
    assert_eq!(
        native.signature,
        expected["signature"].as_str().unwrap(),
        "TxSignature native signature"
    );
}

fn check_sign_contract_payload(native: &SignContractPayload, expected: &Value) {
    bits(
        native.account_id,
        &expected["accountId"],
        "SignContractPayload native accountId",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "SignContractPayload native subAccountId",
    );
    bits(
        native.slot_id,
        &expected["slotId"],
        "SignContractPayload native slotId",
    );
    bits(
        native.nonce,
        &expected["nonce"],
        "SignContractPayload native nonce",
    );
    bits(
        native.pair_id,
        &expected["pairId"],
        "SignContractPayload native pairId",
    );
    assert_eq!(
        native.size,
        expected["size"].as_str().unwrap(),
        "SignContractPayload native size"
    );
    assert_eq!(
        native.price,
        expected["price"].as_str().unwrap(),
        "SignContractPayload native price"
    );
    assert_eq!(
        native.direction,
        expected["direction"].as_bool().unwrap(),
        "SignContractPayload native direction"
    );
    bits(
        native.fee_rates[0],
        &expected["feeRates"][0],
        "SignContractPayload native feeRates.0",
    );
    bits(
        native.fee_rates[1],
        &expected["feeRates"][1],
        "SignContractPayload native feeRates.1",
    );
    if let Some(value) = &native.has_subsidy {
        assert!(
            expected.get("hasSubsidy").is_some(),
            "SignContractPayload native hasSubsidy presence"
        );
        assert_eq!(
            *value,
            expected["hasSubsidy"].as_bool().unwrap(),
            "SignContractPayload native hasSubsidy"
        );
    } else {
        assert!(
            expected.get("hasSubsidy").is_none(),
            "SignContractPayload native hasSubsidy absence"
        );
    }
}

fn check_tx_contract_data(native: &TxContractData, expected: &Value) {
    bits(
        native.account_id,
        &expected["accountId"],
        "TxContractData native accountId",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "TxContractData native subAccountId",
    );
    bits(
        native.slot_id,
        &expected["slotId"],
        "TxContractData native slotId",
    );
    bits(
        native.nonce,
        &expected["nonce"],
        "TxContractData native nonce",
    );
    bits(
        native.pair_id,
        &expected["pairId"],
        "TxContractData native pairId",
    );
    assert_eq!(
        native.size,
        expected["size"].as_str().unwrap(),
        "TxContractData native size"
    );
    assert_eq!(
        native.price,
        expected["price"].as_str().unwrap(),
        "TxContractData native price"
    );
    bits(
        native.direction,
        &expected["direction"],
        "TxContractData native direction",
    );
    bits(
        native.fee_rates[0],
        &expected["feeRates"][0],
        "TxContractData native feeRates.0",
    );
    bits(
        native.fee_rates[1],
        &expected["feeRates"][1],
        "TxContractData native feeRates.1",
    );
    bits(
        native.has_subsidy,
        &expected["hasSubsidy"],
        "TxContractData native hasSubsidy",
    );
    check_tx_signature(&native.signature, &expected["signature"]);
}

fn check_signed_contract_data(native: &SignedContractData, expected: &Value) {
    bits(
        native.tx.pair_id,
        &expected["tx"]["pairId"],
        "SignedContractData native tx.pairId",
    );
    check_tx_contract_data(&native.tx, &expected["tx"]);
}

fn check_sign_order_payload(native: &SignOrderPayload, expected: &Value) {
    bits(
        native.account_id,
        &expected["accountId"],
        "SignOrderPayload native accountId",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "SignOrderPayload native subAccountId",
    );
    bits(
        native.slot_id,
        &expected["slotId"],
        "SignOrderPayload native slotId",
    );
    bits(
        native.nonce,
        &expected["nonce"],
        "SignOrderPayload native nonce",
    );
    bits(
        native.base_token_id,
        &expected["baseTokenId"],
        "SignOrderPayload native baseTokenId",
    );
    bits(
        native.quote_token_id,
        &expected["quoteTokenId"],
        "SignOrderPayload native quoteTokenId",
    );
    assert_eq!(
        native.amount,
        expected["amount"].as_str().unwrap(),
        "SignOrderPayload native amount"
    );
    assert_eq!(
        native.price,
        expected["price"].as_str().unwrap(),
        "SignOrderPayload native price"
    );
    assert_eq!(
        native.is_sell,
        expected["isSell"].as_bool().unwrap(),
        "SignOrderPayload native isSell"
    );
    bits(
        native.fee_rates[0],
        &expected["feeRates"][0],
        "SignOrderPayload native feeRates.0",
    );
    bits(
        native.fee_rates[1],
        &expected["feeRates"][1],
        "SignOrderPayload native feeRates.1",
    );
    if let Some(value) = &native.has_subsidy {
        assert!(
            expected.get("hasSubsidy").is_some(),
            "SignOrderPayload native hasSubsidy presence"
        );
        assert_eq!(
            *value,
            expected["hasSubsidy"].as_bool().unwrap(),
            "SignOrderPayload native hasSubsidy"
        );
    } else {
        assert!(
            expected.get("hasSubsidy").is_none(),
            "SignOrderPayload native hasSubsidy absence"
        );
    }
}

fn check_tx_order_data(native: &TxOrderData, expected: &Value) {
    bits(
        native.account_id,
        &expected["accountId"],
        "TxOrderData native accountId",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "TxOrderData native subAccountId",
    );
    bits(
        native.slot_id,
        &expected["slotId"],
        "TxOrderData native slotId",
    );
    bits(native.nonce, &expected["nonce"], "TxOrderData native nonce");
    bits(
        native.base_token_id,
        &expected["baseTokenId"],
        "TxOrderData native baseTokenId",
    );
    bits(
        native.quote_token_id,
        &expected["quoteTokenId"],
        "TxOrderData native quoteTokenId",
    );
    assert_eq!(
        native.amount,
        expected["amount"].as_str().unwrap(),
        "TxOrderData native amount"
    );
    assert_eq!(
        native.price,
        expected["price"].as_str().unwrap(),
        "TxOrderData native price"
    );
    bits(
        native.is_sell,
        &expected["isSell"],
        "TxOrderData native isSell",
    );
    bits(
        native.fee_rates[0],
        &expected["feeRates"][0],
        "TxOrderData native feeRates.0",
    );
    bits(
        native.fee_rates[1],
        &expected["feeRates"][1],
        "TxOrderData native feeRates.1",
    );
    bits(
        native.has_subsidy,
        &expected["hasSubsidy"],
        "TxOrderData native hasSubsidy",
    );
    check_tx_signature(&native.signature, &expected["signature"]);
}

fn check_signed_order_data(native: &SignedOrderData, expected: &Value) {
    bits(
        native.tx.base_token_id,
        &expected["tx"]["baseTokenId"],
        "SignedOrderData native tx.baseTokenId",
    );
    check_tx_order_data(&native.tx, &expected["tx"]);
}

fn check_sign_transfer_payload(native: &SignTransferPayload, expected: &Value) {
    bits(
        native.account_id,
        &expected["accountId"],
        "SignTransferPayload native accountId",
    );
    assert_eq!(
        native.to,
        expected["to"].as_str().unwrap(),
        "SignTransferPayload native to"
    );
    bits(
        native.from_sub_account_id,
        &expected["fromSubAccountId"],
        "SignTransferPayload native fromSubAccountId",
    );
    bits(
        native.to_sub_account_id,
        &expected["toSubAccountId"],
        "SignTransferPayload native toSubAccountId",
    );
    bits(
        native.token_id,
        &expected["tokenId"],
        "SignTransferPayload native tokenId",
    );
    assert_eq!(
        native.token_symbol,
        expected["tokenSymbol"].as_str().unwrap(),
        "SignTransferPayload native tokenSymbol"
    );
    assert_eq!(
        native.amount,
        expected["amount"].as_str().unwrap(),
        "SignTransferPayload native amount"
    );
    assert_eq!(
        native.fee,
        expected["fee"].as_str().unwrap(),
        "SignTransferPayload native fee"
    );
    bits(
        native.nonce,
        &expected["nonce"],
        "SignTransferPayload native nonce",
    );
    if let Some(value) = &native.ts {
        assert!(
            expected.get("ts").is_some(),
            "SignTransferPayload native ts presence"
        );
        bits(*value, &expected["ts"], "SignTransferPayload native ts");
    } else {
        assert!(
            expected.get("ts").is_none(),
            "SignTransferPayload native ts absence"
        );
    }
}

fn check_tx_transfer_data(native: &TxTransferData, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "Transfer" => TransferKind::Transfer,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "TxTransferData native type");
    bits(
        native.account_id,
        &expected["accountId"],
        "TxTransferData native accountId",
    );
    bits(
        native.from_sub_account_id,
        &expected["fromSubAccountId"],
        "TxTransferData native fromSubAccountId",
    );
    bits(
        native.to_sub_account_id,
        &expected["toSubAccountId"],
        "TxTransferData native toSubAccountId",
    );
    assert_eq!(
        native.to,
        expected["to"].as_str().unwrap(),
        "TxTransferData native to"
    );
    bits(
        native.token,
        &expected["token"],
        "TxTransferData native token",
    );
    assert_eq!(
        native.amount,
        expected["amount"].as_str().unwrap(),
        "TxTransferData native amount"
    );
    assert_eq!(
        native.fee,
        expected["fee"].as_str().unwrap(),
        "TxTransferData native fee"
    );
    bits(native.ts, &expected["ts"], "TxTransferData native ts");
    bits(
        native.nonce,
        &expected["nonce"],
        "TxTransferData native nonce",
    );
    check_tx_signature(&native.signature, &expected["signature"]);
}

fn check_signed_transfer_data(native: &SignedTransferData, expected: &Value) {
    bits(
        native.tx.to_sub_account_id,
        &expected["tx"]["toSubAccountId"],
        "SignedTransferData native tx.toSubAccountId",
    );
    check_tx_transfer_data(&native.tx, &expected["tx"]);
    check_tx_eth_signature(&native.layer1_signature, &expected["layer1_signature"]);
}

fn check_sign_withdraw_payload(native: &SignWithdrawPayload, expected: &Value) {
    bits(
        native.to_chain_id,
        &expected["toChainId"],
        "SignWithdrawPayload native toChainId",
    );
    assert_eq!(
        native.withdraw_to_l1,
        expected["withdrawToL1"].as_bool().unwrap(),
        "SignWithdrawPayload native withdrawToL1"
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "SignWithdrawPayload native subAccountId",
    );
    assert_eq!(
        native.to,
        expected["to"].as_str().unwrap(),
        "SignWithdrawPayload native to"
    );
    bits(
        native.l2_source_token_id,
        &expected["l2SourceTokenId"],
        "SignWithdrawPayload native l2SourceTokenId",
    );
    assert_eq!(
        native.l2_source_token_symbol,
        expected["l2SourceTokenSymbol"].as_str().unwrap(),
        "SignWithdrawPayload native l2SourceTokenSymbol"
    );
    bits(
        native.l1_target_token_id,
        &expected["l1TargetTokenId"],
        "SignWithdrawPayload native l1TargetTokenId",
    );
    bits(
        native.token_decimals,
        &expected["tokenDecimals"],
        "SignWithdrawPayload native tokenDecimals",
    );
    assert_eq!(
        native.amount,
        expected["amount"].as_str().unwrap(),
        "SignWithdrawPayload native amount"
    );
    bits(
        native.withdraw_fee_ratio,
        &expected["withdrawFeeRatio"],
        "SignWithdrawPayload native withdrawFeeRatio",
    );
    bits(
        native.account_id,
        &expected["accountId"],
        "SignWithdrawPayload native accountId",
    );
    assert_eq!(
        native.fee,
        expected["fee"].as_str().unwrap(),
        "SignWithdrawPayload native fee"
    );
    bits(
        native.nonce,
        &expected["nonce"],
        "SignWithdrawPayload native nonce",
    );
    if let Some(value) = &native.from {
        assert!(
            expected.get("from").is_some(),
            "SignWithdrawPayload native from presence"
        );
        assert_eq!(
            value,
            expected["from"].as_str().unwrap(),
            "SignWithdrawPayload native from"
        );
    } else {
        assert!(
            expected.get("from").is_none(),
            "SignWithdrawPayload native from absence"
        );
    }
    if let Some(value) = &native.ts {
        assert!(
            expected.get("ts").is_some(),
            "SignWithdrawPayload native ts presence"
        );
        bits(*value, &expected["ts"], "SignWithdrawPayload native ts");
    } else {
        assert!(
            expected.get("ts").is_none(),
            "SignWithdrawPayload native ts absence"
        );
    }
}

fn check_tx_withdraw_data(native: &TxWithdrawData, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "Withdraw" => WithdrawKind::Withdraw,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "TxWithdrawData native type");
    bits(
        native.to_chain_id,
        &expected["toChainId"],
        "TxWithdrawData native toChainId",
    );
    bits(
        native.withdraw_to_l1,
        &expected["withdrawToL1"],
        "TxWithdrawData native withdrawToL1",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "TxWithdrawData native subAccountId",
    );
    bits(
        native.account_id,
        &expected["accountId"],
        "TxWithdrawData native accountId",
    );
    assert_eq!(
        native.from,
        expected["from"].as_str().unwrap(),
        "TxWithdrawData native from"
    );
    assert_eq!(
        native.to,
        expected["to"].as_str().unwrap(),
        "TxWithdrawData native to"
    );
    bits(
        native.l2_source_token,
        &expected["l2SourceToken"],
        "TxWithdrawData native l2SourceToken",
    );
    bits(
        native.l1_target_token,
        &expected["l1TargetToken"],
        "TxWithdrawData native l1TargetToken",
    );
    assert_eq!(
        native.amount,
        expected["amount"].as_str().unwrap(),
        "TxWithdrawData native amount"
    );
    assert_eq!(
        native.fee,
        expected["fee"].as_str().unwrap(),
        "TxWithdrawData native fee"
    );
    bits(
        native.withdraw_fee_ratio,
        &expected["withdrawFeeRatio"],
        "TxWithdrawData native withdrawFeeRatio",
    );
    bits(native.ts, &expected["ts"], "TxWithdrawData native ts");
    bits(
        native.nonce,
        &expected["nonce"],
        "TxWithdrawData native nonce",
    );
    check_tx_signature(&native.signature, &expected["signature"]);
}

fn check_signed_withdraw_data(native: &SignedWithdrawData, expected: &Value) {
    bits(
        native.tx.withdraw_fee_ratio,
        &expected["tx"]["withdrawFeeRatio"],
        "SignedWithdrawData native tx.withdrawFeeRatio",
    );
    check_tx_withdraw_data(&native.tx, &expected["tx"]);
    check_tx_eth_signature(&native.layer1_signature, &expected["layer1_signature"]);
}

fn check_change_pub_key_onchain(native: &ChangePubKeyOnchain, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "Onchain" => OnchainKind::Onchain,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "ChangePubKeyOnchain native type");
}

fn check_change_pub_key_ecdsa(native: &ChangePubKeyECDSA, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "EthECDSA" => EcdsaKind::EthECDSA,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "ChangePubKeyECDSA native type");
    assert_eq!(
        native.eth_signature,
        expected["ethSignature"].as_str().unwrap(),
        "ChangePubKeyECDSA native ethSignature"
    );
}

fn check_change_pub_key_create2(native: &ChangePubKeyCREATE2, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "EthCreate2" => Create2Kind::EthCreate2,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "ChangePubKeyCREATE2 native type");
    assert_eq!(
        native.creator_address,
        expected["creatorAddress"].as_str().unwrap(),
        "ChangePubKeyCREATE2 native creatorAddress"
    );
    assert_eq!(
        native.salt_arg,
        expected["saltArg"].as_str().unwrap(),
        "ChangePubKeyCREATE2 native saltArg"
    );
    assert_eq!(
        native.code_hash,
        expected["codeHash"].as_str().unwrap(),
        "ChangePubKeyCREATE2 native codeHash"
    );
}

fn check_sign_change_pub_key_payload(native: &SignChangePubKeyPayload, expected: &Value) {
    bits(
        native.chain_id,
        &expected["chainId"],
        "SignChangePubKeyPayload native chainId",
    );
    bits(
        native.account_id,
        &expected["accountId"],
        "SignChangePubKeyPayload native accountId",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "SignChangePubKeyPayload native subAccountId",
    );
    bits(
        native.fee_token_id,
        &expected["feeTokenId"],
        "SignChangePubKeyPayload native feeTokenId",
    );
    let kind = match expected["ethAuthType"].as_str().unwrap() {
        "Onchain" => ChangePubkeyTypes::Onchain,
        "EthECDSA" => ChangePubkeyTypes::EthECDSA,
        "EthCreate2" => ChangePubkeyTypes::EthCreate2,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(
        native.eth_auth_type, kind,
        "SignChangePubKeyPayload native ethAuthType"
    );
    assert_eq!(
        native.fee,
        expected["fee"].as_str().unwrap(),
        "SignChangePubKeyPayload native fee"
    );
    bits(
        native.nonce,
        &expected["nonce"],
        "SignChangePubKeyPayload native nonce",
    );
    if let Some(value) = &native.new_pk_hash {
        assert!(
            expected.get("newPkHash").is_some(),
            "SignChangePubKeyPayload native newPkHash presence"
        );
        assert_eq!(
            value,
            expected["newPkHash"].as_str().unwrap(),
            "SignChangePubKeyPayload native newPkHash"
        );
    } else {
        assert!(
            expected.get("newPkHash").is_none(),
            "SignChangePubKeyPayload native newPkHash absence"
        );
    }
    if let Some(value) = &native.account {
        assert!(
            expected.get("account").is_some(),
            "SignChangePubKeyPayload native account presence"
        );
        assert_eq!(
            value,
            expected["account"].as_str().unwrap(),
            "SignChangePubKeyPayload native account"
        );
    } else {
        assert!(
            expected.get("account").is_none(),
            "SignChangePubKeyPayload native account absence"
        );
    }
    if let Some(value) = &native.ts {
        assert!(
            expected.get("ts").is_some(),
            "SignChangePubKeyPayload native ts presence"
        );
        bits(*value, &expected["ts"], "SignChangePubKeyPayload native ts");
    } else {
        assert!(
            expected.get("ts").is_none(),
            "SignChangePubKeyPayload native ts absence"
        );
    }
}

fn check_tx_change_pub_key_data(native: &TxChangePubKeyData, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "ChangePubKey" => ChangePubKeyKind::ChangePubKey,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "TxChangePubKeyData native type");
    bits(
        native.chain_id,
        &expected["chainId"],
        "TxChangePubKeyData native chainId",
    );
    bits(
        native.sub_account_id,
        &expected["subAccountId"],
        "TxChangePubKeyData native subAccountId",
    );
    assert_eq!(
        native.account,
        expected["account"].as_str().unwrap(),
        "TxChangePubKeyData native account"
    );
    bits(
        native.account_id,
        &expected["accountId"],
        "TxChangePubKeyData native accountId",
    );
    assert_eq!(
        native.new_pk_hash,
        expected["newPkHash"].as_str().unwrap(),
        "TxChangePubKeyData native newPkHash"
    );
    bits(
        native.fee_token,
        &expected["feeToken"],
        "TxChangePubKeyData native feeToken",
    );
    assert_eq!(
        native.fee,
        expected["fee"].as_str().unwrap(),
        "TxChangePubKeyData native fee"
    );
    bits(native.ts, &expected["ts"], "TxChangePubKeyData native ts");
    bits(
        native.nonce,
        &expected["nonce"],
        "TxChangePubKeyData native nonce",
    );
    check_tx_signature(&native.signature, &expected["signature"]);
    if let Some(value) = &native.eth_auth_data {
        assert!(
            expected.get("ethAuthData").is_some(),
            "TxChangePubKeyData native ethAuthData presence"
        );
        match value {
            ChangePubKeyAuthData::Onchain(data) => {
                check_change_pub_key_onchain(data, &expected["ethAuthData"])
            }
            ChangePubKeyAuthData::EthECDSA(data) => {
                check_change_pub_key_ecdsa(data, &expected["ethAuthData"])
            }
            ChangePubKeyAuthData::EthCreate2(data) => {
                check_change_pub_key_create2(data, &expected["ethAuthData"])
            }
        }
    } else {
        assert!(
            expected.get("ethAuthData").is_none(),
            "TxChangePubKeyData native ethAuthData absence"
        );
    }
}

fn check_signed_change_pub_key_data(native: &SignedChangePubKeyData, expected: &Value) {
    bits(
        native.tx.chain_id,
        &expected["tx"]["chainId"],
        "SignedChangePubKeyData native tx.chainId",
    );
    check_tx_change_pub_key_data(&native.tx, &expected["tx"]);
    if let Some(value) = &native.layer1_signature {
        assert!(
            expected.get("layer1_signature").is_some(),
            "SignedChangePubKeyData native layer1_signature presence"
        );
        check_tx_eth_signature(value, &expected["layer1_signature"]);
    } else {
        assert!(
            expected.get("layer1_signature").is_none(),
            "SignedChangePubKeyData native layer1_signature absence"
        );
    }
}

fn check_tx_eth_signature(native: &TxEthSignature, expected: &Value) {
    let kind = match expected["type"].as_str().unwrap() {
        "EthereumSignature" => EthSignatureKind::EthereumSignature,
        "EIP1271Signature" => EthSignatureKind::EIP1271Signature,
        _ => panic!("unclassified actual transaction kind"),
    };
    assert_eq!(native.kind, kind, "TxEthSignature native type");
    assert_eq!(
        native.signature,
        expected["signature"].as_str().unwrap(),
        "TxEthSignature native signature"
    );
}

fn check_row<T: for<'de> Deserialize<'de> + serde::Serialize + JsNumberOutput>(
    name: &str,
    expected_fields: Value,
    check: fn(&T, &Value),
) {
    let rows = corpus()["rows"].as_array().unwrap();
    assert_eq!(corpus()["header"]["rustArch"], std::env::consts::ARCH);
    assert!(!corpus()["header"]["nodeArch"].as_str().unwrap().is_empty());
    assert_eq!(rows.len(), 19, "transaction source record census");
    let row = rows.iter().find(|r| r["name"] == name).unwrap();
    assert_eq!(
        row["fields"], expected_fields,
        "{name} actual declared fields"
    );
    if name == "ChangePubKeyOnchain" {
        assert_eq!(row["boundary"], Value::Null);
    } else {
        assert_eq!(row["boundary"]["units"], json!([0xd800]));
        typed_utf16::assert_projection(
            row["boundary"]["raw"].as_str().unwrap(),
            &(js_from_str::<T>(row["boundary"]["raw"].as_str().unwrap()))
                .expect("typed surrogate record is retained"),
        );
    }
    let samples = row["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 7, "{name} source sample floor");
    let mut comparisons = 0;
    for (index, sample) in samples.iter().enumerate() {
        let raw = sample["raw"].as_str().unwrap();
        let native: T = js_from_str(raw).unwrap();
        check(&native, &sample["native"]);
        native.check_number_output();
        comparisons += 1;
        if index < 4 {
            let input: Value = serde_json::from_str(raw).unwrap();
            let owned: T = serde_json::from_value(input.clone()).unwrap();
            check(&owned, &sample["native"]);
            let borrowed = T::deserialize(&input).unwrap();
            check(&borrowed, &sample["native"]);
            comparisons += 2;
            let emitted = serde_json::to_value(&native).unwrap();
            assert_eq!(
                safe_cex_num::js_json_stringify(&emitted.into())
                    .unwrap()
                    .unwrap(),
                raw,
                "{name} emitted fields"
            );
            for field in row["fields"].as_array().unwrap() {
                let key = field["name"].as_str().unwrap();
                let mut bad = input.clone();
                bad[key] = Value::Null;
                assert!(
                    js_from_str::<T>(&bad.to_string()).is_err(),
                    "{name} rejects null {key}"
                );
                bad.as_object_mut().unwrap().remove(key);
                if field["optional"] == false {
                    assert!(
                        js_from_str::<T>(&bad.to_string()).is_err(),
                        "{name} required {key}"
                    );
                }
                if field["shape"]["kind"] == "tuple" {
                    for tuple in [json!([]), json!([1]), json!([1, 2, 3])] {
                        let mut bad = input.clone();
                        bad[key] = tuple;
                        assert!(
                            js_from_str::<T>(&bad.to_string()).is_err(),
                            "{name} tuple arity {key}"
                        );
                    }
                }
                if field["shape"]["kind"] == "literal" || field["shape"]["kind"] == "union" {
                    let mut bad = input.clone();
                    bad[key] = json!(2);
                    assert!(
                        js_from_str::<T>(&bad.to_string()).is_err(),
                        "{name} literal/union value {key}"
                    );
                }
            }
            let array = input
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                js_from_str::<T>(&json!(array).to_string()).is_err(),
                "{name} rejects positional array"
            );
        }
    }
    assert_eq!(comparisons, 15, "{name} independent decoded sample floor");
}

#[test]
fn tx_signature_native() {
    check_row("TxSignature", serde_json::from_str(r#"[{"name":"pubKey","optional":false,"shape":{"kind":"string"}},{"name":"signature","optional":false,"shape":{"kind":"string"}}]"#).unwrap(), check_tx_signature);
}

#[test]
fn sign_contract_payload_native() {
    check_row("SignContractPayload", serde_json::from_str(r#"[{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"slotId","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"pairId","optional":false,"shape":{"kind":"number"}},{"name":"size","optional":false,"shape":{"kind":"string"}},{"name":"price","optional":false,"shape":{"kind":"string"}},{"name":"direction","optional":false,"shape":{"kind":"boolean"}},{"name":"feeRates","optional":false,"shape":{"kind":"tuple","items":[{"kind":"number"},{"kind":"number"}]}},{"name":"hasSubsidy","optional":true,"shape":{"kind":"boolean"}}]"#).unwrap(), check_sign_contract_payload);
}

#[test]
fn tx_contract_data_native() {
    check_row("TxContractData", serde_json::from_str(r#"[{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"slotId","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"pairId","optional":false,"shape":{"kind":"number"}},{"name":"size","optional":false,"shape":{"kind":"string"}},{"name":"price","optional":false,"shape":{"kind":"string"}},{"name":"direction","optional":false,"shape":{"kind":"union","items":[{"kind":"literal","values":[0]},{"kind":"literal","values":[1]}]}},{"name":"feeRates","optional":false,"shape":{"kind":"tuple","items":[{"kind":"number"},{"kind":"number"}]}},{"name":"hasSubsidy","optional":false,"shape":{"kind":"union","items":[{"kind":"literal","values":[0]},{"kind":"literal","values":[1]}]}},{"name":"signature","optional":false,"shape":{"kind":"record","name":"TxSignature"}}]"#).unwrap(), check_tx_contract_data);
}

#[test]
fn signed_contract_data_native() {
    check_row(
        "SignedContractData",
        serde_json::from_str(
            r#"[{"name":"tx","optional":false,"shape":{"kind":"record","name":"TxContractData"}}]"#,
        )
        .unwrap(),
        check_signed_contract_data,
    );
}

#[test]
fn sign_order_payload_native() {
    check_row("SignOrderPayload", serde_json::from_str(r#"[{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"slotId","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"baseTokenId","optional":false,"shape":{"kind":"number"}},{"name":"quoteTokenId","optional":false,"shape":{"kind":"number"}},{"name":"amount","optional":false,"shape":{"kind":"string"}},{"name":"price","optional":false,"shape":{"kind":"string"}},{"name":"isSell","optional":false,"shape":{"kind":"boolean"}},{"name":"feeRates","optional":false,"shape":{"kind":"tuple","items":[{"kind":"number"},{"kind":"number"}]}},{"name":"hasSubsidy","optional":true,"shape":{"kind":"boolean"}}]"#).unwrap(), check_sign_order_payload);
}

#[test]
fn tx_order_data_native() {
    check_row("TxOrderData", serde_json::from_str(r#"[{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"slotId","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"baseTokenId","optional":false,"shape":{"kind":"number"}},{"name":"quoteTokenId","optional":false,"shape":{"kind":"number"}},{"name":"amount","optional":false,"shape":{"kind":"string"}},{"name":"price","optional":false,"shape":{"kind":"string"}},{"name":"isSell","optional":false,"shape":{"kind":"union","items":[{"kind":"literal","values":[0]},{"kind":"literal","values":[1]}]}},{"name":"feeRates","optional":false,"shape":{"kind":"tuple","items":[{"kind":"number"},{"kind":"number"}]}},{"name":"hasSubsidy","optional":false,"shape":{"kind":"union","items":[{"kind":"literal","values":[0]},{"kind":"literal","values":[1]}]}},{"name":"signature","optional":false,"shape":{"kind":"record","name":"TxSignature"}}]"#).unwrap(), check_tx_order_data);
}

#[test]
fn signed_order_data_native() {
    check_row(
        "SignedOrderData",
        serde_json::from_str(
            r#"[{"name":"tx","optional":false,"shape":{"kind":"record","name":"TxOrderData"}}]"#,
        )
        .unwrap(),
        check_signed_order_data,
    );
}

#[test]
fn sign_transfer_payload_native() {
    check_row("SignTransferPayload", serde_json::from_str(r#"[{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"to","optional":false,"shape":{"kind":"string"}},{"name":"fromSubAccountId","optional":false,"shape":{"kind":"number"}},{"name":"toSubAccountId","optional":false,"shape":{"kind":"number"}},{"name":"tokenId","optional":false,"shape":{"kind":"number"}},{"name":"tokenSymbol","optional":false,"shape":{"kind":"string"}},{"name":"amount","optional":false,"shape":{"kind":"string"}},{"name":"fee","optional":false,"shape":{"kind":"string"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"ts","optional":true,"shape":{"kind":"number"}}]"#).unwrap(), check_sign_transfer_payload);
}

#[test]
fn tx_transfer_data_native() {
    check_row("TxTransferData", serde_json::from_str(r#"[{"name":"type","optional":false,"shape":{"kind":"literal","values":["Transfer"]}},{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"fromSubAccountId","optional":false,"shape":{"kind":"number"}},{"name":"toSubAccountId","optional":false,"shape":{"kind":"number"}},{"name":"to","optional":false,"shape":{"kind":"string"}},{"name":"token","optional":false,"shape":{"kind":"number"}},{"name":"amount","optional":false,"shape":{"kind":"string"}},{"name":"fee","optional":false,"shape":{"kind":"string"}},{"name":"ts","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"signature","optional":false,"shape":{"kind":"record","name":"TxSignature"}}]"#).unwrap(), check_tx_transfer_data);
}

#[test]
fn signed_transfer_data_native() {
    check_row("SignedTransferData", serde_json::from_str(r#"[{"name":"tx","optional":false,"shape":{"kind":"record","name":"TxTransferData"}},{"name":"layer1_signature","optional":false,"shape":{"kind":"record","name":"TxEthSignature"}}]"#).unwrap(), check_signed_transfer_data);
}

#[test]
fn sign_withdraw_payload_native() {
    check_row("SignWithdrawPayload", serde_json::from_str(r#"[{"name":"toChainId","optional":false,"shape":{"kind":"number"}},{"name":"withdrawToL1","optional":false,"shape":{"kind":"boolean"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"to","optional":false,"shape":{"kind":"string"}},{"name":"l2SourceTokenId","optional":false,"shape":{"kind":"number"}},{"name":"l2SourceTokenSymbol","optional":false,"shape":{"kind":"string"}},{"name":"l1TargetTokenId","optional":false,"shape":{"kind":"number"}},{"name":"tokenDecimals","optional":false,"shape":{"kind":"number"}},{"name":"amount","optional":false,"shape":{"kind":"string"}},{"name":"withdrawFeeRatio","optional":false,"shape":{"kind":"number"}},{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"fee","optional":false,"shape":{"kind":"string"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"from","optional":true,"shape":{"kind":"string"}},{"name":"ts","optional":true,"shape":{"kind":"number"}}]"#).unwrap(), check_sign_withdraw_payload);
}

#[test]
fn tx_withdraw_data_native() {
    check_row("TxWithdrawData", serde_json::from_str(r#"[{"name":"type","optional":false,"shape":{"kind":"literal","values":["Withdraw"]}},{"name":"toChainId","optional":false,"shape":{"kind":"number"}},{"name":"withdrawToL1","optional":false,"shape":{"kind":"union","items":[{"kind":"literal","values":[0]},{"kind":"literal","values":[1]}]}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"from","optional":false,"shape":{"kind":"string"}},{"name":"to","optional":false,"shape":{"kind":"string"}},{"name":"l2SourceToken","optional":false,"shape":{"kind":"number"}},{"name":"l1TargetToken","optional":false,"shape":{"kind":"number"}},{"name":"amount","optional":false,"shape":{"kind":"string"}},{"name":"fee","optional":false,"shape":{"kind":"string"}},{"name":"withdrawFeeRatio","optional":false,"shape":{"kind":"number"}},{"name":"ts","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"signature","optional":false,"shape":{"kind":"record","name":"TxSignature"}}]"#).unwrap(), check_tx_withdraw_data);
}

#[test]
fn signed_withdraw_data_native() {
    check_row("SignedWithdrawData", serde_json::from_str(r#"[{"name":"tx","optional":false,"shape":{"kind":"record","name":"TxWithdrawData"}},{"name":"layer1_signature","optional":false,"shape":{"kind":"record","name":"TxEthSignature"}}]"#).unwrap(), check_signed_withdraw_data);
}

#[test]
fn change_pub_key_onchain_native() {
    check_row(
        "ChangePubKeyOnchain",
        serde_json::from_str(
            r#"[{"name":"type","optional":false,"shape":{"kind":"literal","values":["Onchain"]}}]"#,
        )
        .unwrap(),
        check_change_pub_key_onchain,
    );
}

#[test]
fn change_pub_key_ecdsa_native() {
    check_row("ChangePubKeyECDSA", serde_json::from_str(r#"[{"name":"type","optional":false,"shape":{"kind":"literal","values":["EthECDSA"]}},{"name":"ethSignature","optional":false,"shape":{"kind":"string"}}]"#).unwrap(), check_change_pub_key_ecdsa);
}

#[test]
fn change_pub_key_create2_native() {
    check_row("ChangePubKeyCREATE2", serde_json::from_str(r#"[{"name":"type","optional":false,"shape":{"kind":"literal","values":["EthCreate2"]}},{"name":"creatorAddress","optional":false,"shape":{"kind":"string"}},{"name":"saltArg","optional":false,"shape":{"kind":"string"}},{"name":"codeHash","optional":false,"shape":{"kind":"string"}}]"#).unwrap(), check_change_pub_key_create2);
}

#[test]
fn sign_change_pub_key_payload_native() {
    check_row("SignChangePubKeyPayload", serde_json::from_str(r#"[{"name":"chainId","optional":false,"shape":{"kind":"number"}},{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"feeTokenId","optional":false,"shape":{"kind":"number"}},{"name":"ethAuthType","optional":false,"shape":{"kind":"union","items":[{"kind":"literal","values":["Onchain"]},{"kind":"literal","values":["EthECDSA"]},{"kind":"literal","values":["EthCreate2"]}]}},{"name":"fee","optional":false,"shape":{"kind":"string"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"newPkHash","optional":true,"shape":{"kind":"string"}},{"name":"account","optional":true,"shape":{"kind":"string"}},{"name":"ts","optional":true,"shape":{"kind":"number"}}]"#).unwrap(), check_sign_change_pub_key_payload);
}

#[test]
fn tx_change_pub_key_data_native() {
    check_row("TxChangePubKeyData", serde_json::from_str(r#"[{"name":"type","optional":false,"shape":{"kind":"literal","values":["ChangePubKey"]}},{"name":"chainId","optional":false,"shape":{"kind":"number"}},{"name":"subAccountId","optional":false,"shape":{"kind":"number"}},{"name":"account","optional":false,"shape":{"kind":"string"}},{"name":"accountId","optional":false,"shape":{"kind":"number"}},{"name":"newPkHash","optional":false,"shape":{"kind":"string"}},{"name":"feeToken","optional":false,"shape":{"kind":"number"}},{"name":"fee","optional":false,"shape":{"kind":"string"}},{"name":"ts","optional":false,"shape":{"kind":"number"}},{"name":"nonce","optional":false,"shape":{"kind":"number"}},{"name":"signature","optional":false,"shape":{"kind":"record","name":"TxSignature"}},{"name":"ethAuthData","optional":true,"shape":{"kind":"union","items":[{"kind":"record","name":"ChangePubKeyOnchain"},{"kind":"record","name":"ChangePubKeyECDSA"},{"kind":"record","name":"ChangePubKeyCREATE2"}]}}]"#).unwrap(), check_tx_change_pub_key_data);
}

#[test]
fn signed_change_pub_key_data_native() {
    check_row("SignedChangePubKeyData", serde_json::from_str(r#"[{"name":"tx","optional":false,"shape":{"kind":"record","name":"TxChangePubKeyData"}},{"name":"layer1_signature","optional":true,"shape":{"kind":"record","name":"TxEthSignature"}}]"#).unwrap(), check_signed_change_pub_key_data);
}

#[test]
fn change_pubkey_types_native() {
    assert_eq!(
        corpus()["kinds"],
        json!({"kind":"union", "items":[{"kind":"literal", "values":["Onchain"]},{"kind":"literal", "values":["EthECDSA"]},{"kind":"literal", "values":["EthCreate2"]}]})
    );
    for (raw, expected) in [
        ("Onchain", ChangePubkeyTypes::Onchain),
        ("EthECDSA", ChangePubkeyTypes::EthECDSA),
        ("EthCreate2", ChangePubkeyTypes::EthCreate2),
    ] {
        let native: ChangePubkeyTypes = js_from_str(&json!(raw).to_string()).unwrap();
        assert_eq!(native, expected, "ChangePubkeyTypes native variant");
    }
    for bad in [json!(0), json!(["Onchain"]), json!("other"), Value::Null] {
        assert!(js_from_str::<ChangePubkeyTypes>(&bad.to_string()).is_err());
    }
}

#[test]
fn transaction_string_boundary() {
    assert_eq!(corpus()["boundary"]["units"], json!([0xd800]));
    typed_utf16::assert_projection(
        corpus()["boundary"]["raw"].as_str().unwrap(),
        &(js_from_str::<TxSignature>(corpus()["boundary"]["raw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
