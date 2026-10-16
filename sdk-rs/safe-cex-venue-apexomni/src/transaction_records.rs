//! Transaction declarations from the zkLink signer; signing remains a separate adapter.
use crate::signer_records::TxEthSignature;
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChangePubkeyTypes {
    Onchain,
    EthECDSA,
    EthCreate2,
}
impl<'de> serde::Deserialize<'de> for ChangePubkeyTypes {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "Onchain" => Ok(Self::Onchain),
            "EthECDSA" => Ok(Self::EthECDSA),
            "EthCreate2" => Ok(Self::EthCreate2),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for ChangePubkeyTypes {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Onchain, EthECDSA, EthCreate2 });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TransferKind {
    Transfer,
}
impl<'de> serde::Deserialize<'de> for TransferKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "Transfer" => Ok(Self::Transfer),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for TransferKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Transfer });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum WithdrawKind {
    Withdraw,
}
impl<'de> serde::Deserialize<'de> for WithdrawKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "Withdraw" => Ok(Self::Withdraw),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for WithdrawKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Withdraw });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChangePubKeyKind {
    ChangePubKey,
}
impl<'de> serde::Deserialize<'de> for ChangePubKeyKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "ChangePubKey" => Ok(Self::ChangePubKey),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for ChangePubKeyKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { ChangePubKey });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OnchainKind {
    Onchain,
}
impl<'de> serde::Deserialize<'de> for OnchainKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "Onchain" => Ok(Self::Onchain),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for OnchainKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Onchain });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum EcdsaKind {
    EthECDSA,
}
impl<'de> serde::Deserialize<'de> for EcdsaKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "EthECDSA" => Ok(Self::EthECDSA),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for EcdsaKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { EthECDSA });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Create2Kind {
    EthCreate2,
}
impl<'de> serde::Deserialize<'de> for Create2Kind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(decoder)?.as_str() {
            "EthCreate2" => Ok(Self::EthCreate2),
            _ => Err(serde::de::Error::custom("unsupported transaction literal")),
        }
    }
}
impl JsNumberOutput for Create2Kind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { EthCreate2 });
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ChangePubKeyAuthData {
    Onchain(ChangePubKeyOnchain),
    EthECDSA(ChangePubKeyECDSA),
    EthCreate2(ChangePubKeyCREATE2),
}
impl JsNumberOutput for ChangePubKeyAuthData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Onchain(value), EthECDSA(value), EthCreate2(value) });
    }
}

// ts: TxSignature
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxSignature {
    #[serde(rename = "pubKey")]
    pub pub_key: String,
    #[serde(rename = "signature")]
    pub signature: String,
}
impl<'de> serde::Deserialize<'de> for TxSignature {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "pubKey")]
            pub_key: String,
            #[serde(rename = "signature")]
            signature: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    pub_key: fields.pub_key,
                    signature: fields.signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxSignature {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { pub_key, signature });
    }
}

// ts: SignContractPayload
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignContractPayload {
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "slotId")]
    pub slot_id: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "pairId")]
    pub pair_id: f64,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "direction")]
    pub direction: bool,
    #[serde(rename = "feeRates")]
    pub fee_rates: [f64; 2],
    #[serde(rename = "hasSubsidy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_subsidy: Option<bool>,
}
impl<'de> serde::Deserialize<'de> for SignContractPayload {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "slotId")]
            slot_id: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "pairId")]
            pair_id: f64,
            #[serde(rename = "size")]
            size: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "direction")]
            direction: bool,
            #[serde(rename = "feeRates")]
            fee_rates: [f64; 2],
            #[serde(rename = "hasSubsidy")]
            #[serde(default, deserialize_with = "deserialize_present")]
            has_subsidy: Option<bool>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    account_id: fields.account_id,
                    sub_account_id: fields.sub_account_id,
                    slot_id: fields.slot_id,
                    nonce: fields.nonce,
                    pair_id: fields.pair_id,
                    size: fields.size,
                    price: fields.price,
                    direction: fields.direction,
                    fee_rates: fields.fee_rates,
                    has_subsidy: fields.has_subsidy,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignContractPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_id, sub_account_id, slot_id, nonce, pair_id, size, price, direction, fee_rates, has_subsidy });
    }
}

