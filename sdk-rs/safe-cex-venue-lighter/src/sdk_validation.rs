//! Validation decisions over the complete primitive input domain. Property access
//! and the ambient clock are explicit host boundaries; their exceptions retain identity.
use super::{sdk_protocol::*, transaction_signing::HostThrownValue};
use num_bigint::BigInt;
use safe_cex_num::{JsBigIntError, JsPrimitive, js_bigint_primitive, js_number_string};
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    BigInt(BigInt),
    String(Vec<u16>),
    Symbol,
    Function,
    Object,
    /// The browser's document.all exception: falsy, typeof undefined, not === undefined.
    HtmlDda,
}
impl ValidationValue {
    pub fn type_of(&self) -> &'static str {
        match self {
            Self::Undefined | Self::HtmlDda => "undefined",
            Self::Null | Self::Object => "object",
            Self::Boolean(_) => "boolean",
            Self::Number(_) => "number",
            Self::BigInt(_) => "bigint",
            Self::String(_) => "string",
            Self::Symbol => "symbol",
            Self::Function => "function",
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null | Self::HtmlDda => false,
            Self::Boolean(v) => *v,
            Self::Number(v) => *v != 0.0 && !v.is_nan(),
            Self::BigInt(v) => v != &BigInt::from(0),
            Self::String(v) => !v.is_empty(),
            Self::Symbol | Self::Function | Self::Object => true,
        }
    }
}
// ts: ValidationError
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub name: Vec<u16>,
    pub message: Vec<u16>,
}
impl ValidationError {
    // ts: ValidationError.constructor
    pub fn new(message: Vec<u16>) -> Self {
        Self {
            name: "ValidationError".encode_utf16().collect(),
            message,
        }
    }
}
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = String::from_utf16_lossy(&self.name);
        let m = String::from_utf16_lossy(&self.message);
        match (n.is_empty(), m.is_empty()) {
            (true, _) => f.write_str(&m),
            (_, true) => f.write_str(&n),
            _ => write!(f, "{n}: {m}"),
        }
    }
}
impl std::error::Error for ValidationError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationFailure {
    Validation(ValidationError),
    Builtin(JsBigIntError),
    Thrown(HostThrownValue),
}
impl From<HostThrownValue> for ValidationFailure {
    fn from(v: HostThrownValue) -> Self {
        Self::Thrown(v)
    }
}
impl From<JsBigIntError> for ValidationFailure {
    fn from(v: JsBigIntError) -> Self {
        Self::Builtin(v)
    }
}
pub type ValidationResult = Result<(), ValidationFailure>;
fn fail(message: impl AsRef<str>) -> ValidationFailure {
    ValidationFailure::Validation(ValidationError::new(
        message.as_ref().encode_utf16().collect(),
    ))
}
fn bigint<'a>(v: &'a ValidationValue, label: &str) -> Result<&'a BigInt, ValidationFailure> {
    match v {
        ValidationValue::BigInt(n) => Ok(n),
        _ => Err(fail(format!(
            "{label} must be a BigInt, got {}",
            v.type_of()
        ))),
    }
}
fn integer(v: &ValidationValue, label: &str) -> Result<f64, ValidationFailure> {
    let ValidationValue::Number(n) = v else {
        return Err(fail(format!(
            "{label} must be a number, got {}",
            v.type_of()
        )));
    };
    if !n.is_finite() || n.fract() != 0.0 {
        return Err(fail(format!(
            "{label} must be an integer, got {}",
            js_number_string(*n)
        )));
    }
    Ok(*n)
}
// ts: validateAccountIndex
pub fn validate_account_index(v: &ValidationValue) -> ValidationResult {
    let n = bigint(v, "Account index")?;
    if n < &BigInt::from(0) {
        return Err(fail(format!("Account index must be non-negative, got {n}")));
    }
    if n > &BigInt::from(MAX_ACCOUNT_INDEX as u32) {
        return Err(fail(format!(
            "Account index must be at most {}, got {n}",
            js_number_string(MAX_ACCOUNT_INDEX)
        )));
    }
    Ok(())
}
// ts: validateApiKeyIndex
pub fn validate_api_key_index(v: &ValidationValue) -> ValidationResult {
    let n = integer(v, "API key index")?;
    if n < 0.0 {
        return Err(fail(format!(
            "API key index must be non-negative, got {}",
            js_number_string(n)
        )));
    }
    if n > MAX_API_KEY_INDEX {
        return Err(fail(format!(
            "API key index must be at most {}, got {}",
            js_number_string(MAX_API_KEY_INDEX),
            js_number_string(n)
        )));
    }
    Ok(())
}
// ts: validateNonce
pub fn validate_nonce(v: &ValidationValue) -> ValidationResult {
    let n = bigint(v, "Nonce")?;
    if n < &BigInt::from(0) {
        return Err(fail(format!("Nonce must be non-negative, got {n}")));
    }
    if n > &BigInt::from(MAX_NONCE as u64) {
        return Err(fail(format!(
            "Nonce must be at most {} (uint32 max), got {n}",
            js_number_string(MAX_NONCE)
        )));
    }
    Ok(())
}
// ts: validateOrderId
pub fn validate_order_id(v: &ValidationValue) -> ValidationResult {
    let n = bigint(v, "Order ID")?;
    if n <= &BigInt::from(0) {
        return Err(fail(format!("Order ID must be positive, got {n}")));
    }
    Ok(())
}
// ts: validateMarketId
pub fn validate_market_id(v: &ValidationValue) -> ValidationResult {
    let n = integer(v, "Market ID")?;
    if n <= 0.0 {
        return Err(fail(format!(
            "Market ID must be positive, got {}",
            js_number_string(n)
        )));
    }
    Ok(())
}
// ts: validatePrice
pub fn validate_price(v: &ValidationValue) -> ValidationResult {
    let n = bigint(v, "Price")?;
    if n <= &BigInt::from(0) {
        return Err(fail(format!("Price must be positive, got {n}")));
    }
    Ok(())
}
// ts: validateAmount
pub fn validate_amount(v: &ValidationValue) -> ValidationResult {
    let n = bigint(v, "Amount")?;
    if n < &*MIN_ORDER_SIZE {
        return Err(fail(format!(
            "Amount must be at least {}, got {n}",
            *MIN_ORDER_SIZE
        )));
    }
    if n > &*MAX_ORDER_SIZE {
        return Err(fail(format!(
            "Amount must be at most {}, got {n}",
            *MAX_ORDER_SIZE
        )));
    }
    Ok(())
}
pub trait OrderParamsReader {
    fn get(&self, field: &str) -> Result<ValidationValue, HostThrownValue>;
}
// ts: validateOrderParams
pub fn validate_order_params(params: &impl OrderParamsReader) -> ValidationResult {
    if !params.get("orderId")?.truthy()
        && !matches!(params.get("orderId")?,ValidationValue::BigInt(n)if n==BigInt::from(0))
    {
        return Err(fail("Order ID is required"));
    }
    if !params.get("marketId")?.truthy()
        && !matches!(params.get("marketId")?,ValidationValue::Number(n)if n==0.0)
    {
        return Err(fail("Market ID is required"));
    }
    if !matches!(params.get("isBuy")?, ValidationValue::Boolean(_)) {
        return Err(fail("isBuy must be a boolean"));
    }
    if !params.get("price")?.truthy()
        && !matches!(params.get("price")?,ValidationValue::BigInt(n)if n==BigInt::from(0))
    {
        return Err(fail("Price is required"));
    }
    if !params.get("amount")?.truthy()
        && !matches!(params.get("amount")?,ValidationValue::BigInt(n)if n==BigInt::from(0))
    {
        return Err(fail("Amount is required"));
    }
    validate_order_id(&params.get("orderId")?)?;
    validate_market_id(&params.get("marketId")?)?;
    validate_price(&params.get("price")?)?;
    validate_amount(&params.get("amount")?)?;
    if !matches!(params.get("postOnly")?, ValidationValue::Undefined)
        && !matches!(params.get("postOnly")?, ValidationValue::Boolean(_))
    {
        return Err(fail("postOnly must be a boolean"));
    }
    if !matches!(params.get("reduceOnly")?, ValidationValue::Undefined)
        && !matches!(params.get("reduceOnly")?, ValidationValue::Boolean(_))
    {
        return Err(fail("reduceOnly must be a boolean"));
    }
    if params.get("postOnly")?.truthy() && params.get("reduceOnly")?.truthy() {
        return Err(fail("Cannot set both postOnly and reduceOnly"));
    }
    Ok(())
}
fn hex_string<'a>(v: &'a ValidationValue, label: &str) -> Result<&'a [u16], ValidationFailure> {
    match v {
        ValidationValue::String(s) => Ok(s),
        _ => Err(fail(format!(
            "{label} must be a string, got {}",
            v.type_of()
        ))),
    }
}
fn prefixed(s: &[u16], label: &str) -> Result<(), ValidationFailure> {
    if !s.starts_with(&[48, 120]) {
        return Err(fail(format!("{label} must start with 0x prefix")));
    }
    Ok(())
}
fn all_hex(s: &[u16]) -> bool {
    s.iter().all(|n| matches!(n,48..=57|65..=70|97..=102))
}
fn validate_key(v: &ValidationValue, label: &str) -> ValidationResult {
    let s = hex_string(v, label)?;
    prefixed(s, label)?;
    let hex = &s[2..];
    if hex.len() != 64 {
        return Err(fail(format!(
            "{label} must be 64 hex characters (32 bytes), got {}",
            hex.len()
        )));
    }
    if !all_hex(hex) {
        return Err(fail(format!(
            "{label} must contain only hexadecimal characters"
        )));
    }
    Ok(())
}
// ts: validatePrivateKey
pub fn validate_private_key(v: &ValidationValue) -> ValidationResult {
    validate_key(v, "Private key")
}
// ts: validatePublicKey
pub fn validate_public_key(v: &ValidationValue) -> ValidationResult {
    validate_key(v, "Public key")
}
// ts: validateAddress
pub fn validate_address(v: &ValidationValue) -> ValidationResult {
    let s = hex_string(v, "Address")?;
    prefixed(s, "Address")?;
    if s.len() != 42 {
        return Err(fail(format!(
            "Address must be 42 characters (0x + 40 hex chars), got {}",
            s.len()
        )));
    }
    if !all_hex(&s[2..]) {
        return Err(fail("Address must contain only hexadecimal characters"));
    }
    Ok(())
}
// ts: validateDeadline
pub fn validate_deadline(
    v: &ValidationValue,
    min_future: Option<f64>,
    clock: &dyn Fn() -> Result<f64, HostThrownValue>,
) -> ValidationResult {
    let deadline = bigint(v, "Deadline")?;
    if deadline <= &BigInt::from(0) {
        return Err(fail(format!("Deadline must be positive, got {deadline}")));
    }
    let now = js_bigint_primitive(&JsPrimitive::Number((clock()? / 1000.0).floor()))?;
    let future = min_future.unwrap_or(60.0);
    let min_deadline = now + js_bigint_primitive(&JsPrimitive::Number(future))?;
    if deadline < &min_deadline {
        return Err(fail(format!(
            "Deadline must be at least {} seconds in the future. Current: {deadline}, minimum required: {min_deadline}",
            js_number_string(future)
        )));
    }
    Ok(())
}
// ts: validateChainId
pub fn validate_chain_id(v: &ValidationValue) -> ValidationResult {
    let n = integer(v, "Chain ID")?;
    if n <= 0.0 {
        return Err(fail(format!(
            "Chain ID must be positive, got {}",
            js_number_string(n)
        )));
    }
    Ok(())
}
