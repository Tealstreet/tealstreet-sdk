//! Hosted (mode 2) Paradex signing: the closed descriptor a device rebuilds,
//! checks and hashes before it signs with its own key. Nothing here holds a key
//! or reads a clock. See docs/plans/RUST_PARADEX_HOSTED_STARK_CONSUMER.md.

use serde::{Deserialize, Deserializer, de::Error as _};
use starknet_crypto::Felt;

use crate::sign::{
    AUTH_TYPE, auth_values, chain_id, felt, order_values, quantum, typed_hash, word,
};

/// Native `/v1/orders/batch` accepts at most this many orders.
pub const MAX_BATCH: usize = 10;
const MAX_SAFE: u64 = (1 << 53) - 1;
const AUTH_LIFETIME_S: u64 = 300;
const AUTH_SKEW_S: u64 = 30;
const ORDER_SKEW_MS: u64 = 30_000;

/// Closed, sanitized refusal codes; none carries request content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorError {
    /// Not a well-formed descriptor: unknown, missing or ill-typed fields.
    Malformed,
    /// Well-formed fields in a combination native never emits.
    Presence,
    /// Peer time or deadline is not finite and nonnegative.
    Time,
    /// The original signing deadline has passed.
    Deadline,
    /// A signed timestamp is outside the reviewed skew window.
    Skew,
    /// The trusted binding is invalid.
    Binding,
}

impl DescriptorError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::Presence => "presence",
            Self::Time => "time",
            Self::Deadline => "deadline",
            Self::Skew => "skew",
            Self::Binding => "binding",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Side {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

/// Types native placement emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    Limit,
    Market,
    StopLimit,
    StopMarket,
    TakeProfitMarket,
}

/// Types native amendment admits: a LIMIT, or an untriggered `is_trigger` kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModifyOrderType {
    Limit,
    StopLimit,
    StopMarket,
    StopLossLimit,
    StopLossMarket,
    TakeProfitLimit,
    TakeProfitMarket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Instruction {
    Gtc,
    Ioc,
    PostOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Flag {
    #[serde(rename = "REDUCE_ONLY")]
    ReduceOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Stp {
    #[serde(rename = "EXPIRE_TAKER")]
    ExpireTaker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateTarget {
    Orders,
    OrdersBatchSingle,
}

/// One order exactly as native placement projects it into its body.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Order {
    #[serde(deserialize_with = "safe_int")]
    timestamp_ms: u64,
    #[serde(deserialize_with = "market")]
    market: String,
    side: Side,
    order_type: OrderType,
    #[serde(deserialize_with = "decimal")]
    size: String,
    #[serde(deserialize_with = "nullable_decimal")]
    price: Option<String>,
    #[serde(deserialize_with = "nullable_decimal")]
    trigger_price: Option<String>,
    instruction: Instruction,
    #[serde(deserialize_with = "present")]
    flags: Option<[Flag; 1]>,
    #[serde(deserialize_with = "present")]
    stp: Option<Stp>,
    #[serde(deserialize_with = "client_id")]
    client_id: Option<String>,
}

/// One amendment exactly as native projects it into its body.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Modify {
    id: String,
    timestamp_ms: u64,
    market: String,
    side: Side,
    order_type: ModifyOrderType,
    size: String,
    price: String,
    trigger_price: Option<String>,
}

/// Built only by `from_json`; every value it holds passes `check`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParadexDescriptor(Kind);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Auth { timestamp_s: u64, expiration_s: u64 },
    OrderCreate { target: CreateTarget, order: Order },
    OrderBatch { items: Vec<Order> },
    OrderModify(Modify),
}

/// The trusted account and network a device signs for, from its own setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParadexBinding {
    account: Felt,
    chain_id: Felt,
}

impl ParadexBinding {
    pub fn new(account: &str, starknet_chain_id: &str) -> Result<Self, DescriptorError> {
        let account = felt(account).map_err(|_| DescriptorError::Binding)?;
        if account == Felt::ZERO {
            return Err(DescriptorError::Binding);
        }
        let chain_id = chain_id(starknet_chain_id).map_err(|_| DescriptorError::Binding)?;
        Ok(Self { account, chain_id })
    }
}

/// One digest per signature: exactly one, or one per batch item in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digests(Vec<[u8; 32]>);

impl Digests {
    pub fn as_slice(&self) -> &[[u8; 32]] {
        &self.0
    }
}

#[derive(Deserialize)]
enum Family {
    #[serde(rename = "paradex")]
    Paradex,
}

