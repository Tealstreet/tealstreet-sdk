//! Native Lighter transaction envelopes and ECgFp5 signing.
use safe_cex_crypto::lighter::poseidon2_hash_fp5;
#[cfg(test)]
use safe_cex_crypto::lighter::schnorr_sign;
use safe_cex_engine::signing::{
    Algorithm, Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};
use serde::{Deserialize, Serialize};
#[path = "sign/statement.rs"]
pub mod statement;
#[cfg(test)]
use zeroize::Zeroizing;

const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const MAX_ACCOUNT: u64 = (1 << 48) - 2;
const MAX_ORDER_VALUE: u64 = (1 << 48) - 1;
const MAX_ORDER_INDEX: u64 = (1 << 60) - 1;
const TX_TTL: u64 = 10 * 60 * 1_000;
const ORDER_TTL: u64 = 28 * 24 * 60 * 60 * 1_000;
/// A request is one statement commitment and one signature per transaction,
/// inside the signing request's 32-operation bound.
const MAX_BATCH: usize = 31;

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
    /// One create_grouped_orders transaction: OTO (1, parent then child), OCO (2,
    /// two children) or OTOCO (3, parent then stop loss then take profit).
    Grouped {
        grouping: u8,
        orders: Vec<Order>,
        builder: Option<Builder>,
    },
}

pub struct SignedTransaction {
    pub tx_type: u8,
    pub tx_info: String,
    /// Raw transaction fields before optional L2 attribute aggregation.
    pub data: Vec<u8>,
    pub signature: String,
}

/// The signing chain for a deployment.
pub fn chain_id(robinhood: bool, testnet: bool) -> u32 {
    match (robinhood, testnet) {
        (_, true) => 300,
        (true, false) => 466_324,
        (false, false) => 304,
    }
}

