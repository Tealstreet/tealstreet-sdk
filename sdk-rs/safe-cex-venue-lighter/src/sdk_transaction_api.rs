//! Transaction history values and source-ordered API orchestration at native host boundaries.
use crate::{
    sdk_account_api::*,
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::SigningResult,
};
use num_bigint::BigInt;
use safe_cex_num::js_number_string;
use std::rc::Rc;
// ts: TransactionStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Confirmed,
    Failed,
}
impl TransactionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
        }
    }
}
// ts: TransactionType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    CreateOrder,
    CancelOrder,
    Transfer,
    Withdrawal,
    Deposit,
}
impl TransactionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateOrder => "create_order",
            Self::CancelOrder => "cancel_order",
            Self::Transfer => "transfer",
            Self::Withdrawal => "withdrawal",
            Self::Deposit => "deposit",
        }
    }
}
// ts: Transaction
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub tx_hash: Vec<u16>,
    pub r#type: TransactionType,
    pub account_index: Vec<u16>,
    pub api_key_index: f64,
    pub nonce: Vec<u16>,
    pub status: TransactionStatus,
    pub block_number: Option<f64>,
    pub block_timestamp: Option<f64>,
    pub sequence: Option<f64>,
    pub data: Vec<u16>,
    pub signature: Vec<u16>,
    pub gas_used: Option<Vec<u16>>,
    pub error: Option<Vec<u16>>,
    pub created_at: f64,
}
// ts: TransactionsResponse
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionsResponse {
    pub transactions: Vec<Transaction>,
    pub total: Option<f64>,
    pub cursor: Option<Vec<u16>>,
}
// ts: BlockTransactionsResponse
#[derive(Debug, Clone, PartialEq)]
pub struct BlockTransactionsResponse {
    pub block_number: f64,
    pub block_timestamp: f64,
    pub transactions: Vec<Transaction>,
    pub total_transactions: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepositStatus {
    Pending,
    Confirming,
    Completed,
    Failed,
}
impl DepositStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirming => "confirming",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}
// ts: Deposit
#[derive(Debug, Clone, PartialEq)]
pub struct Deposit {
    pub deposit_id: Vec<u16>,
    pub account_index: Vec<u16>,
    pub l1_address: Vec<u16>,
    pub token_id: f64,
    pub amount: Vec<u16>,
    pub l1_tx_hash: Vec<u16>,
    pub status: DepositStatus,
    pub confirmations: f64,
    pub required_confirmations: f64,
    pub created_at: f64,
    pub completed_at: Option<f64>,
}
// ts: DepositHistoryResponse
#[derive(Debug, Clone, PartialEq)]
pub struct DepositHistoryResponse {
    pub deposits: Vec<Deposit>,
    pub total: Option<f64>,
    pub cursor: Option<Vec<u16>>,
}
// ts: Transfer
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    pub transfer_id: Vec<u16>,
    pub tx_hash: Vec<u16>,
    pub from_account_index: Vec<u16>,
    pub to_account_index: Vec<u16>,
    pub token_id: f64,
    pub amount: Vec<u16>,
    pub status: TransactionStatus,
    pub created_at: f64,
}
// ts: TransferHistoryResponse
#[derive(Debug, Clone, PartialEq)]
pub struct TransferHistoryResponse {
    pub transfers: Vec<Transfer>,
    pub total: Option<f64>,
    pub cursor: Option<Vec<u16>>,
}
// ts: TransactionDetailResponse
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionDetailResponse {
    pub transaction: Transaction,
}
// ts: GetAccountTransactionsOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetAccountTransactionsOptions {
    pub limit: Option<f64>,
    pub cursor: Option<Vec<u16>>,
    pub types: Option<Vec<TransactionType>>,
    pub auth: Option<Vec<u16>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DepositFilter {
    pub status: Option<DepositStatus>,
    pub token_id: Option<f64>,
}
// ts: GetDepositHistoryOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetDepositHistoryOptions {
    pub l1_address: Option<Vec<u16>>,
    pub auth: Option<Vec<u16>>,
    pub cursor: Option<Vec<u16>>,
    pub filter: Option<DepositFilter>,
}
// ts: GetTransferHistoryOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetTransferHistoryOptions {
    pub auth: Option<Vec<u16>>,
    pub cursor: Option<Vec<u16>>,
    pub limit: Option<f64>,
}
pub enum TransactionIdentifier {
    String(Vec<u16>),
    Number(f64),
}
/// Source array/coercion operations remain explicit host dependencies. Calls
/// preserve lookup order, receiver binding, native results and original throws.
pub trait TransactionQuerySemantics {
    fn greater_than_zero(&self, value: &AccountValue) -> SigningResult<bool>;
    /// Get join then call with the original value as receiver and supplied separator.
    fn join(&self, value: &AccountValue, separator: &[u16]) -> SigningResult<AccountValue>;
}
// ts: TransactionApi
pub struct TransactionApi {
    host: AccountApi,
    semantics: Rc<dyn AccountSemantics>,
    query: Rc<dyn TransactionQuerySemantics>,
    runtime: Rc<dyn TransactionRuntime>,
}
impl TransactionApi {
    // ts: TransactionApi.constructor
    pub fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
        query: Rc<dyn TransactionQuerySemantics>,
    ) -> Self {
        Self {
            host: AccountApi::new(client, runtime.clone(), semantics.clone()),
            semantics,
            query,
            runtime,
        }
    }
    fn option(&self, opts: Option<&AccountValue>, key: &str) -> SigningResult<AccountValue> {
        match opts {
            Some(value) if !value.is_nullish() => self.semantics.get_property(value, key),
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn direct_params(
        &self,
        opts: Option<&AccountValue>,
        keys: &[&str],
    ) -> SigningResult<AccountRequestConfig> {
        let mut params = crate::http_models::Utf16Record::new();
        for key in keys {
            params.insert(key.encode_utf16().collect(), self.option(opts, key)?);
        }
        Ok(AccountRequestConfig { params })
    }
    fn path(index: &BigInt, suffix: &str) -> Vec<u16> {
        format!("/api/v1/accounts/{index}/{suffix}")
            .encode_utf16()
            .collect()
    }
    fn with_params(
        &self,
        index: &BigInt,
        suffix: &str,
        config: SigningResult<AccountRequestConfig>,
        fallback: &'static str,
    ) -> NativePromise<AccountValue> {
        match config {
            Ok(config) => self.host.request_projected(
                || Ok((Self::path(index, suffix), Some(config))),
                fallback,
                |_, data| Ok(data),
            ),
            Err(error) => NativePromise::settled(self.runtime.clone(), Err(error)),
        }
    }
    // ts: TransactionApi.getAccountTransactions
    pub fn get_account_transactions(
        &self,
        index: &BigInt,
        opts: Option<&AccountValue>,
    ) -> NativePromise<AccountValue> {
        let config = (|| {
            let mut config = self.direct_params(opts, &["limit", "cursor", "auth"])?;
            if self.option(opts, "types")?.truthy() {
                let types = self.option(opts, "types")?;
                let length = self.semantics.get_property(&types, "length")?;
                if self.query.greater_than_zero(&length)? {
                    let types = self.option(opts, "types")?;
                    config.params.insert(
                        "types".encode_utf16().collect(),
                        self.query.join(&types, &[44])?,
                    );
                }
            }
            Ok(config)
        })();
        self.with_params(
            index,
            "transactions",
            config,
            "Failed to get account transactions",
        )
    }
    // ts: TransactionApi.getBlockTransactions
    pub fn get_block_transactions(&self, block_number: f64) -> NativePromise<AccountValue> {
        self.host.request_projected(
            || {
                Ok((
                    format!(
                        "/api/v1/blocks/{}/transactions",
                        js_number_string(block_number)
                    )
                    .encode_utf16()
                    .collect(),
                    None,
                ))
            },
            "Failed to get block transactions",
            |_, data| Ok(data),
        )
    }
    // ts: TransactionApi.getDepositHistory
    pub fn get_deposit_history(
        &self,
        index: &BigInt,
        opts: Option<&AccountValue>,
    ) -> NativePromise<AccountValue> {
        let config = (|| {
            let mut config = self.direct_params(opts, &["cursor", "auth", "l1Address"])?;
            if self.option(opts, "filter")?.truthy() {
                let filter = self.option(opts, "filter")?;
                if self.semantics.get_property(&filter, "status")?.truthy() {
                    let filter = self.option(opts, "filter")?;
                    config.params.insert(
                        "status".encode_utf16().collect(),
                        self.semantics.get_property(&filter, "status")?,
                    );
                }
                let filter = self.option(opts, "filter")?;
                if !matches!(
                    self.semantics.get_property(&filter, "tokenId")?,
                    AccountValue::Undefined
                ) {
                    let filter = self.option(opts, "filter")?;
                    config.params.insert(
                        "tokenId".encode_utf16().collect(),
                        self.semantics.get_property(&filter, "tokenId")?,
                    );
                }
            }
            Ok(config)
        })();
        self.with_params(index, "deposits", config, "Failed to get deposit history")
    }
    // ts: TransactionApi.getTransferHistory
    pub fn get_transfer_history(
        &self,
        index: &BigInt,
        opts: Option<&AccountValue>,
    ) -> NativePromise<AccountValue> {
        self.with_params(
            index,
            "transfers",
            self.direct_params(opts, &["cursor", "auth", "limit"]),
            "Failed to get transfer history",
        )
    }
    // ts: TransactionApi.getTransaction
    pub fn get_transaction(
        &self,
        identifier: &TransactionIdentifier,
    ) -> NativePromise<AccountValue> {
        let identifier = match identifier {
            TransactionIdentifier::String(value) => value.clone(),
            TransactionIdentifier::Number(value) => {
                js_number_string(*value).encode_utf16().collect()
            }
        };
        self.host.request_projected(
            || {
                Ok((
                    "/api/v1/transactions/"
                        .encode_utf16()
                        .chain(identifier)
                        .collect(),
                    None,
                ))
            },
            "Failed to get transaction",
            |semantics, data| semantics.get_property(&data, "transaction"),
        )
    }
    // ts: TransactionApi.getTransactionFromL1Hash
    pub fn get_transaction_from_l1_hash(&self, hash: &[u16]) -> NativePromise<AccountValue> {
        self.host.request_projected(
            || {
                Ok((
                    "/api/v1/transactions/l1/"
                        .encode_utf16()
                        .chain(hash.iter().copied())
                        .collect(),
                    None,
                ))
            },
            "Failed to get transaction from L1 hash",
            |semantics, data| semantics.get_property(&data, "transaction"),
        )
    }
}
