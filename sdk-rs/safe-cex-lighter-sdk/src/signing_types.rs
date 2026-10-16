//! Value-level signer declarations retain UTF-16, BigInt and binary64 fields.
use num_bigint::BigInt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionSignature {
    pub tx_hash: Vec<u16>,
    pub signature: Vec<u16>,
}
impl TransactionSignature {
    pub fn new(tx_hash: Vec<u16>, signature: Vec<u16>) -> Self {
        Self { tx_hash, signature }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct AuthToken {
    pub token: Vec<u16>,
    pub message: Vec<u16>,
    pub message_hash: Vec<u16>,
    pub account_index: f64,
    pub api_key_index: f64,
    pub deadline: BigInt,
}
impl AuthToken {
    pub fn new(
        token: Vec<u16>,
        message: Vec<u16>,
        message_hash: Vec<u16>,
        account_index: f64,
        api_key_index: f64,
        deadline: BigInt,
    ) -> Self {
        Self {
            token,
            message,
            message_hash,
            account_index,
            api_key_index,
            deadline,
        }
    }
}
#[derive(Debug, Clone)]
pub struct SignerError<C> {
    pub name: &'static str,
    pub message: Vec<u16>,
    pub cause: Option<C>,
}
impl<C> SignerError<C> {
    pub fn new(message: Vec<u16>, cause: Option<C>) -> Self {
        Self {
            name: "SignerError",
            message,
            cause,
        }
    }
}