#[derive(Deserialize)]
enum Revision {
    #[serde(rename = "paradex/1")]
    V1,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthWire {
    #[allow(dead_code)]
    family: Family,
    #[allow(dead_code)]
    op: String,
    #[allow(dead_code)]
    revision: Revision,
    #[serde(deserialize_with = "safe_int")]
    timestamp_s: u64,
    #[serde(deserialize_with = "safe_int")]
    expiration_s: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateWire {
    #[allow(dead_code)]
    family: Family,
    #[allow(dead_code)]
    op: String,
    #[allow(dead_code)]
    revision: Revision,
    target: CreateTarget,
    order: Order,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchWire {
    #[allow(dead_code)]
    family: Family,
    #[allow(dead_code)]
    op: String,
    #[allow(dead_code)]
    revision: Revision,
    items: Vec<Order>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModifyWire {
    #[allow(dead_code)]
    family: Family,
    #[allow(dead_code)]
    op: String,
    #[allow(dead_code)]
    revision: Revision,
    #[serde(deserialize_with = "market")]
    id: String,
    #[serde(deserialize_with = "safe_int")]
    timestamp_ms: u64,
    #[serde(deserialize_with = "market")]
    market: String,
    side: Side,
    order_type: ModifyOrderType,
    #[serde(deserialize_with = "decimal")]
    size: String,
    #[serde(deserialize_with = "decimal")]
    price: String,
    #[serde(deserialize_with = "nullable_decimal")]
    trigger_price: Option<String>,
}

impl ParadexDescriptor {
    /// Parse and check one descriptor. Every accepted value obeys the native
    /// presence table; anything else refuses.
    pub fn from_json(json: &str) -> Result<Self, DescriptorError> {
        // Read only the tag first; each closed wire struct then parses the
        // whole object and refuses unknown fields.
        #[derive(Deserialize)]
        struct Tag {
            op: String,
        }
        let tag: Tag = serde_json::from_str(json).map_err(|_| DescriptorError::Malformed)?;
        let malformed = |_| DescriptorError::Malformed;
        let kind = match tag.op.as_str() {
            "auth" => {
                let wire: AuthWire = serde_json::from_str(json).map_err(malformed)?;
                Kind::Auth {
                    timestamp_s: wire.timestamp_s,
                    expiration_s: wire.expiration_s,
                }
            }
            "orderCreate" => {
                let wire: CreateWire = serde_json::from_str(json).map_err(malformed)?;
                Kind::OrderCreate {
                    target: wire.target,
                    order: wire.order,
                }
            }
            "orderBatch" => {
                let wire: BatchWire = serde_json::from_str(json).map_err(malformed)?;
                Kind::OrderBatch { items: wire.items }
            }
            "orderModify" => {
                let wire: ModifyWire = serde_json::from_str(json).map_err(malformed)?;
                Kind::OrderModify(Modify {
                    id: wire.id,
                    timestamp_ms: wire.timestamp_ms,
                    market: wire.market,
                    side: wire.side,
                    order_type: wire.order_type,
                    size: wire.size,
                    price: wire.price,
                    trigger_price: wire.trigger_price,
                })
            }
            _ => return Err(DescriptorError::Malformed),
        };
        let descriptor = Self(kind);
        descriptor.check()?;
        Ok(descriptor)
    }

    /// The native presence table (trading.rs `prepare`, `native_batch` and the
    /// amendment builder). Re-checked by `digests`, so a constructed value
    /// cannot bypass it.
    fn check(&self) -> Result<(), DescriptorError> {
        match &self.0 {
            Kind::Auth {
                timestamp_s,
                expiration_s,
            } => {
                if *timestamp_s > MAX_SAFE {
                    return Err(DescriptorError::Malformed);
                }
                if timestamp_s.checked_add(AUTH_LIFETIME_S) != Some(*expiration_s)
                    || *expiration_s > MAX_SAFE
                {
                    return Err(DescriptorError::Presence);
                }
            }
            Kind::OrderCreate { target, order } => {
                check_order(order)?;
                if *target == CreateTarget::OrdersBatchSingle {
                    check_batch_item(order)?;
                }
            }
            Kind::OrderBatch { items } => {
                if items.is_empty() || items.len() > MAX_BATCH {
                    return Err(DescriptorError::Presence);
                }
                let mut ids = std::collections::HashSet::new();
                for item in items {
                    check_order(item)?;
                    check_batch_item(item)?;
                    if !ids.insert(item.client_id.as_deref()) {
                        return Err(DescriptorError::Presence);
                    }
                }
            }
            Kind::OrderModify(modify) => {
                check_modify_values(modify)?;
                let trigger = !matches!(modify.order_type, ModifyOrderType::Limit);
                if modify.trigger_price.is_some() != trigger
                    || (!trigger && !positive(&modify.price))
                    || !positive(&modify.size)
                    || modify
                        .trigger_price
                        .as_deref()
                        .is_some_and(|p| !positive(p))
                {
                    return Err(DescriptorError::Presence);
                }
            }
        }
        Ok(())
    }
}

fn check_order(order: &Order) -> Result<(), DescriptorError> {
    use OrderType::*;
    let values = order.timestamp_ms <= MAX_SAFE
        && native_word(&order.market)
        && canonical(&order.size)
        && order.price.as_deref().is_none_or(canonical)
        && order.trigger_price.as_deref().is_none_or(canonical)
        && order.client_id.as_deref().is_none_or(client_id_ok);
    if !values {
        return Err(DescriptorError::Malformed);
    }
    let present = |field: &Option<String>| field.as_deref().is_none_or(positive);
    let ok = positive(&order.size)
        && present(&order.price)
        && present(&order.trigger_price)
        && order.price.is_some() == matches!(order.order_type, Limit | StopLimit)
        && match order.order_type {
            StopLimit | StopMarket | TakeProfitMarket => order.trigger_price.is_some(),
            Limit | Market => true,
        }
        && match order.order_type {
            StopMarket | TakeProfitMarket => order.flags.is_some(),
            StopLimit => order.flags.is_none(),
            Limit | Market => true,
        }
        && order.stp.is_some() == order.client_id.is_some();
    ok.then_some(()).ok_or(DescriptorError::Presence)
}

/// Native `native_batch` eligibility visible on the wire. Original FOK, close
/// and attached legs are origin-only facts, enforced by native admission.
/// The value rules `from_json` applies, re-applied to every held value.
fn check_modify_values(modify: &Modify) -> Result<(), DescriptorError> {
    (modify.timestamp_ms <= MAX_SAFE
        && native_word(&modify.id)
        && native_word(&modify.market)
        && canonical(&modify.size)
        && canonical(&modify.price)
        && modify.trigger_price.as_deref().is_none_or(canonical))
    .then_some(())
    .ok_or(DescriptorError::Malformed)
}

fn native_word(text: &str) -> bool {
    !text.is_empty() && word(text).is_ok()
}

fn client_id_ok(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= 64 && !id.contains(['\n', '\r'])
}

fn check_batch_item(order: &Order) -> Result<(), DescriptorError> {
    (order.order_type == OrderType::Limit
        && order.trigger_price.is_none()
        && order.client_id.is_some())
    .then_some(())
    .ok_or(DescriptorError::Presence)
}

fn positive(decimal: &str) -> bool {
    decimal.bytes().any(|b| (b'1'..=b'9').contains(&b))
}

/// Refuse NaN, infinities and negative times before anything compares them.
/// The deadline compares raw values; only the skew converts units, flooring as
/// native does.
pub fn validate(
    descriptor: &ParadexDescriptor,
    peer_now_ms: f64,
    deadline_ms: f64,
) -> Result<(), DescriptorError> {
    for time in [peer_now_ms, deadline_ms] {
        if !time.is_finite() || time < 0. || time > MAX_SAFE as f64 {
            return Err(DescriptorError::Time);
        }
    }
    if peer_now_ms >= deadline_ms {
        return Err(DescriptorError::Deadline);
    }
    descriptor.check()?;
    let now_ms = peer_now_ms as u64;
    let within = |stamp: u64, now: u64, skew: u64| stamp.abs_diff(now) <= skew;
    let fresh = match &descriptor.0 {
        Kind::Auth { timestamp_s, .. } => within(*timestamp_s, now_ms / 1000, AUTH_SKEW_S),
        Kind::OrderCreate { order, .. } => within(order.timestamp_ms, now_ms, ORDER_SKEW_MS),
        Kind::OrderBatch { items } => items
            .iter()
            .all(|item| within(item.timestamp_ms, now_ms, ORDER_SKEW_MS)),
        Kind::OrderModify(modify) => within(modify.timestamp_ms, now_ms, ORDER_SKEW_MS),
    };
    fresh.then_some(()).ok_or(DescriptorError::Skew)
}

/// The native typed-data hashes to sign, through the same builder local signing
/// uses.
pub fn digests(
    descriptor: &ParadexDescriptor,
    binding: &ParadexBinding,
) -> Result<Digests, DescriptorError> {
    descriptor.check()?;
    let hash = |definition: &str, values: &[Felt]| {
        typed_hash(binding.chain_id, binding.account, definition, values)
            .map(|hash| hash.to_bytes_be())
            .map_err(|_| DescriptorError::Presence)
    };
    let order = |order: &Order| {
        let (definition, values, len) = order_values(
            order.timestamp_ms,
            &order.market,
            side(order.side),
            order_type(order.order_type),
            &order.size,
            order.price.as_deref(),
            None,
        )
        .map_err(|_| DescriptorError::Presence)?;
        hash(definition, &values[..len])
    };
    let digests = match &descriptor.0 {
        Kind::Auth {
            timestamp_s,
            expiration_s,
        } => vec![hash(AUTH_TYPE, &auth_values(*timestamp_s, *expiration_s))?],
        Kind::OrderCreate { order: item, .. } => vec![order(item)?],
        Kind::OrderBatch { items } => items.iter().map(order).collect::<Result<_, _>>()?,
        Kind::OrderModify(modify) => {
            let (definition, values, len) = order_values(
                modify.timestamp_ms,
                &modify.market,
                side(modify.side),
                modify_type(modify.order_type),
                &modify.size,
                Some(&modify.price),
                Some(&modify.id),
            )
            .map_err(|_| DescriptorError::Presence)?;
            vec![hash(definition, &values[..len])?]
        }
    };
    Ok(Digests(digests))
}

fn side(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

fn order_type(kind: OrderType) -> &'static str {
    match kind {
        OrderType::Limit => "LIMIT",
        OrderType::Market => "MARKET",
        OrderType::StopLimit => "STOP_LIMIT",
        OrderType::StopMarket => "STOP_MARKET",
        OrderType::TakeProfitMarket => "TAKE_PROFIT_MARKET",
    }
}

fn modify_type(kind: ModifyOrderType) -> &'static str {
    match kind {
        ModifyOrderType::Limit => "LIMIT",
        ModifyOrderType::StopLimit => "STOP_LIMIT",
        ModifyOrderType::StopMarket => "STOP_MARKET",
        ModifyOrderType::StopLossLimit => "STOP_LOSS_LIMIT",
        ModifyOrderType::StopLossMarket => "STOP_LOSS_MARKET",
        ModifyOrderType::TakeProfitLimit => "TAKE_PROFIT_LIMIT",
        ModifyOrderType::TakeProfitMarket => "TAKE_PROFIT_MARKET",
    }
}

fn safe_int<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(d)?;
    (value <= MAX_SAFE)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("unsafe integer"))
}

/// Canonical native decimal (`num::format`): no sign, exponent or padding.
fn canonical(text: &str) -> bool {
    let (integer, fraction) = match text.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (text, None),
    };
    !integer.is_empty()
        && integer.bytes().all(|b| b.is_ascii_digit())
        && (integer == "0" || !integer.starts_with('0'))
        && fraction.is_none_or(|f| {
            !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) && !f.ends_with('0')
        })
        && quantum(text).is_ok()
}

