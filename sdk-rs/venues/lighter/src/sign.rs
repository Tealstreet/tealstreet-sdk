//! Native Lighter transaction envelopes and ECgFp5 signing.
use safe_cex_crypto::lighter::{SchnorrNonce, poseidon2_hash_fp5, schnorr_sign};
use serde::Serialize;
use zeroize::{Zeroize, Zeroizing};

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const MAX_ACCOUNT: u64 = (1 << 48) - 2;
const MAX_ORDER_VALUE: u64 = (1 << 48) - 1;
const MAX_ORDER_INDEX: u64 = (1 << 60) - 1;
const TX_TTL: u64 = 10 * 60 * 1_000;
const ORDER_TTL: u64 = 28 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Debug)]
pub struct Order {
    pub order_id: u64,
    pub market_id: u32,
    pub is_buy: bool,
    pub price: u32,
    pub amount: u64,
    pub order_type: u8,
    pub time_in_force: u8,
    pub reduce_only: bool,
    pub trigger_price: u32,
    pub order_expiry: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct Builder {
    pub account_index: u64,
    pub maker_fee: u32,
    pub taker_fee: u32,
}

#[derive(Clone, Debug)]
pub enum Transaction {
    Create {
        order: Order,
        builder: Option<Builder>,
    },
    Cancel {
        market_id: u32,
        order_id: u64,
    },
    Leverage {
        market_id: u32,
        leverage: f64,
        margin_mode: u8,
    },
}

pub struct SignedTransaction {
    pub tx_type: u8,
    pub tx_info: String,
    /// Raw transaction fields before optional L2 attribute aggregation.
    pub data: Vec<u8>,
    pub signature: String,
}

pub struct Signer {
    key: Zeroizing<[u8; 40]>,
    chain_id: u32,
    account_index: u64,
    api_key_index: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct OrderInfo {
    market_index: u32,
    client_order_index: u64,
    base_amount: u64,
    price: u32,
    is_ask: u8,
    #[serde(rename = "Type")]
    order_type: u8,
    time_in_force: u8,
    reduce_only: u8,
    trigger_price: u32,
    order_expiry: u64,
}

#[derive(Serialize)]
struct Attributes {
    #[serde(rename = "1")]
    account_index: u64,
    #[serde(rename = "2")]
    taker_fee: u32,
    #[serde(rename = "3")]
    maker_fee: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CreateInfo<'a> {
    account_index: u64,
    api_key_index: u8,
    #[serde(flatten)]
    order: &'a OrderInfo,
    expired_at: u64,
    nonce: u64,
    sig: &'a str,
    #[serde(rename = "L2TxAttributes", skip_serializing_if = "Option::is_none")]
    attributes: Option<Attributes>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CancelInfo<'a> {
    account_index: u64,
    api_key_index: u8,
    market_index: u32,
    index: u64,
    expired_at: u64,
    nonce: u64,
    sig: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LeverageInfo<'a> {
    account_index: u64,
    api_key_index: u8,
    market_index: u32,
    initial_margin_fraction: u32,
    margin_mode: u8,
    expired_at: u64,
    nonce: u64,
    sig: &'a str,
}

fn valid_market(market: u32) -> bool {
    market <= 32_767 && market != 255
}

fn safe_integer(value: u64, name: &str) -> Result<(), String> {
    bounded(value, MAX_SAFE_INTEGER, name)
}
fn bounded(value: u64, maximum: u64, name: &str) -> Result<(), String> {
    if value > maximum {
        return Err(format!("{name} exceeds the supported integer range"));
    }
    Ok(())
}

fn append_words(bytes: &mut Vec<u8>, words: &[u64]) {
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
}

fn hedged_sign(input: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    hedged_sign_with_entropy(input, key, |bytes| {
        getrandom::getrandom(bytes).map_err(|_| ())
    })
}

fn hedged_sign_with_entropy(
    input: &[u8],
    key: &[u8],
    fill: impl FnOnce(&mut [u8]) -> Result<(), ()>,
) -> Result<Vec<u8>, String> {
    let mut entropy = Zeroizing::new([0u8; 32]);
    fill(entropy.as_mut()).map_err(|_| "Lighter entropy unavailable")?;
    let nonce = safe_cex_crypto::lighter::hedged_nonce(input, key, &entropy)?;
    schnorr_sign(input, key, nonce)
}

impl OrderInfo {
    fn new(order: &Order, now_ms: u64) -> Result<Self, String> {
        if !valid_market(order.market_id) || order.order_type > 5 || order.time_in_force > 2 {
            return Err("Invalid Lighter order market, type or time in force".into());
        }
        bounded(order.order_id, MAX_ORDER_VALUE, "client order index")?;
        bounded(order.amount, MAX_ORDER_VALUE, "base amount")?;
        let order_expiry =
            order
                .order_expiry
                .unwrap_or(if order.time_in_force == 0 && order.order_type < 2 {
                    0
                } else {
                    now_ms
                        .checked_add(ORDER_TTL)
                        .ok_or("Lighter order expiry overflow")?
                });
        safe_integer(order_expiry, "order expiry")?;
        Ok(Self {
            market_index: order.market_id,
            client_order_index: order.order_id,
            base_amount: order.amount,
            price: order.price,
            is_ask: u8::from(!order.is_buy),
            order_type: order.order_type,
            time_in_force: order.time_in_force,
            reduce_only: u8::from(order.reduce_only),
            trigger_price: order.trigger_price,
            order_expiry,
        })
    }

