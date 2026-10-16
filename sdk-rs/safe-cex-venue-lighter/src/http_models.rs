//! Value-level HTTP records. Strings and record keys retain UTF16 code units;
//! generic payloads stay native host values. No JSON codec or HTTP client is implied.
use indexmap::IndexMap;
use num_bigint::BigInt;
pub type Utf16Record<T> = IndexMap<Vec<u16>, T>;
// ts: LighterApiClientConfig
#[derive(Debug, Clone, PartialEq)]
pub struct LighterApiClientConfig {
    pub base_url: Vec<u16>,
    pub timeout: Option<f64>,
    pub headers: Option<Utf16Record<Vec<u16>>>,
    pub debug: Option<bool>,
}
// ts: ApiKey
#[derive(Debug, Clone, PartialEq)]
pub struct ApiKey {
    pub index: f64,
    pub public_key: Vec<u16>,
    pub active: bool,
    pub created_at: Option<f64>,
    pub last_used_at: Option<f64>,
}
// ts: TransferFeeInfo
#[derive(Debug, Clone, PartialEq)]
pub struct TransferFeeInfo {
    pub fee: BigInt,
    pub min_amount: BigInt,
    pub max_amount: BigInt,
    pub fee_percentage: Option<f64>,
}
// ts: ApiErrorResponse
#[derive(Debug, Clone, PartialEq)]
pub struct ApiErrorResponse<V> {
    pub code: Vec<u16>,
    pub message: Vec<u16>,
    pub details: Option<Utf16Record<V>>,
    pub request_id: Option<Vec<u16>>,
}
// ts: ApiResponse
#[derive(Debug, Clone, PartialEq)]
pub struct ApiResponse<T, V> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiErrorResponse<V>>,
    pub timestamp: Option<f64>,
}
// ts: GetNonceResponse
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetNonceResponse {
    pub nonce: Vec<u16>,
    pub next_nonce: Vec<u16>,
}
// ts: GetApiKeysResponse
#[derive(Debug, Clone, PartialEq)]
pub struct GetApiKeysResponse {
    pub api_keys: Vec<ApiKey>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendTxStatus {
    Pending,
    Confirmed,
    Failed,
}
// ts: SendTxResponse
#[derive(Debug, Clone, PartialEq)]
pub struct SendTxResponse {
    pub tx_hash: Vec<u16>,
    pub status: SendTxStatus,
    pub block_number: Option<f64>,
}
// ts: GetTransferFeeResponse
#[derive(Debug, Clone, PartialEq)]
pub struct GetTransferFeeResponse {
    pub fee: Vec<u16>,
    pub min_amount: Vec<u16>,
    pub max_amount: Vec<u16>,
    pub fee_percentage: Option<f64>,
}