// ts: TxContractData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxContractData {
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "slotId")]
    pub slot_id: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "pairId")]
    pub pair_id: f64,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "direction")]
    pub direction: f64,
    #[serde(rename = "feeRates")]
    pub fee_rates: [f64; 2],
    #[serde(rename = "hasSubsidy")]
    pub has_subsidy: f64,
    #[serde(rename = "signature")]
    pub signature: TxSignature,
}
impl<'de> serde::Deserialize<'de> for TxContractData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "slotId")]
            slot_id: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "pairId")]
            pair_id: f64,
            #[serde(rename = "size")]
            size: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "direction")]
            #[serde(deserialize_with = "deserialize_flag")]
            direction: f64,
            #[serde(rename = "feeRates")]
            fee_rates: [f64; 2],
            #[serde(rename = "hasSubsidy")]
            #[serde(deserialize_with = "deserialize_flag")]
            has_subsidy: f64,
            #[serde(rename = "signature")]
            signature: TxSignature,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    account_id: fields.account_id,
                    sub_account_id: fields.sub_account_id,
                    slot_id: fields.slot_id,
                    nonce: fields.nonce,
                    pair_id: fields.pair_id,
                    size: fields.size,
                    price: fields.price,
                    direction: fields.direction,
                    fee_rates: fields.fee_rates,
                    has_subsidy: fields.has_subsidy,
                    signature: fields.signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxContractData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_id, sub_account_id, slot_id, nonce, pair_id, size, price, direction, fee_rates, has_subsidy, signature });
    }
}

// ts: SignedContractData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignedContractData {
    #[serde(rename = "tx")]
    pub tx: TxContractData,
}
impl<'de> serde::Deserialize<'de> for SignedContractData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "tx")]
            tx: TxContractData,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self { tx: fields.tx })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignedContractData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx });
    }
}

// ts: SignOrderPayload
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignOrderPayload {
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "slotId")]
    pub slot_id: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "baseTokenId")]
    pub base_token_id: f64,
    #[serde(rename = "quoteTokenId")]
    pub quote_token_id: f64,
    #[serde(rename = "amount")]
    pub amount: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "isSell")]
    pub is_sell: bool,
    #[serde(rename = "feeRates")]
    pub fee_rates: [f64; 2],
    #[serde(rename = "hasSubsidy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_subsidy: Option<bool>,
}
impl<'de> serde::Deserialize<'de> for SignOrderPayload {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "slotId")]
            slot_id: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "baseTokenId")]
            base_token_id: f64,
            #[serde(rename = "quoteTokenId")]
            quote_token_id: f64,
            #[serde(rename = "amount")]
            amount: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "isSell")]
            is_sell: bool,
            #[serde(rename = "feeRates")]
            fee_rates: [f64; 2],
            #[serde(rename = "hasSubsidy")]
            #[serde(default, deserialize_with = "deserialize_present")]
            has_subsidy: Option<bool>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    account_id: fields.account_id,
                    sub_account_id: fields.sub_account_id,
                    slot_id: fields.slot_id,
                    nonce: fields.nonce,
                    base_token_id: fields.base_token_id,
                    quote_token_id: fields.quote_token_id,
                    amount: fields.amount,
                    price: fields.price,
                    is_sell: fields.is_sell,
                    fee_rates: fields.fee_rates,
                    has_subsidy: fields.has_subsidy,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignOrderPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_id, sub_account_id, slot_id, nonce, base_token_id, quote_token_id, amount, price, is_sell, fee_rates, has_subsidy });
    }
}

// ts: TxOrderData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxOrderData {
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "slotId")]
    pub slot_id: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "baseTokenId")]
    pub base_token_id: f64,
    #[serde(rename = "quoteTokenId")]
    pub quote_token_id: f64,
    #[serde(rename = "amount")]
    pub amount: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "isSell")]
    pub is_sell: f64,
    #[serde(rename = "feeRates")]
    pub fee_rates: [f64; 2],
    #[serde(rename = "hasSubsidy")]
    pub has_subsidy: f64,
    #[serde(rename = "signature")]
    pub signature: TxSignature,
}
impl<'de> serde::Deserialize<'de> for TxOrderData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "slotId")]
            slot_id: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "baseTokenId")]
            base_token_id: f64,
            #[serde(rename = "quoteTokenId")]
            quote_token_id: f64,
            #[serde(rename = "amount")]
            amount: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "isSell")]
            #[serde(deserialize_with = "deserialize_flag")]
            is_sell: f64,
            #[serde(rename = "feeRates")]
            fee_rates: [f64; 2],
            #[serde(rename = "hasSubsidy")]
            #[serde(deserialize_with = "deserialize_flag")]
            has_subsidy: f64,
            #[serde(rename = "signature")]
            signature: TxSignature,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    account_id: fields.account_id,
                    sub_account_id: fields.sub_account_id,
                    slot_id: fields.slot_id,
                    nonce: fields.nonce,
                    base_token_id: fields.base_token_id,
                    quote_token_id: fields.quote_token_id,
                    amount: fields.amount,
                    price: fields.price,
                    is_sell: fields.is_sell,
                    fee_rates: fields.fee_rates,
                    has_subsidy: fields.has_subsidy,
                    signature: fields.signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxOrderData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_id, sub_account_id, slot_id, nonce, base_token_id, quote_token_id, amount, price, is_sell, fee_rates, has_subsidy, signature });
    }
}