    fn append(&self, bytes: &mut Vec<u8>) {
        append_words(
            bytes,
            &[
                self.market_index.into(),
                self.client_order_index,
                self.base_amount,
                self.price.into(),
                self.is_ask.into(),
                self.order_type.into(),
                self.time_in_force.into(),
                self.reduce_only.into(),
                self.trigger_price.into(),
                self.order_expiry,
            ],
        );
    }
}

fn attributes(builder: Option<Builder>) -> Result<Option<Attributes>, String> {
    builder
        .map(|builder| {
            bounded(
                builder.account_index,
                MAX_ACCOUNT,
                "integrator account index",
            )?;
            Ok(Attributes {
                account_index: builder.account_index,
                taker_fee: builder.taker_fee,
                maker_fee: builder.maker_fee,
            })
        })
        .transpose()
}

fn signing_preimage(data: &[u8], attributes: &Option<Attributes>) -> Result<Vec<u8>, String> {
    let Some(attributes) = attributes else {
        return Ok(data.to_vec());
    };
    let mut words = Vec::with_capacity(64);
    for (kind, value) in [
        (1, attributes.account_index),
        (2, attributes.taker_fee.into()),
        (3, attributes.maker_fee.into()),
    ] {
        if value != 0 {
            append_words(&mut words, &[kind, value]);
        }
    }
    words.resize(64, 0);
    let mut preimage = Vec::with_capacity(80);
    preimage.extend_from_slice(&poseidon2_hash_fp5(data)?);
    preimage.extend_from_slice(&poseidon2_hash_fp5(&words)?);
    Ok(preimage)
}

impl Signer {
    pub fn new(
        secret: &str,
        chain_id: u32,
        account_index: u64,
        api_key_index: u8,
    ) -> Result<Self, String> {
        bounded(account_index, MAX_ACCOUNT, "account index")?;
        if api_key_index > 254 {
            return Err("Lighter API key index must be in 0..=254".into());
        }
        let secret = secret.strip_prefix("0x").unwrap_or(secret);
        if secret.len() != 80 {
            return Err("Lighter private key must contain exactly 40 bytes".into());
        }
        let mut key = Zeroizing::new([0u8; 40]);
        for (byte, pair) in key.iter_mut().zip(secret.as_bytes().as_chunks::<2>().0) {
            let pair = std::str::from_utf8(pair).map_err(|_| "Invalid Lighter private key hex")?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| "Invalid Lighter private key hex")?;
        }
        // Scalar reduction belongs to the shared crypto primitive, as in the venue signer.
        if safe_cex_crypto::lighter::public_key(key.as_ref())? == [0; 40] {
            key.zeroize();
            return Err("Lighter private key must not reduce to zero".into());
        }
        Ok(Self {
            key,
            chain_id,
            account_index,
            api_key_index,
        })
    }

    pub fn auth_token(&self, deadline_seconds: u64) -> Result<String, String> {
        self.auth_token_inner(deadline_seconds, None)
    }

    /// The supplied nonce must come from fresh CSPRNG entropy and must never be reused.
    pub fn auth_token_with_nonce(
        &self,
        deadline_seconds: u64,
        schnorr_nonce: SchnorrNonce,
    ) -> Result<String, String> {
        self.auth_token_inner(deadline_seconds, Some(schnorr_nonce))
    }

    fn auth_token_inner(
        &self,
        deadline_seconds: u64,
        schnorr_nonce: Option<SchnorrNonce>,
    ) -> Result<String, String> {
        bounded(deadline_seconds, MAX_ORDER_VALUE, "authentication deadline")?;
        let message = format!(
            "{deadline_seconds}:{}:{}",
            self.account_index, self.api_key_index
        );
        let mut data = message.as_bytes().to_vec();
        data.resize(data.len().div_ceil(8) * 8, 0);
        let signature = match schnorr_nonce {
            Some(nonce) => schnorr_sign(&data, self.key.as_ref(), nonce)?,
            None => hedged_sign(&data, self.key.as_ref())?,
        };
        Ok(format!("{message}:{}", safe_cex_crypto::hex(&signature)))
    }

    pub fn sign(
        &self,
        tx: &Transaction,
        nonce: u64,
        now_ms: u64,
    ) -> Result<SignedTransaction, String> {
        self.sign_inner(tx, nonce, now_ms, None)
    }

    /// The supplied nonce must come from fresh CSPRNG entropy and must never be reused.
    pub fn sign_with_nonce(
        &self,
        tx: &Transaction,
        nonce: u64,
        now_ms: u64,
        schnorr_nonce: SchnorrNonce,
    ) -> Result<SignedTransaction, String> {
        self.sign_inner(tx, nonce, now_ms, Some(schnorr_nonce))
    }

