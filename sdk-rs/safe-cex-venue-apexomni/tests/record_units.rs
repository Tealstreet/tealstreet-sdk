use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_apexomni::{
    config_records::*, order_records::*, private_records::*, public_records::*, signer_records::*,
    transaction_records::*,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::process::Command;

fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
        .collect()
}
fn expected(label: &str) -> &'static [u16] {
    match label {
        "high" => &[0xd800],
        "low" => &[0xdc00],
        "high-tail" => &[0xd800, 65],
        "pair" => &[0xd83d, 0xde00],
        "scalar" => &[233],
        "replacement" => &[0xfffd],
        "literal" => &[92, 117, 100, 56, 48, 48],
        _ => panic!("unselected String case"),
    }
}
fn check<T: DeserializeOwned + serde::Serialize + safe_cex_num::numeric_ingress::JsNumberOutput>(
    row: &Value,
    access: impl Fn(&T) -> &str,
) {
    let name = row["name"].as_str().unwrap();
    let label = row["label"].as_str().unwrap();
    let marker = format!("APEX-RECORD-UNIT {name}/{label}");
    let actual = js_from_str::<T>(row["raw"].as_str().unwrap());
    if ["high", "low", "high-tail"].contains(&label) {
        let actual = actual.expect("typed record retains surrogate leaf");
        assert_eq!(
            access(&actual),
            String::from_utf16_lossy(expected(label)),
            "{marker} typed U+FFFD leaf"
        );
        typed_utf16::assert_projection(row["raw"].as_str().unwrap(), &actual);
    } else {
        let actual =
            actual.unwrap_or_else(|error| panic!("{marker} native unexpected refusal: {error}"));
        assert_eq!(
            access(&actual).encode_utf16().collect::<Vec<_>>(),
            expected(label),
            "{marker} decoded native declared leaf units"
        );
    }
    assert_eq!(
        row["observation"]["kind"], "string",
        "{marker} source returned kind"
    );
    assert_eq!(
        units(&row["observation"]["units"]),
        expected(label),
        "{marker} source returned declared leaf units"
    );
}
#[test]
fn declared_record_unit_boundaries_are_native_first() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/record-unit-boundaries.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-RECORD-UNIT source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        350,
        "APEX-RECORD-UNIT independent source case floor"
    );
    let labels = [
        "high",
        "low",
        "high-tail",
        "pair",
        "scalar",
        "replacement",
        "literal",
    ];
    let names = [
        "KeyPair",
        "ISO8601",
        "ApiKeyCredentials",
        "WithPrice",
        "OrderWithClientId",
        "UserObject",
        "WalletObject",
        "OpenPositionObject",
        "AccountsItem",
        "AccountObject",
        "OrderObject",
        "WorstPriceObject",
        "CreateOrderOptionsObject",
        "FundingRateObject",
        "HistoricalPNLObject",
        "OrderResponseObject",
        "AccountBalanceObject",
        "TickerObject",
        "DepthObject",
        "GetHistoryFundingOptions",
        "PerpetualCurrencyObject",
        "CurrencyObject",
        "GlobalObject",
        "PerpetualContractObject",
        "MultiChainObject",
        "PerpetualObject",
        "SymbolObject",
        "TradeObject",
        "KlineObject",
        "HistoryFundingObject",
        "SymbolInfoObject",
        "ChangePubKeyECDSA",
        "ChangePubKeyCREATE2",
        "SignChangePubKeyPayload",
        "TxChangePubKeyData",
        "SignedChangePubKeyData",
        "SignContractPayload",
        "TxContractData",
        "SignedContractData",
        "SignOrderPayload",
        "TxOrderData",
        "SignedOrderData",
        "SignTransferPayload",
        "TxTransferData",
        "SignedTransferData",
        "SignWithdrawPayload",
        "TxWithdrawData",
        "SignedWithdrawData",
        "TxEthSignature",
        "TxSignature",
    ];
    let mut typed_boundaries = 0;
    let mut positive = 0;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row["name"],
            names[index / 7],
            "APEX-RECORD-UNIT independent row order"
        );
        assert_eq!(
            row["label"],
            labels[index % 7],
            "APEX-RECORD-UNIT independent sample order"
        );
        match row["name"].as_str().unwrap() {
            "KeyPair" => check::<KeyPair>(row, |value| value.public_key.as_str()),
            "ISO8601" => check::<Iso8601>(row, |value| match value {
                Iso8601::String(value) => value.as_str(),
                Iso8601::Number(_) => panic!("expected String variant"),
            }),
            "ApiKeyCredentials" => check::<ApiKeyCredentials>(row, |value| value.key.as_str()),
            "WithPrice" => check::<WithPrice>(row, |value| value.human_price.as_str()),
            "OrderWithClientId" => {
                check::<OrderWithClientId>(row, |value| value.position_id.as_str())
            }
            "UserObject" => check::<UserObject>(row, |value| value.ethereum_address.as_str()),
            "WalletObject" => check::<WalletObject>(row, |value| value.user_id.as_str()),
            "OpenPositionObject" => {
                check::<OpenPositionObject>(row, |value| value.user_id.as_str())
            }
            "AccountsItem" => check::<AccountsItem>(row, |value| value.taker_fee_rate.as_str()),
            "AccountObject" => check::<AccountObject>(row, |value| value.stark_key.as_str()),
            "OrderObject" => check::<OrderObject>(row, |value| value.id.as_str()),
            "WorstPriceObject" => {
                check::<WorstPriceObject>(row, |value| value.worst_price.as_str())
            }
            "CreateOrderOptionsObject" => {
                check::<CreateOrderOptionsObject>(row, |value| value.client_order_id.as_str())
            }
            "FundingRateObject" => check::<FundingRateObject>(row, |value| value.id.as_str()),
            "HistoricalPNLObject" => {
                check::<HistoricalPNLObject>(row, |value| value.symbol.as_str())
            }
            "OrderResponseObject" => check::<OrderResponseObject>(row, |value| value.id.as_str()),
            "AccountBalanceObject" => {
                check::<AccountBalanceObject>(row, |value| value.total_equity_value.as_str())
            }
            "TickerObject" => check::<TickerObject>(row, |value| value.funding_rate.as_str()),
            "DepthObject" => check::<DepthObject>(row, |value| value.a[0][0].as_str()),
            "GetHistoryFundingOptions" => {
                check::<GetHistoryFundingOptions>(row, |value| value.symbol.as_str())
            }
            "PerpetualCurrencyObject" => {
                check::<PerpetualCurrencyObject>(row, |value| value.usdc[0].id.as_str())
            }
            "CurrencyObject" => check::<CurrencyObject>(row, |value| value.id.as_str()),
            "GlobalObject" => check::<GlobalObject>(row, |value| value.fee_account_id.as_str()),
            "PerpetualContractObject" => check::<PerpetualContractObject>(row, |value| {
                value.baseline_position_value.as_str()
            }),
            "MultiChainObject" => check::<MultiChainObject>(row, |value| value.currency.as_str()),
            "PerpetualObject" => {
                check::<PerpetualObject>(row, |value| value.usdc_config.currency[0].id.as_str())
            }
            "SymbolObject" => check::<SymbolObject>(row, |value| value.currency[0].id.as_str()),
            "TradeObject" => check::<TradeObject>(row, |value| value.i.as_str()),
            "KlineObject" => check::<KlineObject>(row, |value| value.s.as_str()),
            "HistoryFundingObject" => {
                check::<HistoryFundingObject>(row, |value| value.symbol.as_str())
            }
            "SymbolInfoObject" => {
                check::<SymbolInfoObject>(row, |value| value.base_coin.as_deref().unwrap())
            }
            "ChangePubKeyECDSA" => {
                check::<ChangePubKeyECDSA>(row, |value| value.eth_signature.as_str())
            }
            "ChangePubKeyCREATE2" => {
                check::<ChangePubKeyCREATE2>(row, |value| value.creator_address.as_str())
            }
            "SignChangePubKeyPayload" => {
                check::<SignChangePubKeyPayload>(row, |value| value.fee.as_str())
            }
            "TxChangePubKeyData" => {
                check::<TxChangePubKeyData>(row, |value| value.account.as_str())
            }
            "SignedChangePubKeyData" => {
                check::<SignedChangePubKeyData>(row, |value| value.tx.account.as_str())
            }
            "SignContractPayload" => check::<SignContractPayload>(row, |value| value.size.as_str()),
            "TxContractData" => check::<TxContractData>(row, |value| value.size.as_str()),
            "SignedContractData" => {
                check::<SignedContractData>(row, |value| value.tx.size.as_str())
            }
            "SignOrderPayload" => check::<SignOrderPayload>(row, |value| value.amount.as_str()),
            "TxOrderData" => check::<TxOrderData>(row, |value| value.amount.as_str()),
            "SignedOrderData" => check::<SignedOrderData>(row, |value| value.tx.amount.as_str()),
            "SignTransferPayload" => check::<SignTransferPayload>(row, |value| value.to.as_str()),
            "TxTransferData" => check::<TxTransferData>(row, |value| value.to.as_str()),
            "SignedTransferData" => check::<SignedTransferData>(row, |value| value.tx.to.as_str()),
            "SignWithdrawPayload" => check::<SignWithdrawPayload>(row, |value| value.to.as_str()),
            "TxWithdrawData" => check::<TxWithdrawData>(row, |value| value.from.as_str()),
            "SignedWithdrawData" => {
                check::<SignedWithdrawData>(row, |value| value.tx.from.as_str())
            }
            "TxEthSignature" => check::<TxEthSignature>(row, |value| value.signature.as_str()),
            "TxSignature" => check::<TxSignature>(row, |value| value.pub_key.as_str()),
            name => panic!("APEX-RECORD-UNIT unselected target {name}"),
        }
        if index % 7 < 3 {
            typed_boundaries += 1
        } else {
            positive += 1
        }
    }
    assert_eq!(names.len(), 50, "APEX-RECORD-UNIT independent target floor");
    assert_eq!(
        typed_boundaries, 150,
        "APEX-RECORD-UNIT independent typed-boundary floor"
    );
    assert_eq!(
        positive, 200,
        "APEX-RECORD-UNIT independent native-positive floor"
    );
    // Every typed native leaf/boundary and source-unit assertion finishes before wire.
    for row in rows.iter().filter(|row| row["label"] == "high") {
        assert_eq!(
            row["returnedWire"], r#""\ud800""#,
            "APEX-RECORD-UNIT source materialized high wire"
        );
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