// ts: SignedOrderData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignedOrderData {
    #[serde(rename = "tx")]
    pub tx: TxOrderData,
}
impl<'de> serde::Deserialize<'de> for SignedOrderData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "tx")]
            tx: TxOrderData,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self { tx: fields.tx })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignedOrderData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx });
    }
}

// ts: SignTransferPayload
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignTransferPayload {
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "to")]
    pub to: String,
    #[serde(rename = "fromSubAccountId")]
    pub from_sub_account_id: f64,
    #[serde(rename = "toSubAccountId")]
    pub to_sub_account_id: f64,
    #[serde(rename = "tokenId")]
    pub token_id: f64,
    #[serde(rename = "tokenSymbol")]
    pub token_symbol: String,
    #[serde(rename = "amount")]
    pub amount: String,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "ts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<f64>,
}
impl<'de> serde::Deserialize<'de> for SignTransferPayload {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "to")]
            to: String,
            #[serde(rename = "fromSubAccountId")]
            from_sub_account_id: f64,
            #[serde(rename = "toSubAccountId")]
            to_sub_account_id: f64,
            #[serde(rename = "tokenId")]
            token_id: f64,
            #[serde(rename = "tokenSymbol")]
            token_symbol: String,
            #[serde(rename = "amount")]
            amount: String,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "ts")]
            #[serde(default, deserialize_with = "deserialize_present")]
            ts: Option<f64>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    account_id: fields.account_id,
                    to: fields.to,
                    from_sub_account_id: fields.from_sub_account_id,
                    to_sub_account_id: fields.to_sub_account_id,
                    token_id: fields.token_id,
                    token_symbol: fields.token_symbol,
                    amount: fields.amount,
                    fee: fields.fee,
                    nonce: fields.nonce,
                    ts: fields.ts,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignTransferPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_id, to, from_sub_account_id, to_sub_account_id, token_id, token_symbol, amount, fee, nonce, ts });
    }
}

// ts: TxTransferData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxTransferData {
    #[serde(rename = "type")]
    pub kind: TransferKind,
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "fromSubAccountId")]
    pub from_sub_account_id: f64,
    #[serde(rename = "toSubAccountId")]
    pub to_sub_account_id: f64,
    #[serde(rename = "to")]
    pub to: String,
    #[serde(rename = "token")]
    pub token: f64,
    #[serde(rename = "amount")]
    pub amount: String,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "ts")]
    pub ts: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "signature")]
    pub signature: TxSignature,
}
impl<'de> serde::Deserialize<'de> for TxTransferData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "type")]
            kind: TransferKind,
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "fromSubAccountId")]
            from_sub_account_id: f64,
            #[serde(rename = "toSubAccountId")]
            to_sub_account_id: f64,
            #[serde(rename = "to")]
            to: String,
            #[serde(rename = "token")]
            token: f64,
            #[serde(rename = "amount")]
            amount: String,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "ts")]
            ts: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "signature")]
            signature: TxSignature,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    kind: fields.kind,
                    account_id: fields.account_id,
                    from_sub_account_id: fields.from_sub_account_id,
                    to_sub_account_id: fields.to_sub_account_id,
                    to: fields.to,
                    token: fields.token,
                    amount: fields.amount,
                    fee: fields.fee,
                    ts: fields.ts,
                    nonce: fields.nonce,
                    signature: fields.signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxTransferData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, account_id, from_sub_account_id, to_sub_account_id, to, token, amount, fee, ts, nonce, signature });
    }
}