    fn sign_inner(
        &self,
        tx: &Transaction,
        nonce: u64,
        now_ms: u64,
        schnorr_nonce: Option<SchnorrNonce>,
    ) -> Result<SignedTransaction, String> {
        safe_integer(nonce, "transaction nonce")?;
        let expired_at = now_ms
            .checked_add(TX_TTL)
            .ok_or("Lighter expiry overflow")?;
        bounded(expired_at, MAX_ORDER_VALUE, "transaction expiry")?;
        let tx_type = match tx {
            Transaction::Create { .. } => 14,
            Transaction::Cancel { .. } => 15,
            Transaction::Leverage { .. } => 20,
        };
        let mut data = Vec::with_capacity(128);
        append_words(
            &mut data,
            &[
                self.chain_id.into(),
                tx_type.into(),
                nonce,
                expired_at,
                self.account_index,
                self.api_key_index.into(),
            ],
        );
        let (orders, attrs, initial_margin_fraction) = match tx {
            Transaction::Create { order, builder } => {
                let order = OrderInfo::new(order, now_ms)?;
                order.append(&mut data);
                (vec![order], attributes(*builder)?, 0)
            }
            Transaction::Cancel {
                market_id,
                order_id,
            } => {
                if !valid_market(*market_id) || *order_id == 0 || *order_id > MAX_ORDER_INDEX {
                    return Err("Invalid Lighter cancellation market or order index".into());
                }
                append_words(&mut data, &[(*market_id).into(), *order_id]);
                (Vec::new(), None, 0)
            }
            Transaction::Leverage {
                market_id,
                leverage,
                margin_mode,
            } => {
                let fraction = (10_000.0 / leverage).floor();
                if !valid_market(*market_id)
                    || !leverage.is_finite()
                    || *leverage <= 0.0
                    || *margin_mode > 1
                    || !(1.0..=10_000.0).contains(&fraction)
                {
                    return Err("Invalid Lighter leverage or margin mode".into());
                }
                let fraction = fraction as u32;
                append_words(
                    &mut data,
                    &[(*market_id).into(), fraction.into(), (*margin_mode).into()],
                );
                (Vec::new(), None, fraction)
            }
        };
        let preimage = signing_preimage(&data, &attrs)?;
        let signature_bytes = match schnorr_nonce {
            Some(nonce) => schnorr_sign(&preimage, self.key.as_ref(), nonce)?,
            None => hedged_sign(&preimage, self.key.as_ref())?,
        };
        let signature = safe_cex_crypto::hex(&signature_bytes);
        let encoded = safe_cex_crypto::encode(&signature_bytes, safe_cex_crypto::Encoding::Base64);
        let tx_info = match tx {
            Transaction::Create { .. } => serde_json::to_string(&CreateInfo {
                account_index: self.account_index,
                api_key_index: self.api_key_index,
                order: &orders[0],
                expired_at,
                nonce,
                sig: &encoded,
                attributes: attrs,
            }),
            Transaction::Cancel {
                market_id,
                order_id,
            } => serde_json::to_string(&CancelInfo {
                account_index: self.account_index,
                api_key_index: self.api_key_index,
                market_index: *market_id,
                index: *order_id,
                expired_at,
                nonce,
                sig: &encoded,
            }),
            Transaction::Leverage {
                market_id,
                margin_mode,
                ..
            } => serde_json::to_string(&LeverageInfo {
                account_index: self.account_index,
                api_key_index: self.api_key_index,
                market_index: *market_id,
                initial_margin_fraction,
                margin_mode: *margin_mode,
                expired_at,
                nonce,
                sig: &encoded,
            }),
        }
        .map_err(|e| e.to_string())?;
        Ok(SignedTransaction {
            tx_type,
            tx_info,
            data,
            signature,
        })
    }
}

#[cfg(test)]
mod security_hedging {
    use super::*;
    #[test]
    fn entropy_failure_never_produces_a_signature() {
        let error = hedged_sign_with_entropy(&[1; 8], &[3; 40], |bytes| {
            bytes.fill(7);
            Err(())
        })
        .unwrap_err();
        assert_eq!(error, "Lighter entropy unavailable");
    }
    #[test]
    fn repeated_entropy_remains_safe_at_the_production_signing_boundary() {
        let sign = |input: &[u8]| {
            hedged_sign_with_entropy(input, &[3; 40], |bytes| {
                bytes.fill(0);
                Ok(())
            })
            .unwrap()
        };
        let first = sign(&[1; 8]);
        let second = sign(&[2; 8]);
        assert_ne!(first, second);
        let public = safe_cex_crypto::lighter::public_key(&[3; 40]).unwrap();
        assert!(safe_cex_crypto::lighter::schnorr_verify(&[1; 8], &public, &first).unwrap());
        assert!(safe_cex_crypto::lighter::schnorr_verify(&[2; 8], &public, &second).unwrap());
    }
}
