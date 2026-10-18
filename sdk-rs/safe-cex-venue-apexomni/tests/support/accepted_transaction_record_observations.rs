//! Direct native fields, presence, variants and bits; no owner serialization.
pub(super) fn observe_enum_transfer(
    d: &safe_cex_venue_apexomni::transaction_records::TransferKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::TransferKind::Transfer => "Transfer",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_withdraw(
    d: &safe_cex_venue_apexomni::transaction_records::WithdrawKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::WithdrawKind::Withdraw => "Withdraw",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_changepub(
    d: &safe_cex_venue_apexomni::transaction_records::ChangePubKeyKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::ChangePubKeyKind::ChangePubKey => {
            "ChangePubKey"
        }
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_onchain(
    d: &safe_cex_venue_apexomni::transaction_records::OnchainKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::OnchainKind::Onchain => "Onchain",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_ecdsa(
    d: &safe_cex_venue_apexomni::transaction_records::EcdsaKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::EcdsaKind::EthECDSA => "EthECDSA",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_create2(
    d: &safe_cex_venue_apexomni::transaction_records::Create2Kind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::Create2Kind::EthCreate2 => "EthCreate2",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_auth(
    d: &safe_cex_venue_apexomni::transaction_records::ChangePubkeyTypes,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::transaction_records::ChangePubkeyTypes::Onchain => "Onchain",
        safe_cex_venue_apexomni::transaction_records::ChangePubkeyTypes::EthECDSA => "EthECDSA",
        safe_cex_venue_apexomni::transaction_records::ChangePubkeyTypes::EthCreate2 => "EthCreate2",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_eth(
    d: &safe_cex_venue_apexomni::signer_records::EthSignatureKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::signer_records::EthSignatureKind::EthereumSignature => {
            "EthereumSignature"
        }
        safe_cex_venue_apexomni::signer_records::EthSignatureKind::EIP1271Signature => {
            "EIP1271Signature"
        }
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_side(
    d: &safe_cex_venue_apexomni::interface::OrderSide,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::interface::OrderSide::Buy => "Buy",
        safe_cex_venue_apexomni::interface::OrderSide::Sell => "Sell",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_createorder(
    d: &safe_cex_venue_apexomni::order_records::CreateOrderKind,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::order_records::CreateOrderKind::Limit => "Limit",
        safe_cex_venue_apexomni::order_records::CreateOrderKind::Market => "Market",
        safe_cex_venue_apexomni::order_records::CreateOrderKind::StopLimit => "StopLimit",
        safe_cex_venue_apexomni::order_records::CreateOrderKind::StopMarket => "StopMarket",
        safe_cex_venue_apexomni::order_records::CreateOrderKind::TakeProfitLimit => {
            "TakeProfitLimit"
        }
        safe_cex_venue_apexomni::order_records::CreateOrderKind::TakeProfitMarket => {
            "TakeProfitMarket"
        }
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_createtif(
    d: &safe_cex_venue_apexomni::order_records::CreateTimeInForce,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::order_records::CreateTimeInForce::GoodTilCancel => "GoodTilCancel",
        safe_cex_venue_apexomni::order_records::CreateTimeInForce::FillOrKill => "FillOrKill",
        safe_cex_venue_apexomni::order_records::CreateTimeInForce::ImmediateOrCancel => {
            "ImmediateOrCancel"
        }
        safe_cex_venue_apexomni::order_records::CreateTimeInForce::PostOnly => "PostOnly",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_order(
    d: &safe_cex_venue_apexomni::interface::OrderType,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::interface::OrderType::Limit => "Limit",
        safe_cex_venue_apexomni::interface::OrderType::Market => "Market",
        safe_cex_venue_apexomni::interface::OrderType::Condition => "Condition",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_status(
    d: &safe_cex_venue_apexomni::interface::OrderStatus,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::interface::OrderStatus::Pending => "Pending",
        safe_cex_venue_apexomni::interface::OrderStatus::Open => "Open",
        safe_cex_venue_apexomni::interface::OrderStatus::Filled => "Filled",
        safe_cex_venue_apexomni::interface::OrderStatus::Canceled => "Canceled",
        safe_cex_venue_apexomni::interface::OrderStatus::Untriggered => "Untriggered",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_tif(
    d: &safe_cex_venue_apexomni::interface::TimeInForce,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::interface::TimeInForce::Gtt => "Gtt",
        safe_cex_venue_apexomni::interface::TimeInForce::Fok => "Fok",
        safe_cex_venue_apexomni::interface::TimeInForce::Ioc => "Ioc",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_enum_market(
    d: &safe_cex_venue_apexomni::interface::ApexMarket,
) -> serde_json::Value {
    let variant = match d {
        safe_cex_venue_apexomni::interface::ApexMarket::BtcUsdt => "BtcUsdt",
        safe_cex_venue_apexomni::interface::ApexMarket::BtcUsdc => "BtcUsdc",
        safe_cex_venue_apexomni::interface::ApexMarket::BtcUsd => "BtcUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::EthUsd => "EthUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::EthUsdc => "EthUsdc",
        safe_cex_venue_apexomni::interface::ApexMarket::LinkUsd => "LinkUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::AaveUsd => "AaveUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::UniUsd => "UniUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::SushiUsd => "SushiUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::SolUsd => "SolUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::YfiUsd => "YfiUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::OneinchUsd => "OneinchUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::AvaxUsd => "AvaxUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::SnxUsd => "SnxUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::CrvUsd => "CrvUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::UmaUsd => "UmaUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::DotUsd => "DotUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::DogeUsd => "DogeUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::MaticUsd => "MaticUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::MkrUsd => "MkrUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::FilUsd => "FilUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::AdaUsd => "AdaUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::AtomUsd => "AtomUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::CompUsd => "CompUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::BchUsd => "BchUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::LtcUsd => "LtcUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::EosUsd => "EosUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::AlgoUsd => "AlgoUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::ZrxUsd => "ZrxUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::XmrUsd => "XmrUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::ZecUsd => "ZecUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::EnjUsd => "EnjUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::EtcUsd => "EtcUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::XlmUsd => "XlmUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::TrxUsd => "TrxUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::XtzUsd => "XtzUsd",
        safe_cex_venue_apexomni::interface::ApexMarket::HntUsd => "HntUsd",
    };
    serde_json::json!(variant)
}
pub(super) fn observe_i(d: &safe_cex_venue_apexomni::order_records::Iso8601) -> serde_json::Value {
    match d {
        safe_cex_venue_apexomni::order_records::Iso8601::String(v) => {
            serde_json::json!({"variant":"String","value":v})
        }
        safe_cex_venue_apexomni::order_records::Iso8601::Number(v) => {
            serde_json::json!({"variant":"Number","bits":format!("{:016x}",v.to_bits())})
        }
    }
}
pub(super) fn observe_auth(
    d: &safe_cex_venue_apexomni::transaction_records::ChangePubKeyAuthData,
) -> serde_json::Value {
    match d {
        safe_cex_venue_apexomni::transaction_records::ChangePubKeyAuthData::Onchain(v) => {
            serde_json::json!({"variant":"Onchain","fields":observe_changepubkeyonchain(v)})
        }
        safe_cex_venue_apexomni::transaction_records::ChangePubKeyAuthData::EthECDSA(v) => {
            serde_json::json!({"variant":"EthECDSA","fields":observe_changepubkeyecdsa(v)})
        }
        safe_cex_venue_apexomni::transaction_records::ChangePubKeyAuthData::EthCreate2(v) => {
            serde_json::json!({"variant":"EthCreate2","fields":observe_changepubkeycreate2(v)})
        }
    }
}
pub(super) fn observe_txsignature(
    d: &safe_cex_venue_apexomni::transaction_records::TxSignature,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("pubKey".to_owned(), serde_json::json!(&d.pub_key)),
        ("signature".to_owned(), serde_json::json!(&d.signature)),
    ]))
}
pub(super) fn observe_signcontractpayload(
    d: &safe_cex_venue_apexomni::transaction_records::SignContractPayload,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("accountId".to_owned(),serde_json::json!(format!("{:016x}", (d.account_id).to_bits()))),("subAccountId".to_owned(),serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits()))),("slotId".to_owned(),serde_json::json!(format!("{:016x}", (d.slot_id).to_bits()))),("nonce".to_owned(),serde_json::json!(format!("{:016x}", (d.nonce).to_bits()))),("pairId".to_owned(),serde_json::json!(format!("{:016x}", (d.pair_id).to_bits()))),("size".to_owned(),serde_json::json!(&d.size)),("price".to_owned(),serde_json::json!(&d.price)),("direction".to_owned(),serde_json::json!(&d.direction)),("feeRates".to_owned(),serde_json::Value::Array((d.fee_rates).iter().map(|v|serde_json::json!(format!("{:016x}",v.to_bits()))).collect())),("hasSubsidy".to_owned(),(d.has_subsidy).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)})))]))
}
pub(super) fn observe_txcontractdata(
    d: &safe_cex_venue_apexomni::transaction_records::TxContractData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "accountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.account_id).to_bits())),
        ),
        (
            "subAccountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits())),
        ),
        (
            "slotId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.slot_id).to_bits())),
        ),
        (
            "nonce".to_owned(),
            serde_json::json!(format!("{:016x}", (d.nonce).to_bits())),
        ),
        (
            "pairId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.pair_id).to_bits())),
        ),
        ("size".to_owned(), serde_json::json!(&d.size)),
        ("price".to_owned(), serde_json::json!(&d.price)),
        (
            "direction".to_owned(),
            serde_json::json!(format!("{:016x}", (d.direction).to_bits())),
        ),
        (
            "feeRates".to_owned(),
            serde_json::Value::Array(
                (d.fee_rates)
                    .iter()
                    .map(|v| serde_json::json!(format!("{:016x}", v.to_bits())))
                    .collect(),
            ),
        ),
        (
            "hasSubsidy".to_owned(),
            serde_json::json!(format!("{:016x}", (d.has_subsidy).to_bits())),
        ),
        ("signature".to_owned(), observe_txsignature(&d.signature)),
    ]))
}
pub(super) fn observe_signedcontractdata(
    d: &safe_cex_venue_apexomni::transaction_records::SignedContractData,
) -> serde_json::Value {
    let serde_json::Value::Object(fields) = observe_txcontractdata(&d.tx) else {
        unreachable!()
    };
    serde_json::Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (format!("tx.{key}"), value))
            .collect(),
    )
}
pub(super) fn observe_signorderpayload(
    d: &safe_cex_venue_apexomni::transaction_records::SignOrderPayload,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("accountId".to_owned(),serde_json::json!(format!("{:016x}", (d.account_id).to_bits()))),("subAccountId".to_owned(),serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits()))),("slotId".to_owned(),serde_json::json!(format!("{:016x}", (d.slot_id).to_bits()))),("nonce".to_owned(),serde_json::json!(format!("{:016x}", (d.nonce).to_bits()))),("baseTokenId".to_owned(),serde_json::json!(format!("{:016x}", (d.base_token_id).to_bits()))),("quoteTokenId".to_owned(),serde_json::json!(format!("{:016x}", (d.quote_token_id).to_bits()))),("amount".to_owned(),serde_json::json!(&d.amount)),("price".to_owned(),serde_json::json!(&d.price)),("isSell".to_owned(),serde_json::json!(&d.is_sell)),("feeRates".to_owned(),serde_json::Value::Array((d.fee_rates).iter().map(|v|serde_json::json!(format!("{:016x}",v.to_bits()))).collect())),("hasSubsidy".to_owned(),(d.has_subsidy).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)})))]))
}
pub(super) fn observe_txorderdata(
    d: &safe_cex_venue_apexomni::transaction_records::TxOrderData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "accountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.account_id).to_bits())),
        ),
        (
            "subAccountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits())),
        ),
        (
            "slotId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.slot_id).to_bits())),
        ),
        (
            "nonce".to_owned(),
            serde_json::json!(format!("{:016x}", (d.nonce).to_bits())),
        ),
        (
            "baseTokenId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.base_token_id).to_bits())),
        ),
        (
            "quoteTokenId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.quote_token_id).to_bits())),
        ),
        ("amount".to_owned(), serde_json::json!(&d.amount)),
        ("price".to_owned(), serde_json::json!(&d.price)),
        (
            "isSell".to_owned(),
            serde_json::json!(format!("{:016x}", (d.is_sell).to_bits())),
        ),
        (
            "feeRates".to_owned(),
            serde_json::Value::Array(
                (d.fee_rates)
                    .iter()
                    .map(|v| serde_json::json!(format!("{:016x}", v.to_bits())))
                    .collect(),
            ),
        ),
        (
            "hasSubsidy".to_owned(),
            serde_json::json!(format!("{:016x}", (d.has_subsidy).to_bits())),
        ),
        ("signature".to_owned(), observe_txsignature(&d.signature)),
    ]))
}
pub(super) fn observe_signedorderdata(
    d: &safe_cex_venue_apexomni::transaction_records::SignedOrderData,
) -> serde_json::Value {
    let serde_json::Value::Object(fields) = observe_txorderdata(&d.tx) else {
        unreachable!()
    };
    serde_json::Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (format!("tx.{key}"), value))
            .collect(),
    )
}
pub(super) fn observe_signtransferpayload(
    d: &safe_cex_venue_apexomni::transaction_records::SignTransferPayload,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("accountId".to_owned(),serde_json::json!(format!("{:016x}", (d.account_id).to_bits()))),("to".to_owned(),serde_json::json!(&d.to)),("fromSubAccountId".to_owned(),serde_json::json!(format!("{:016x}", (d.from_sub_account_id).to_bits()))),("toSubAccountId".to_owned(),serde_json::json!(format!("{:016x}", (d.to_sub_account_id).to_bits()))),("tokenId".to_owned(),serde_json::json!(format!("{:016x}", (d.token_id).to_bits()))),("tokenSymbol".to_owned(),serde_json::json!(&d.token_symbol)),("amount".to_owned(),serde_json::json!(&d.amount)),("fee".to_owned(),serde_json::json!(&d.fee)),("nonce".to_owned(),serde_json::json!(format!("{:016x}", (d.nonce).to_bits()))),("ts".to_owned(),(d.ts).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))})))]))
}
pub(super) fn observe_txtransferdata(
    d: &safe_cex_venue_apexomni::transaction_records::TxTransferData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("type".to_owned(), observe_enum_transfer(&d.kind)),
        (
            "accountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.account_id).to_bits())),
        ),
        (
            "fromSubAccountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.from_sub_account_id).to_bits())),
        ),
        (
            "toSubAccountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.to_sub_account_id).to_bits())),
        ),
        ("to".to_owned(), serde_json::json!(&d.to)),
        (
            "token".to_owned(),
            serde_json::json!(format!("{:016x}", (d.token).to_bits())),
        ),
        ("amount".to_owned(), serde_json::json!(&d.amount)),
        ("fee".to_owned(), serde_json::json!(&d.fee)),
        (
            "ts".to_owned(),
            serde_json::json!(format!("{:016x}", (d.ts).to_bits())),
        ),
        (
            "nonce".to_owned(),
            serde_json::json!(format!("{:016x}", (d.nonce).to_bits())),
        ),
        ("signature".to_owned(), observe_txsignature(&d.signature)),
    ]))
}
pub(super) fn observe_signedtransferdata(
    d: &safe_cex_venue_apexomni::transaction_records::SignedTransferData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("tx".to_owned(), observe_txtransferdata(&d.tx)),
        (
            "layer1_signature".to_owned(),
            observe_txethsignature(&d.layer1_signature),
        ),
    ]))
}
pub(super) fn observe_signwithdrawpayload(
    d: &safe_cex_venue_apexomni::transaction_records::SignWithdrawPayload,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("toChainId".to_owned(),serde_json::json!(format!("{:016x}", (d.to_chain_id).to_bits()))),("withdrawToL1".to_owned(),serde_json::json!(&d.withdraw_to_l1)),("subAccountId".to_owned(),serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits()))),("to".to_owned(),serde_json::json!(&d.to)),("l2SourceTokenId".to_owned(),serde_json::json!(format!("{:016x}", (d.l2_source_token_id).to_bits()))),("l2SourceTokenSymbol".to_owned(),serde_json::json!(&d.l2_source_token_symbol)),("l1TargetTokenId".to_owned(),serde_json::json!(format!("{:016x}", (d.l1_target_token_id).to_bits()))),("tokenDecimals".to_owned(),serde_json::json!(format!("{:016x}", (d.token_decimals).to_bits()))),("amount".to_owned(),serde_json::json!(&d.amount)),("withdrawFeeRatio".to_owned(),serde_json::json!(format!("{:016x}", (d.withdraw_fee_ratio).to_bits()))),("accountId".to_owned(),serde_json::json!(format!("{:016x}", (d.account_id).to_bits()))),("fee".to_owned(),serde_json::json!(&d.fee)),("nonce".to_owned(),serde_json::json!(format!("{:016x}", (d.nonce).to_bits()))),("from".to_owned(),(d.from).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("ts".to_owned(),(d.ts).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))})))]))
}
pub(super) fn observe_txwithdrawdata(
    d: &safe_cex_venue_apexomni::transaction_records::TxWithdrawData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("type".to_owned(), observe_enum_withdraw(&d.kind)),
        (
            "toChainId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.to_chain_id).to_bits())),
        ),
        (
            "withdrawToL1".to_owned(),
            serde_json::json!(format!("{:016x}", (d.withdraw_to_l1).to_bits())),
        ),
        (
            "subAccountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits())),
        ),
        (
            "accountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.account_id).to_bits())),
        ),
        ("from".to_owned(), serde_json::json!(&d.from)),
        ("to".to_owned(), serde_json::json!(&d.to)),
        (
            "l2SourceToken".to_owned(),
            serde_json::json!(format!("{:016x}", (d.l2_source_token).to_bits())),
        ),
        (
            "l1TargetToken".to_owned(),
            serde_json::json!(format!("{:016x}", (d.l1_target_token).to_bits())),
        ),
        ("amount".to_owned(), serde_json::json!(&d.amount)),
        ("fee".to_owned(), serde_json::json!(&d.fee)),
        (
            "withdrawFeeRatio".to_owned(),
            serde_json::json!(format!("{:016x}", (d.withdraw_fee_ratio).to_bits())),
        ),
        (
            "ts".to_owned(),
            serde_json::json!(format!("{:016x}", (d.ts).to_bits())),
        ),
        (
            "nonce".to_owned(),
            serde_json::json!(format!("{:016x}", (d.nonce).to_bits())),
        ),
        ("signature".to_owned(), observe_txsignature(&d.signature)),
    ]))
}
pub(super) fn observe_signedwithdrawdata(
    d: &safe_cex_venue_apexomni::transaction_records::SignedWithdrawData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("tx".to_owned(), observe_txwithdrawdata(&d.tx)),
        (
            "layer1_signature".to_owned(),
            observe_txethsignature(&d.layer1_signature),
        ),
    ]))
}
pub(super) fn observe_changepubkeyonchain(
    d: &safe_cex_venue_apexomni::transaction_records::ChangePubKeyOnchain,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([(
        "type".to_owned(),
        observe_enum_onchain(&d.kind),
    )]))
}
pub(super) fn observe_changepubkeyecdsa(
    d: &safe_cex_venue_apexomni::transaction_records::ChangePubKeyECDSA,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("type".to_owned(), observe_enum_ecdsa(&d.kind)),
        (
            "ethSignature".to_owned(),
            serde_json::json!(&d.eth_signature),
        ),
    ]))
}
pub(super) fn observe_changepubkeycreate2(
    d: &safe_cex_venue_apexomni::transaction_records::ChangePubKeyCREATE2,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("type".to_owned(), observe_enum_create2(&d.kind)),
        (
            "creatorAddress".to_owned(),
            serde_json::json!(&d.creator_address),
        ),
        ("saltArg".to_owned(), serde_json::json!(&d.salt_arg)),
        ("codeHash".to_owned(), serde_json::json!(&d.code_hash)),
    ]))
}
pub(super) fn observe_signchangepubkeypayload(
    d: &safe_cex_venue_apexomni::transaction_records::SignChangePubKeyPayload,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("chainId".to_owned(),serde_json::json!(format!("{:016x}", (d.chain_id).to_bits()))),("accountId".to_owned(),serde_json::json!(format!("{:016x}", (d.account_id).to_bits()))),("subAccountId".to_owned(),serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits()))),("feeTokenId".to_owned(),serde_json::json!(format!("{:016x}", (d.fee_token_id).to_bits()))),("ethAuthType".to_owned(),observe_enum_auth(&d.eth_auth_type)),("fee".to_owned(),serde_json::json!(&d.fee)),("nonce".to_owned(),serde_json::json!(format!("{:016x}", (d.nonce).to_bits()))),("newPkHash".to_owned(),(d.new_pk_hash).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("account".to_owned(),(d.account).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("ts".to_owned(),(d.ts).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))})))]))
}
pub(super) fn observe_txchangepubkeydata(
    d: &safe_cex_venue_apexomni::transaction_records::TxChangePubKeyData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("type".to_owned(), observe_enum_changepub(&d.kind)),
        (
            "chainId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.chain_id).to_bits())),
        ),
        (
            "subAccountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.sub_account_id).to_bits())),
        ),
        ("account".to_owned(), serde_json::json!(&d.account)),
        (
            "accountId".to_owned(),
            serde_json::json!(format!("{:016x}", (d.account_id).to_bits())),
        ),
        ("newPkHash".to_owned(), serde_json::json!(&d.new_pk_hash)),
        (
            "feeToken".to_owned(),
            serde_json::json!(format!("{:016x}", (d.fee_token).to_bits())),
        ),
        ("fee".to_owned(), serde_json::json!(&d.fee)),
        (
            "ts".to_owned(),
            serde_json::json!(format!("{:016x}", (d.ts).to_bits())),
        ),
        (
            "nonce".to_owned(),
            serde_json::json!(format!("{:016x}", (d.nonce).to_bits())),
        ),
        ("signature".to_owned(), observe_txsignature(&d.signature)),
        (
            "ethAuthData".to_owned(),
            (d.eth_auth_data).as_ref().map_or_else(
                || serde_json::json!({"present":false}),
                |optional0| serde_json::json!({"present":true,"value":observe_auth(optional0)}),
            ),
        ),
    ]))
}
pub(super) fn observe_signedchangepubkeydata(
    d: &safe_cex_venue_apexomni::transaction_records::SignedChangePubKeyData,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("tx".to_owned(),observe_txchangepubkeydata(&d.tx)),("layer1_signature".to_owned(),(d.layer1_signature).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":observe_txethsignature(optional0)})))]))
}
pub(super) fn observe_txethsignature(
    d: &safe_cex_venue_apexomni::signer_records::TxEthSignature,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("type".to_owned(), observe_enum_eth(&d.kind)),
        ("signature".to_owned(), serde_json::json!(&d.signature)),
    ]))
}
pub(super) fn observe_createorderoptionsobject(
    d: &safe_cex_venue_apexomni::order_records::CreateOrderOptionsObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("clientOrderId".to_owned(),serde_json::json!(&d.client_order_id)),("positionId".to_owned(),serde_json::json!(&d.position_id)),("symbol".to_owned(),serde_json::json!(&d.symbol)),("side".to_owned(),observe_enum_side(&d.side)),("type".to_owned(),observe_enum_createorder(&d.r#type)),("size".to_owned(),serde_json::json!(&d.size)),("price".to_owned(),serde_json::json!(&d.price)),("limitFee".to_owned(),serde_json::json!(&d.limit_fee)),("timeInForce".to_owned(),(d.time_in_force).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":observe_enum_createtif(optional0)}))),("triggerPrice".to_owned(),(d.trigger_price).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("trailingPercent".to_owned(),(d.trailing_percent).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("reduceOnly".to_owned(),(d.reduce_only).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("brokerId".to_owned(),(d.broker_id).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)})))]))
}
pub(super) fn observe_orderresponseobject(
    d: &safe_cex_venue_apexomni::order_records::OrderResponseObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("id".to_owned(),serde_json::json!(&d.id)),("clientId".to_owned(),(d.client_id).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),("accountId".to_owned(),serde_json::json!(&d.account_id)),("symbol".to_owned(),observe_enum_market(&d.symbol)),("side".to_owned(),observe_enum_side(&d.side)),("price".to_owned(),serde_json::json!(&d.price)),("triggerPrice".to_owned(),(d.trigger_price).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":(optional0).as_ref().map_or_else(||serde_json::json!({"null":true}),|nullable1|serde_json::json!({"null":false,"value":serde_json::json!(nullable1)}))}))),("trailingPercent".to_owned(),(d.trailing_percent).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":(optional0).as_ref().map_or_else(||serde_json::json!({"null":true}),|nullable1|serde_json::json!({"null":false,"value":serde_json::json!(nullable1)}))}))),("size".to_owned(),serde_json::json!(&d.size)),("remainingSize".to_owned(),serde_json::json!(&d.remaining_size)),("type".to_owned(),observe_enum_order(&d.r#type)),("createdAt".to_owned(),observe_i(&d.created_at)),("unfillableAt".to_owned(),(d.unfillable_at).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":(optional0).as_ref().map_or_else(||serde_json::json!({"null":true}),|nullable1|serde_json::json!({"null":false,"value":observe_i(nullable1)}))}))),("expiresAt".to_owned(),(d.expires_at).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":observe_i(optional0)}))),("status".to_owned(),observe_enum_status(&d.status)),("timeInForce".to_owned(),observe_enum_tif(&d.time_in_force)),("postOnly".to_owned(),serde_json::json!(&d.post_only)),("cancelReason".to_owned(),(d.cancel_reason).as_ref().map_or_else(||serde_json::json!({"present":false}),|optional0|serde_json::json!({"present":true,"value":(optional0).as_ref().map_or_else(||serde_json::json!({"null":true}),|nullable1|serde_json::json!({"null":false,"value":serde_json::json!(nullable1)}))})))]))
}