// ts: SignedTransferData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignedTransferData {
    #[serde(rename = "tx")]
    pub tx: TxTransferData,
    #[serde(rename = "layer1_signature")]
    pub layer1_signature: TxEthSignature,
}
impl<'de> serde::Deserialize<'de> for SignedTransferData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "tx")]
            tx: TxTransferData,
            #[serde(rename = "layer1_signature")]
            layer1_signature: TxEthSignature,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    tx: fields.tx,
                    layer1_signature: fields.layer1_signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignedTransferData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx, layer1_signature });
    }
}

// ts: SignWithdrawPayload
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignWithdrawPayload {
    #[serde(rename = "toChainId")]
    pub to_chain_id: f64,
    #[serde(rename = "withdrawToL1")]
    pub withdraw_to_l1: bool,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "to")]
    pub to: String,
    #[serde(rename = "l2SourceTokenId")]
    pub l2_source_token_id: f64,
    #[serde(rename = "l2SourceTokenSymbol")]
    pub l2_source_token_symbol: String,
    #[serde(rename = "l1TargetTokenId")]
    pub l1_target_token_id: f64,
    #[serde(rename = "tokenDecimals")]
    pub token_decimals: f64,
    #[serde(rename = "amount")]
    pub amount: String,
    #[serde(rename = "withdrawFeeRatio")]
    pub withdraw_fee_ratio: f64,
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "from")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(rename = "ts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<f64>,
}
impl<'de> serde::Deserialize<'de> for SignWithdrawPayload {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "toChainId")]
            to_chain_id: f64,
            #[serde(rename = "withdrawToL1")]
            withdraw_to_l1: bool,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "to")]
            to: String,
            #[serde(rename = "l2SourceTokenId")]
            l2_source_token_id: f64,
            #[serde(rename = "l2SourceTokenSymbol")]
            l2_source_token_symbol: String,
            #[serde(rename = "l1TargetTokenId")]
            l1_target_token_id: f64,
            #[serde(rename = "tokenDecimals")]
            token_decimals: f64,
            #[serde(rename = "amount")]
            amount: String,
            #[serde(rename = "withdrawFeeRatio")]
            withdraw_fee_ratio: f64,
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "from")]
            #[serde(default, deserialize_with = "deserialize_present")]
            from: Option<String>,
            #[serde(rename = "ts")]
            #[serde(default, deserialize_with = "deserialize_present")]
            ts: Option<f64>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    to_chain_id: fields.to_chain_id,
                    withdraw_to_l1: fields.withdraw_to_l1,
                    sub_account_id: fields.sub_account_id,
                    to: fields.to,
                    l2_source_token_id: fields.l2_source_token_id,
                    l2_source_token_symbol: fields.l2_source_token_symbol,
                    l1_target_token_id: fields.l1_target_token_id,
                    token_decimals: fields.token_decimals,
                    amount: fields.amount,
                    withdraw_fee_ratio: fields.withdraw_fee_ratio,
                    account_id: fields.account_id,
                    fee: fields.fee,
                    nonce: fields.nonce,
                    from: fields.from,
                    ts: fields.ts,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignWithdrawPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { to_chain_id, withdraw_to_l1, sub_account_id, to, l2_source_token_id, l2_source_token_symbol, l1_target_token_id, token_decimals, amount, withdraw_fee_ratio, account_id, fee, nonce, from, ts });
    }
}

