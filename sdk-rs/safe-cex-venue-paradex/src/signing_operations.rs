//! Source signing payload construction and decimal guards; crypto remains a capability.
use std::future::Future;

fn join(prefix: &[u16], middle: &str, suffix: &[u16]) -> Vec<u16> {
    prefix
        .iter()
        .copied()
        .chain(middle.encode_utf16())
        .chain(suffix.iter().copied())
        .collect()
}

// ts: ParadexRequestSigner#toChainQuantum
pub fn to_chain_quantum<D, M, R, T, E>(
    value: &[u16],
    field: &[u16],
    decimal: impl FnOnce(&[u16]) -> Result<D, E>,
    finite: impl FnOnce(&D) -> Result<bool, E>,
    nan: impl FnOnce(&D) -> Result<bool, E>,
    negative: impl FnOnce(&D) -> Result<bool, E>,
    multiply: impl FnOnce(D, f64) -> Result<M, E>,
    integer: impl FnOnce(M, u32) -> Result<R, E>,
    fixed: impl FnOnce(R, u32) -> Result<T, E>,
    make_error: impl FnOnce(Vec<u16>) -> Result<E, E>,
) -> Result<T, E> {
    let number = decimal(value)?;
    if !finite(&number)? || nan(&number)? {
        let prefix: Vec<u16> = "Invalid "
            .encode_utf16()
            .chain(field.iter().copied())
            .collect();
        return Err(make_error(join(&prefix, " value for signing: ", value))?);
    }
    if negative(&number)? {
        return Err(make_error(join(
            field,
            " cannot be negative for signing: ",
            value,
        ))?);
    }
    fixed(integer(multiply(number, 1e8)?, 1)?, 0)
}

// ts: ParadexRequestSigner.signOrder; ParadexRequestSigner.signModifyOrder
pub struct OrderSigningData {
    pub domain_name: &'static str,
    pub chain_id: Vec<u16>,
    pub domain_version: &'static str,
    pub domain_fields: [(&'static str, &'static str); 3],
    pub primary_type: &'static str,
    pub message_fields: Vec<(&'static str, &'static str)>,
    pub timestamp: Vec<u16>,
    pub market: Vec<u16>,
    pub side: &'static str,
    pub order_type: Vec<u16>,
    pub size: Vec<u16>,
    pub price: Vec<u16>,
    pub id: Option<Vec<u16>>,
}

// ts: ParadexRequestSigner.signOrder; ParadexRequestSigner.signModifyOrder
pub struct SignedOrder<S, H> {
    pub signature: S,
    pub signature_timestamp: f64,
    pub headers: H,
}

fn data(
    chain_id: Vec<u16>,
    timestamp: Vec<u16>,
    market: Vec<u16>,
    side: &'static str,
    kind: Vec<u16>,
    size: Vec<u16>,
    price: Vec<u16>,
    id: Option<Vec<u16>>,
) -> OrderSigningData {
    let mut fields = vec![
        ("timestamp", "felt"),
        ("market", "felt"),
        ("side", "felt"),
        ("orderType", "felt"),
        ("size", "felt"),
        ("price", "felt"),
    ];
    if id.is_some() {
        fields.push(("id", "felt"));
    }
    OrderSigningData {
        domain_name: "Paradex",
        chain_id,
        domain_version: "1",
        domain_fields: [("name", "felt"), ("chainId", "felt"), ("version", "felt")],
        primary_type: if id.is_some() { "ModifyOrder" } else { "Order" },
        message_fields: fields,
        timestamp,
        market,
        side,
        order_type: kind,
        size,
        price,
        id,
    }
}

// ts: ParadexRequestSigner.signOrder
pub async fn sign_order<F, R, S, H, E>(
    read_now: impl FnOnce() -> Result<f64, E>,
    read_buy: impl FnOnce() -> Result<bool, E>,
    quantum_size: impl FnOnce() -> Result<Vec<u16>, E>,
    mut read_type: impl FnMut() -> Result<Vec<u16>, E>,
    read_price_truthy: impl FnOnce() -> Result<bool, E>,
    quantum_price: impl FnOnce() -> Result<Vec<u16>, E>,
    chain_id_hex: impl FnOnce() -> Result<Vec<u16>, E>,
    timestamp_string: impl FnOnce(f64) -> Result<Vec<u16>, E>,
    read_market: impl FnOnce() -> Result<Vec<u16>, E>,
    sign: impl FnOnce(OrderSigningData) -> Result<F, E>,
    read_signature: impl FnOnce(R) -> Result<(S, H), E>,
    log_error: impl FnOnce(&'static str, &E) -> Result<(), E>,
) -> Result<SignedOrder<S, H>, E>
where
    F: Future<Output = Result<R, E>>,
{
    let result = async {
        let now = read_now()?;
        let side = if read_buy()? { "1" } else { "2" };
        let size = quantum_size()?;
        let price =
            if read_type()?.iter().copied().eq("MARKET".encode_utf16()) || !read_price_truthy()? {
                vec![48]
            } else {
                quantum_price()?
            };
        let chain = chain_id_hex()?;
        let timestamp = timestamp_string(now)?;
        let market = read_market()?;
        let kind = read_type()?;
        let payload = data(chain, timestamp, market, side, kind, size, price, None);
        let (signature, headers) = read_signature(sign(payload)?.await?)?;
        Ok(SignedOrder {
            signature,
            signature_timestamp: now,
            headers,
        })
    }
    .await;
    match result {
        Ok(signed) => Ok(signed),
        Err(error) => {
            log_error("[Paradex Order Signing] Failed to sign order:", &error)?;
            Err(error)
        }
    }
}

// ts: ParadexRequestSigner.signModifyOrder
pub async fn sign_modify_order<F, R, S, H, E>(
    read_id_truthy: impl FnOnce() -> Result<bool, E>,
    make_missing_id_error: impl FnOnce(&'static str) -> Result<E, E>,
    read_now: impl FnOnce() -> Result<f64, E>,
    read_buy: impl FnOnce() -> Result<bool, E>,
    quantum_size: impl FnOnce() -> Result<Vec<u16>, E>,
    quantum_price: impl FnOnce() -> Result<Vec<u16>, E>,
    chain_id_hex: impl FnOnce() -> Result<Vec<u16>, E>,
    timestamp_string: impl FnOnce(f64) -> Result<Vec<u16>, E>,
    read_market: impl FnOnce() -> Result<Vec<u16>, E>,
    read_type: impl FnOnce() -> Result<Vec<u16>, E>,
    read_id: impl FnOnce() -> Result<Vec<u16>, E>,
    sign: impl FnOnce(OrderSigningData) -> Result<F, E>,
    read_signature: impl FnOnce(R) -> Result<(S, H), E>,
) -> Result<SignedOrder<S, H>, E>
where
    F: Future<Output = Result<R, E>>,
{
    if !read_id_truthy()? {
        return Err(make_missing_id_error(
            "Cannot sign Paradex order amendment without an order id",
        )?);
    }
    let now = read_now()?;
    let side = if read_buy()? { "1" } else { "2" };
    let size = quantum_size()?;
    let price = quantum_price()?;
    let chain = chain_id_hex()?;
    let timestamp = timestamp_string(now)?;
    let market = read_market()?;
    let kind = read_type()?;
    let id = read_id()?;
    let payload = data(chain, timestamp, market, side, kind, size, price, Some(id));
    let (signature, headers) = read_signature(sign(payload)?.await?)?;
    Ok(SignedOrder {
        signature,
        signature_timestamp: now,
        headers,
    })
}