fn decimal<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let text = String::deserialize(d)?;
    canonical(&text)
        .then_some(text)
        .ok_or_else(|| D::Error::custom("noncanonical decimal"))
}

/// Present with a value or explicitly null; a missing key refuses.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

fn nullable_decimal<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    match Option::<String>::deserialize(d)? {
        Some(text) if !canonical(&text) => Err(D::Error::custom("noncanonical decimal")),
        other => Ok(other),
    }
}

/// A native market or id: an ASCII short string or felt, as native signs it.
fn market<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let text = String::deserialize(d)?;
    native_word(&text)
        .then_some(text)
        .ok_or_else(|| D::Error::custom("invalid market or id"))
}

/// Native `validate_caller_ids`: 1..=64 characters, no line breaks.
fn client_id<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    match Option::<String>::deserialize(d)? {
        Some(id) if !client_id_ok(&id) => Err(D::Error::custom("invalid client id")),
        other => Ok(other),
    }
}

#[cfg(test)]
mod constructed_value_tests {
    use super::*;

    const NOW: u64 = 1_717_171_717_171;

    fn binding() -> ParadexBinding {
        ParadexBinding::new("0x0123", "PRIVATE_SN_POTC_SEPOLIA").unwrap()
    }

    fn order_descriptor() -> ParadexDescriptor {
        ParadexDescriptor::from_json(&format!(
            r#"{{"family":"paradex","op":"orderCreate","revision":"paradex/1","target":"orders",
            "order":{{"timestampMs":{NOW},"market":"BTC-USD-PERP","side":"BUY","orderType":"LIMIT",
            "size":"1","price":"100","triggerPrice":null,"instruction":"GTC","flags":null,
            "stp":"EXPIRE_TAKER","clientId":"ts_1"}}}}"#
        ))
        .unwrap()
    }

    fn modify_descriptor() -> ParadexDescriptor {
        ParadexDescriptor::from_json(&format!(
            r#"{{"family":"paradex","op":"orderModify","revision":"paradex/1","id":"123",
            "timestampMs":{NOW},"market":"BTC-USD-PERP","side":"BUY","orderType":"LIMIT",
            "size":"1","price":"100","triggerPrice":null}}"#
        ))
        .unwrap()
    }

    /// A value edited after parsing must fail validate AND digests exactly as
    /// the wire would have.
    fn refused(descriptor: &ParadexDescriptor, expected: DescriptorError) {
        assert_eq!(
            validate(descriptor, NOW as f64, NOW as f64 + 1_000.),
            Err(expected),
            "{descriptor:?}"
        );
        assert_eq!(
            digests(descriptor, &binding()),
            Err(expected),
            "{descriptor:?}"
        );
    }

    fn edit_order(edit: impl Fn(&mut Order)) -> ParadexDescriptor {
        let mut descriptor = order_descriptor();
        let Kind::OrderCreate { order, .. } = &mut descriptor.0 else {
            unreachable!()
        };
        edit(order);
        descriptor
    }

    #[test]
    fn valid_controls_pass_both_paths() {
        for descriptor in [order_descriptor(), modify_descriptor()] {
            assert_eq!(
                validate(&descriptor, NOW as f64, NOW as f64 + 1_000.),
                Ok(())
            );
            assert!(digests(&descriptor, &binding()).is_ok());
        }
    }

    #[test]
    fn edited_order_values_refuse_like_the_wire() {
        for descriptor in [
            edit_order(|o| o.timestamp_ms = u64::MAX),
            edit_order(|o| o.timestamp_ms = MAX_SAFE + 1),
            edit_order(|o| o.client_id = Some(String::new())),
            edit_order(|o| o.client_id = Some("a\nb".into())),
            edit_order(|o| o.client_id = Some("x".repeat(65))),
            edit_order(|o| o.size = "1e1".into()),
            edit_order(|o| o.size = "01".into()),
            edit_order(|o| o.size = "1.0".into()),
            edit_order(|o| o.price = Some("+1".into())),
            edit_order(|o| o.trigger_price = Some("1e-8".into())),
            edit_order(|o| o.market = String::new()),
            edit_order(|o| o.market = "a market name longer than thirty-one".into()),
        ] {
            refused(&descriptor, DescriptorError::Malformed);
        }
    }

    #[test]
    fn edited_presence_refuses_like_the_wire() {
        refused(&edit_order(|o| o.price = None), DescriptorError::Presence);
        refused(&edit_order(|o| o.stp = None), DescriptorError::Presence);
        let mut batch = order_descriptor();
        let Kind::OrderCreate { order, .. } = &batch.0 else {
            unreachable!()
        };
        batch.0 = Kind::OrderBatch {
            items: vec![order.clone(), order.clone()],
        };
        refused(&batch, DescriptorError::Presence);
        batch.0 = Kind::OrderBatch { items: Vec::new() };
        refused(&batch, DescriptorError::Presence);
        let mut auth = order_descriptor();
        auth.0 = Kind::Auth {
            timestamp_s: NOW / 1000,
            expiration_s: NOW / 1000 + 301,
        };
        refused(&auth, DescriptorError::Presence);
        auth.0 = Kind::Auth {
            timestamp_s: MAX_SAFE + 1,
            expiration_s: MAX_SAFE + 301,
        };
        refused(&auth, DescriptorError::Malformed);
    }

    #[test]
    fn edited_modify_values_refuse_like_the_wire() {
        let edit = |change: &dyn Fn(&mut Modify)| {
            let mut descriptor = modify_descriptor();
            let Kind::OrderModify(modify) = &mut descriptor.0 else {
                unreachable!()
            };
            change(modify);
            descriptor
        };
        for descriptor in [
            edit(&|m| m.id = String::new()),
            edit(&|m| m.timestamp_ms = u64::MAX),
            edit(&|m| m.size = "1e1".into()),
            edit(&|m| m.price = "01".into()),
            edit(&|m| m.market = String::new()),
        ] {
            refused(&descriptor, DescriptorError::Malformed);
        }
    }
}