// ts: TxWithdrawData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxWithdrawData {
    #[serde(rename = "type")]
    pub kind: WithdrawKind,
    #[serde(rename = "toChainId")]
    pub to_chain_id: f64,
    #[serde(rename = "withdrawToL1")]
    pub withdraw_to_l1: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "from")]
    pub from: String,
    #[serde(rename = "to")]
    pub to: String,
    #[serde(rename = "l2SourceToken")]
    pub l2_source_token: f64,
    #[serde(rename = "l1TargetToken")]
    pub l1_target_token: f64,
    #[serde(rename = "amount")]
    pub amount: String,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "withdrawFeeRatio")]
    pub withdraw_fee_ratio: f64,
    #[serde(rename = "ts")]
    pub ts: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "signature")]
    pub signature: TxSignature,
}
impl<'de> serde::Deserialize<'de> for TxWithdrawData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "type")]
            kind: WithdrawKind,
            #[serde(rename = "toChainId")]
            to_chain_id: f64,
            #[serde(rename = "withdrawToL1")]
            #[serde(deserialize_with = "deserialize_flag")]
            withdraw_to_l1: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "from")]
            from: String,
            #[serde(rename = "to")]
            to: String,
            #[serde(rename = "l2SourceToken")]
            l2_source_token: f64,
            #[serde(rename = "l1TargetToken")]
            l1_target_token: f64,
            #[serde(rename = "amount")]
            amount: String,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "withdrawFeeRatio")]
            withdraw_fee_ratio: f64,
            #[serde(rename = "ts")]
            ts: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "signature")]
            signature: TxSignature,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    kind: fields.kind,
                    to_chain_id: fields.to_chain_id,
                    withdraw_to_l1: fields.withdraw_to_l1,
                    sub_account_id: fields.sub_account_id,
                    account_id: fields.account_id,
                    from: fields.from,
                    to: fields.to,
                    l2_source_token: fields.l2_source_token,
                    l1_target_token: fields.l1_target_token,
                    amount: fields.amount,
                    fee: fields.fee,
                    withdraw_fee_ratio: fields.withdraw_fee_ratio,
                    ts: fields.ts,
                    nonce: fields.nonce,
                    signature: fields.signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxWithdrawData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, to_chain_id, withdraw_to_l1, sub_account_id, account_id, from, to, l2_source_token, l1_target_token, amount, fee, withdraw_fee_ratio, ts, nonce, signature });
    }
}

// ts: SignedWithdrawData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignedWithdrawData {
    #[serde(rename = "tx")]
    pub tx: TxWithdrawData,
    #[serde(rename = "layer1_signature")]
    pub layer1_signature: TxEthSignature,
}
impl<'de> serde::Deserialize<'de> for SignedWithdrawData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "tx")]
            tx: TxWithdrawData,
            #[serde(rename = "layer1_signature")]
            layer1_signature: TxEthSignature,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    tx: fields.tx,
                    layer1_signature: fields.layer1_signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignedWithdrawData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx, layer1_signature });
    }
}

// ts: ChangePubKeyOnchain
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChangePubKeyOnchain {
    #[serde(rename = "type")]
    pub kind: OnchainKind,
}
impl<'de> serde::Deserialize<'de> for ChangePubKeyOnchain {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "type")]
            kind: OnchainKind,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self { kind: fields.kind })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for ChangePubKeyOnchain {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind });
    }
}

// ts: ChangePubKeyECDSA
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChangePubKeyECDSA {
    #[serde(rename = "type")]
    pub kind: EcdsaKind,
    #[serde(rename = "ethSignature")]
    pub eth_signature: String,
}
impl<'de> serde::Deserialize<'de> for ChangePubKeyECDSA {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "type")]
            kind: EcdsaKind,
            #[serde(rename = "ethSignature")]
            eth_signature: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    kind: fields.kind,
                    eth_signature: fields.eth_signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for ChangePubKeyECDSA {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, eth_signature });
    }
}

// ts: ChangePubKeyCREATE2
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChangePubKeyCREATE2 {
    #[serde(rename = "type")]
    pub kind: Create2Kind,
    #[serde(rename = "creatorAddress")]
    pub creator_address: String,
    #[serde(rename = "saltArg")]
    pub salt_arg: String,
    #[serde(rename = "codeHash")]
    pub code_hash: String,
}
impl<'de> serde::Deserialize<'de> for ChangePubKeyCREATE2 {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "type")]
            kind: Create2Kind,
            #[serde(rename = "creatorAddress")]
            creator_address: String,
            #[serde(rename = "saltArg")]
            salt_arg: String,
            #[serde(rename = "codeHash")]
            code_hash: String,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    kind: fields.kind,
                    creator_address: fields.creator_address,
                    salt_arg: fields.salt_arg,
                    code_hash: fields.code_hash,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for ChangePubKeyCREATE2 {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, creator_address, salt_arg, code_hash });
    }
}