/// The public signing scope: chain, account and API key slot.
#[derive(Clone, Copy, Debug)]
pub struct Account {
    chain_id: u32,
    account_index: u64,
    api_key_index: u8,
}
impl Account {
    pub fn new(chain_id: u32, account_index: u64, api_key_index: u8) -> Result<Self, String> {
        bounded(account_index, MAX_ACCOUNT, "account index")?;
        if api_key_index > 254 {
            return Err("Lighter API key index must be in 0..=254".into());
        }
        Ok(Self {
            chain_id,
            account_index,
            api_key_index,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
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

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
struct GroupedInfo<'a> {
    account_index: u64,
    api_key_index: u8,
    grouping_type: u8,
    orders: &'a [OrderInfo],
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

#[cfg(test)]
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

    /// Every bound `new` establishes, rechecked for fields read from a statement.
    fn validate(&self) -> Result<(), String> {
        if !valid_market(self.market_index)
            || self.order_type > 5
            || self.time_in_force > 2
            || self.is_ask > 1
            || self.reduce_only > 1
        {
            return Err("Invalid Lighter order market, type or time in force".into());
        }
        bounded(
            self.client_order_index,
            MAX_ORDER_VALUE,
            "client order index",
        )?;
        bounded(self.base_amount, MAX_ORDER_VALUE, "base amount")?;
        safe_integer(self.order_expiry, "order expiry")
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

fn signing_preimage(data: &[u8], attributes: Option<&Attributes>) -> Result<Vec<u8>, String> {
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

#[derive(Clone)]
enum Fields {
    Create(OrderInfo, Option<Attributes>),
    Grouped(u8, Vec<OrderInfo>, Option<Attributes>),
    Cancel {
        market_id: u32,
        order_id: u64,
    },
    Leverage {
        market_id: u32,
        fraction: u32,
        margin_mode: u8,
    },
}

impl Fields {
    fn tx_type(&self) -> u8 {
        match self {
            Self::Create(..) => 14,
            Self::Cancel { .. } => 15,
            Self::Leverage { .. } => 20,
            Self::Grouped(..) => 28,
        }
    }
}

/// One transaction canonicalized for signing. The legacy `Signer` and
/// `TxRecipe` share it, so both sign and serialize identical bytes.
struct TxInput {
    account: Account,
    tx_type: u8,
    nonce: u64,
    expired_at: u64,
    data: Vec<u8>,
    preimage: Vec<u8>,
    fields: Fields,
}

fn tx_input(
    account: &Account,
    tx: &Transaction,
    nonce: u64,
    now_ms: u64,
) -> Result<TxInput, String> {
    let expired_at = now_ms
        .checked_add(TX_TTL)
        .ok_or("Lighter expiry overflow")?;
    let fields = match tx {
        Transaction::Create { order, builder } => {
            Fields::Create(OrderInfo::new(order, now_ms)?, attributes(*builder)?)
        }
        Transaction::Grouped {
            grouping,
            orders,
            builder,
        } => Fields::Grouped(
            *grouping,
            orders
                .iter()
                .map(|order| OrderInfo::new(order, now_ms))
                .collect::<Result<Vec<_>, _>>()?,
            attributes(*builder)?,
        ),
        Transaction::Cancel {
            market_id,
            order_id,
        } => Fields::Cancel {
            market_id: *market_id,
            order_id: *order_id,
        },
        Transaction::Leverage {
            market_id,
            leverage,
            margin_mode,
        } => {
            let fraction = (10_000.0 / leverage).floor();
            if !leverage.is_finite() || *leverage <= 0.0 || !(1.0..=10_000.0).contains(&fraction) {
                return Err("Invalid Lighter leverage or margin mode".into());
            }
            Fields::Leverage {
                market_id: *market_id,
                fraction: fraction as u32,
                margin_mode: *margin_mode,
            }
        }
    };
    encode(account, nonce, expired_at, fields)
}

/// The signed words for resolved fields, shared by local issue and by a remote
/// signer re-deriving them from a statement, so both validate and sign alike.
fn encode(
    account: &Account,
    nonce: u64,
    expired_at: u64,
    fields: Fields,
) -> Result<TxInput, String> {
    safe_integer(nonce, "transaction nonce")?;
    bounded(expired_at, MAX_ORDER_VALUE, "transaction expiry")?;
    let tx_type = fields.tx_type();
    let mut data = Vec::with_capacity(128);
    append_words(
        &mut data,
        &[
            account.chain_id.into(),
            tx_type.into(),
            nonce,
            expired_at,
            account.account_index,
            account.api_key_index.into(),
        ],
    );
    match &fields {
        Fields::Create(order, _) => {
            order.validate()?;
            order.append(&mut data);
        }
        Fields::Grouped(grouping, orders, _) => {
            let expected = match grouping {
                1 | 2 => 2,
                3 => 3,
                _ => return Err("Invalid Lighter order grouping type".into()),
            };
            if orders.len() != expected {
                return Err("Lighter grouped orders do not match their grouping type".into());
            }
            let mut words = Vec::with_capacity(orders.len() * 80);
            for order in orders {
                order.validate()?;
                order.append(&mut words);
            }
            append_words(&mut data, &[(*grouping).into()]);
            data.extend_from_slice(&safe_cex_crypto::lighter::grouped_orders_digest(&words)?);
        }
        Fields::Cancel {
            market_id,
            order_id,
        } => {
            if !valid_market(*market_id) || *order_id == 0 || *order_id > MAX_ORDER_INDEX {
                return Err("Invalid Lighter cancellation market or order index".into());
            }
            append_words(&mut data, &[(*market_id).into(), *order_id]);
        }
        Fields::Leverage {
            market_id,
            fraction,
            margin_mode,
        } => {
            if !valid_market(*market_id) || *margin_mode > 1 || !(1..=10_000).contains(fraction) {
                return Err("Invalid Lighter leverage or margin mode".into());
            }
            append_words(
                &mut data,
                &[
                    (*market_id).into(),
                    (*fraction).into(),
                    (*margin_mode).into(),
                ],
            );
        }
    }
    if let Fields::Create(_, Some(attributes)) | Fields::Grouped(_, _, Some(attributes)) = &fields {
        bounded(
            attributes.account_index,
            MAX_ACCOUNT,
            "integrator account index",
        )?;
    }
    let attrs = match &fields {
        Fields::Create(_, attributes) | Fields::Grouped(_, _, attributes) => attributes.as_ref(),
        _ => None,
    };
    let preimage = signing_preimage(&data, attrs)?;
    Ok(TxInput {
        account: *account,
        tx_type,
        nonce,
        expired_at,
        data,
        preimage,
        fields,
    })
}

impl TxInput {
    fn finish(self, signature_bytes: &[u8]) -> Result<SignedTransaction, String> {
        let Self {
            account,
            tx_type,
            nonce,
            expired_at,
            data,
            fields,
            ..
        } = self;
        let encoded = safe_cex_crypto::encode(signature_bytes, safe_cex_crypto::Encoding::Base64);
        let tx_info = match fields {
            Fields::Create(order, attributes) => serde_json::to_string(&CreateInfo {
                account_index: account.account_index,
                api_key_index: account.api_key_index,
                order: &order,
                expired_at,
                nonce,
                sig: &encoded,
                attributes,
            }),
            Fields::Grouped(grouping, orders, attributes) => serde_json::to_string(&GroupedInfo {
                account_index: account.account_index,
                api_key_index: account.api_key_index,
                grouping_type: grouping,
                orders: &orders,
                expired_at,
                nonce,
                sig: &encoded,
                attributes,
            }),
            Fields::Cancel {
                market_id,
                order_id,
            } => serde_json::to_string(&CancelInfo {
                account_index: account.account_index,
                api_key_index: account.api_key_index,
                market_index: market_id,
                index: order_id,
                expired_at,
                nonce,
                sig: &encoded,
            }),
            Fields::Leverage {
                market_id,
                fraction,
                margin_mode,
            } => serde_json::to_string(&LeverageInfo {
                account_index: account.account_index,
                api_key_index: account.api_key_index,
                market_index: market_id,
                initial_margin_fraction: fraction,
                margin_mode,
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
            signature: safe_cex_crypto::hex(signature_bytes),
        })
    }
}

/// The `sendTx` (one) or `sendTxBatch` (several) path and form body.
pub fn send_body(signed: &[SignedTransaction]) -> (&'static str, String) {
    if let [single] = signed {
        return (
            "sendTx",
            format!(
                "tx_type={}&tx_info={}",
                single.tx_type,
                safe_cex_engine::url::encode_form(&single.tx_info)
            ),
        );
    }
    let types = signed.iter().map(|s| s.tx_type).collect::<Vec<_>>();
    let infos = signed
        .iter()
        .map(|s| s.tx_info.as_str())
        .collect::<Vec<_>>();
    (
        "sendTxBatch",
        format!(
            "tx_types={}&tx_infos={}",
            safe_cex_engine::url::encode_form(&serde_json::to_string(&types).unwrap()),
            safe_cex_engine::url::encode_form(&serde_json::to_string(&infos).unwrap())
        ),
    )
}

/// Dispatch recipe for one `sendTx`/`sendTxBatch` POST. Each preparation
/// re-stamps the expiry and draws fresh entropy, but keeps the transaction
/// nonces fixed at issue; apply writes the signed form body.
pub struct TxRecipe {
    identity: Identity,
    account: Account,
    txs: Vec<(Transaction, u64)>,
    prepared: Vec<TxInput>,
    commitment: Option<[u8; 32]>,
}
impl TxRecipe {
    pub fn new(
        identity: Identity,
        account: Account,
        txs: Vec<(Transaction, u64)>,
    ) -> Result<Self, String> {
        if txs.is_empty() || txs.len() > MAX_BATCH {
            return Err("Lighter transaction batch must hold 1..=31 transactions".into());
        }
        Ok(Self {
            identity,
            account,
            txs,
            prepared: Vec::new(),
            commitment: None,
        })
    }
    fn path(&self) -> &'static str {
        if self.txs.len() == 1 {
            "/api/v1/sendTx"
        } else {
            "/api/v1/sendTxBatch"
        }
    }
}
impl SignStep for TxRecipe {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let path = self.path();
        let host = http
            .url
            .split_once("://")
            .and_then(|(_, rest)| rest.strip_suffix(path))
            .filter(|host| !host.is_empty() && !host.contains('/'))
            .ok_or(SignError::InvalidRequest)?;
        if http.method != Method::Post {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        let now_ms = stamp as u64;
        self.prepared.clear();
        self.commitment = None;
        let mut operations = Vec::with_capacity(self.txs.len() + 1);
        for (tx, nonce) in &self.txs {
            let input = tx_input(&self.account, tx, *nonce, now_ms)
                .map_err(|_| SignError::InvalidRequest)?;
            // Entropy is not secret on its own: the nonce is keyed by the scalar.
            let mut payload = Vec::with_capacity(32 + input.preimage.len());
            payload.resize(32, 0);
            dispatch.signing_bytes(&mut payload)?;
            payload.extend_from_slice(&input.preimage);
            operations.push(Operation::Sign {
                algorithm: Algorithm::SchnorrDigest,
                key: self.identity.signing_ref()?.clone(),
                payload: vec![PayloadPart::Bytes { bytes: payload }],
            });
            self.prepared.push(input);
        }
        // A remote signer reads and re-derives the statement; the commitment
        // binds what it read to these signatures.
        let statement = statement::txs(&self.account, &self.prepared);
        self.commitment = Some(safe_cex_crypto::sha256(statement.as_bytes()));
        operations.insert(
            0,
            Operation::Sha256 {
                payload: vec![PayloadPart::Bytes {
                    bytes: statement.into_bytes(),
                }],
            },
        );
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp),
                expiry_ms: Some(stamp + TX_TTL as f64),
            },
            operations,
            context: Context {
                venue: "lighter".into(),
                method: http.method.as_str().into(),
                host: host.into(),
                path: path.into(),
            },
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let prepared = std::mem::take(&mut self.prepared);
        let commitment = self.commitment.take();
        let (Some(commitment), false) = (commitment, prepared.is_empty()) else {
            return Err(SignError::InvalidRequest);
        };
        let [digest, signatures @ ..] = result.values.as_slice() else {
            return Err(SignError::InvalidResult);
        };
        if digest.as_slice() != commitment || signatures.len() != prepared.len() {
            return Err(SignError::InvalidResult);
        }
        let signed = prepared
            .into_iter()
            .zip(signatures)
            .map(|(input, signature)| {
                if signature.len() != 80 {
                    return Err(SignError::InvalidResult);
                }
                input
                    .finish(signature)
                    .map_err(|_| SignError::InvalidResult)
            })
            .collect::<Result<Vec<_>, _>>()?;
        http.body = Some(send_body(&signed).1);
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

/// The one auth-token canonicalization: `"{deadline}:{account}:{api_key}"`,
/// and its signed form zero-padded to whole 8-byte words.
fn auth_message(account: &Account, deadline_seconds: u64) -> Result<(String, Vec<u8>), String> {
    bounded(deadline_seconds, MAX_ORDER_VALUE, "authentication deadline")?;
    let message = format!(
        "{deadline_seconds}:{}:{}",
        account.account_index, account.api_key_index
    );
    let mut data = message.as_bytes().to_vec();
    data.resize(data.len().div_ceil(8) * 8, 0);
    Ok((message, data))
}

/// Where a signed auth token goes, exactly as Lighter's private reads use it.
pub enum AuthPlacement {
    /// Appended as the final `auth=` query parameter.
    Query,
    /// The `authorization` request header.
    Header,
    /// The private subscribe frame on this socket, whose own URL (the venue's
    /// `ws_url()`) names the signing context; a send has no URL of its own.
    WsSubscribe {
        socket: safe_cex_engine::SocketId,
        url: String,
        channel: String,
    },
}

/// Dispatch recipe for one auth token. Each preparation stamps a fresh
/// 60-second deadline and draws fresh entropy; apply places the token.
pub struct AuthRecipe {
    identity: Identity,
    account: Account,
    origin: String,
    placement: AuthPlacement,
    message: Option<String>,
    unsigned: Option<String>,
    commitment: Option<[u8; 32]>,
}
impl AuthRecipe {
    pub fn new(
        identity: Identity,
        account: Account,
        origin: String,
        placement: AuthPlacement,
    ) -> Self {
        Self {
            identity,
            account,
            origin,
            placement,
            message: None,
            unsigned: None,
            commitment: None,
        }
    }
    /// The outbound this recipe is bound to: an HTTP request under this
    /// origin's API, or a send on its own socket.
    fn target(&self, outbound: &Action) -> Result<(String, String, String), SignError> {
        let split = |url: &str, scheme: &str| {
            url.strip_prefix(scheme)
                .map(|rest| match rest.split_once('/') {
                    Some((host, path)) => (host.to_owned(), format!("/{path}")),
                    None => (rest.to_owned(), String::new()),
                })
                .filter(|(host, _)| !host.is_empty())
                .ok_or(SignError::InvalidRequest)
        };
        match (&self.placement, outbound) {
            (AuthPlacement::Query | AuthPlacement::Header, Action::Http(http)) => {
                let (host, _) = split(&self.origin, "https://")?;
                let path = http
                    .url
                    .strip_prefix(self.origin.as_str())
                    .filter(|path| path.starts_with("/api/v1/"))
                    .ok_or(SignError::InvalidRequest)?;
                let path = path.split_once('?').map_or(path, |(path, _)| path);
                Ok((http.method.as_str().into(), host, path.into()))
            }
            (
                AuthPlacement::WsSubscribe { socket, url, .. },
                Action::WsSend { socket: actual, .. },
            ) if socket == actual => {
                let (host, path) = split(url, "wss://")?;
                Ok(("WS".into(), host, path))
            }
            _ => Err(SignError::InvalidRequest),
        }
    }
    /// The unsigned outbound a preparation signed for; apply refuses any other.
    fn unsigned(outbound: &Action) -> String {
        match outbound {
            Action::Http(http) => format!(
                "{}\n{}\n{:?}\n{:?}",
                http.method.as_str(),
                http.url,
                http.headers,
                http.body
            ),
            Action::WsSend { text, .. } => text.clone(),
            _ => String::new(),
        }
    }
}
impl SignStep for AuthRecipe {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let (method, host, path) = self.target(outbound)?;
        if let Action::Http(http) = outbound {
            match self.placement {
                // An exact `auth` parameter name, never a substring match.
                AuthPlacement::Query
                    if http.url.split_once('?').is_some_and(|(_, query)| {
                        query
                            .split('&')
                            .any(|pair| pair.split('=').next() == Some("auth"))
                    }) =>
                {
                    return Err(SignError::InvalidRequest);
                }
                AuthPlacement::Header
                    if http
                        .headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("authorization")) =>
                {
                    return Err(SignError::InvalidRequest);
                }
                _ => {}
            }
        }
        let stamp = dispatch.stamp_ms();
        let deadline = (stamp / 1000.).floor() as u64 + 60;
        let (message, data) =
            auth_message(&self.account, deadline).map_err(|_| SignError::InvalidRequest)?;
        let mut payload = Vec::with_capacity(32 + data.len());
        payload.resize(32, 0);
        dispatch.signing_bytes(&mut payload)?;
        payload.extend_from_slice(&data);
        self.message = Some(message);
        self.unsigned = Some(Self::unsigned(outbound));
        let statement = statement::auth(&self.account, deadline);
        self.commitment = Some(safe_cex_crypto::sha256(statement.as_bytes()));
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp),
                expiry_ms: Some(deadline as f64 * 1000.),
            },
            operations: vec![
                Operation::Sha256 {
                    payload: vec![PayloadPart::Bytes {
                        bytes: statement.into_bytes(),
                    }],
                },
                Operation::Sign {
                    algorithm: Algorithm::SchnorrDigest,
                    key: self.identity.signing_ref()?.clone(),
                    payload: vec![PayloadPart::Bytes { bytes: payload }],
                },
            ],
            context: Context {
                venue: "lighter".into(),
                method,
                host,
                path,
            },
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.target(outbound)?;
        let message = self.message.take().ok_or(SignError::InvalidResult)?;
        let commitment = self.commitment.take().ok_or(SignError::InvalidResult)?;
        if self.unsigned.take().as_deref() != Some(Self::unsigned(outbound).as_str()) {
            return Err(SignError::InvalidRequest);
        }
        let [digest, signature] = result.values.as_slice() else {
            return Err(SignError::InvalidResult);
        };
        if digest.as_slice() != commitment || signature.len() != 80 {
            return Err(SignError::InvalidResult);
        }
        let token = format!("{message}:{}", safe_cex_crypto::hex(signature));
        match (&self.placement, outbound) {
            (AuthPlacement::Query, Action::Http(http)) => {
                let separator = if http.url.contains('?') { '&' } else { '?' };
                http.url = format!("{}{separator}auth={token}", http.url);
            }
            (AuthPlacement::Header, Action::Http(http)) => {
                http.headers.push(("authorization", token));
            }
            (AuthPlacement::WsSubscribe { channel, .. }, Action::WsSend { text, .. }) => {
                #[derive(Serialize)]
                struct Subscribe<'a> {
                    r#type: &'a str,
                    channel: &'a str,
                    auth: &'a str,
                }
                *text = serde_json::to_string(&Subscribe {
                    r#type: "subscribe",
                    channel,
                    auth: &token,
                })
                .map_err(|_| SignError::InvalidResult)?;
            }
            _ => return Err(SignError::InvalidRequest),
        }
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

/// A tier POST has the native form header before the recipe binds its unsigned bytes.
pub(crate) struct FormRecipe<T>(pub T);
impl<T: SignStep> SignStep for FormRecipe<T> {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if let Action::Http(http) = outbound
            && http.method == Method::Post
        {
            http.headers
                .retain(|(name, _)| !name.eq_ignore_ascii_case("content-type"));
            http.headers
                .push(("Content-Type", "application/x-www-form-urlencoded".into()));
        }
        self.0.prepare(outbound, dispatch)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.0.apply(outbound, result)
    }
    fn freshness_ms(&self) -> f64 {
        self.0.freshness_ms()
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

#[cfg(test)]
mod grouped_shape_tests {
    use super::*;

    fn leg(order_id: u64, order_type: u8) -> Order {
        // Protection legs are always reduce-only in our construction (trading.rs).
        Order {
            order_id,
            market_id: 1,
            is_buy: order_type == 0,
            price: 310_000,
            amount: if order_type == 0 { 1_000 } else { 0 },
            order_type,
            time_in_force: if order_type == 0 { 1 } else { 0 },
            reduce_only: order_type != 0,
            trigger_price: if order_type == 0 { 0 } else { 300_000 },
            order_expiry: None,
        }
    }

    fn grouped(grouping: u8, orders: Vec<Order>) -> Result<TxInput, String> {
        let account = Account::new(304, 42, 3).unwrap();
        let tx = Transaction::Grouped {
            grouping,
            orders,
            builder: None,
        };
        tx_input(&account, &tx, 7, 1_700_000_000_000)
    }

    #[test]
    fn grouped_shapes_outside_oto_oco_otoco_are_refused_before_signing() {
        assert!(
            grouped(2, vec![leg(1, 0), leg(2, 2), leg(3, 4)]).is_err(),
            "OCO carries two orders"
        );
        assert!(
            grouped(4, vec![leg(1, 0), leg(2, 2)]).is_err(),
            "grouping 4 is undefined"
        );
        assert!(
            grouped(0, vec![leg(1, 0), leg(2, 2)]).is_err(),
            "grouping 0 is undefined"
        );
        assert!(
            grouped(1, vec![leg(1, 0), leg(2, 2)]).is_ok(),
            "control: OTO"
        );
        assert!(
            grouped(3, vec![leg(1, 0), leg(2, 2), leg(3, 4)]).is_ok(),
            "control: OTOCO"
        );
    }
}