// ts: SignChangePubKeyPayload
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignChangePubKeyPayload {
    #[serde(rename = "chainId")]
    pub chain_id: f64,
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "feeTokenId")]
    pub fee_token_id: f64,
    #[serde(rename = "ethAuthType")]
    pub eth_auth_type: ChangePubkeyTypes,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "newPkHash")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_pk_hash: Option<String>,
    #[serde(rename = "account")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(rename = "ts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<f64>,
}
impl<'de> serde::Deserialize<'de> for SignChangePubKeyPayload {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "chainId")]
            chain_id: f64,
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "feeTokenId")]
            fee_token_id: f64,
            #[serde(rename = "ethAuthType")]
            eth_auth_type: ChangePubkeyTypes,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "newPkHash")]
            #[serde(default, deserialize_with = "deserialize_present")]
            new_pk_hash: Option<String>,
            #[serde(rename = "account")]
            #[serde(default, deserialize_with = "deserialize_present")]
            account: Option<String>,
            #[serde(rename = "ts")]
            #[serde(default, deserialize_with = "deserialize_present")]
            ts: Option<f64>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    chain_id: fields.chain_id,
                    account_id: fields.account_id,
                    sub_account_id: fields.sub_account_id,
                    fee_token_id: fields.fee_token_id,
                    eth_auth_type: fields.eth_auth_type,
                    fee: fields.fee,
                    nonce: fields.nonce,
                    new_pk_hash: fields.new_pk_hash,
                    account: fields.account,
                    ts: fields.ts,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignChangePubKeyPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { chain_id, account_id, sub_account_id, fee_token_id, eth_auth_type, fee, nonce, new_pk_hash, account, ts });
    }
}

// ts: TxChangePubKeyData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TxChangePubKeyData {
    #[serde(rename = "type")]
    pub kind: ChangePubKeyKind,
    #[serde(rename = "chainId")]
    pub chain_id: f64,
    #[serde(rename = "subAccountId")]
    pub sub_account_id: f64,
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "newPkHash")]
    pub new_pk_hash: String,
    #[serde(rename = "feeToken")]
    pub fee_token: f64,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "ts")]
    pub ts: f64,
    #[serde(rename = "nonce")]
    pub nonce: f64,
    #[serde(rename = "signature")]
    pub signature: TxSignature,
    #[serde(rename = "ethAuthData")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eth_auth_data: Option<ChangePubKeyAuthData>,
}
impl<'de> serde::Deserialize<'de> for TxChangePubKeyData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "type")]
            kind: ChangePubKeyKind,
            #[serde(rename = "chainId")]
            chain_id: f64,
            #[serde(rename = "subAccountId")]
            sub_account_id: f64,
            #[serde(rename = "account")]
            account: String,
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "newPkHash")]
            new_pk_hash: String,
            #[serde(rename = "feeToken")]
            fee_token: f64,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "ts")]
            ts: f64,
            #[serde(rename = "nonce")]
            nonce: f64,
            #[serde(rename = "signature")]
            signature: TxSignature,
            #[serde(rename = "ethAuthData")]
            #[serde(default, deserialize_with = "deserialize_present")]
            eth_auth_data: Option<ChangePubKeyAuthData>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    kind: fields.kind,
                    chain_id: fields.chain_id,
                    sub_account_id: fields.sub_account_id,
                    account: fields.account,
                    account_id: fields.account_id,
                    new_pk_hash: fields.new_pk_hash,
                    fee_token: fields.fee_token,
                    fee: fields.fee,
                    ts: fields.ts,
                    nonce: fields.nonce,
                    signature: fields.signature,
                    eth_auth_data: fields.eth_auth_data,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TxChangePubKeyData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { kind, chain_id, sub_account_id, account, account_id, new_pk_hash, fee_token, fee, ts, nonce, signature, eth_auth_data });
    }
}

// ts: SignedChangePubKeyData
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignedChangePubKeyData {
    #[serde(rename = "tx")]
    pub tx: TxChangePubKeyData,
    #[serde(rename = "layer1_signature")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer1_signature: Option<TxEthSignature>,
}
impl<'de> serde::Deserialize<'de> for SignedChangePubKeyData {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            #[serde(rename = "tx")]
            tx: TxChangePubKeyData,
            #[serde(rename = "layer1_signature")]
            #[serde(default, deserialize_with = "deserialize_present")]
            layer1_signature: Option<TxEthSignature>,
        }
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                let fields = Fields::deserialize(safe_cex_types::strict_decode::object(decoder))?;
                Ok(Self {
                    tx: fields.tx,
                    layer1_signature: fields.layer1_signature,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SignedChangePubKeyData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx, layer1_signature });
    }
}

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

fn deserialize_flag<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<f64, D::Error> {
    let value = <f64 as serde::Deserialize>::deserialize(decoder)?;
    if value == 0.0 || value == 1.0 {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("expected 0 or 1"))
    }
}
