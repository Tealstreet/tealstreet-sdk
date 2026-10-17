//! Placement retry and batch control over opaque host objects and errors.
use std::future::Future;

pub trait PlaceOrderHost {
    type Options;
    type Payloads;
    type Positions;
    type Result;
    type Message;
    type Error;
    fn resolve_caller(&mut self, options: &Self::Options) -> Result<(), Self::Error>;
    fn hydrate(&mut self, options: &Self::Options)
    -> impl Future<Output = Result<(), Self::Error>>;
    fn format(&mut self, options: &Self::Options) -> Result<Self::Payloads, Self::Error>;
    fn batch(
        &mut self,
        payloads: Self::Payloads,
    ) -> impl Future<Output = Result<Self::Result, Self::Error>>;
    fn error_message(&mut self, error: &Self::Error) -> Result<Self::Message, Self::Error>;
    fn includes(
        &mut self,
        message: &Self::Message,
        needle: &'static str,
    ) -> Result<bool, Self::Error>;
    fn lowercase(&mut self, message: &Self::Message) -> Result<Self::Message, Self::Error>;
    fn message_string(&mut self, message: &Self::Message) -> Result<Vec<u16>, Self::Error>;
    fn warn(&mut self, message: Vec<u16>) -> Result<(), Self::Error>;
    fn symbol(&mut self, options: &Self::Options) -> Result<Vec<u16>, Self::Error>;
    fn delete_position_id(&mut self, key: Vec<u16>) -> Result<(), Self::Error>;
    fn fetch_positions(&mut self) -> impl Future<Output = Result<Self::Positions, Self::Error>>;
    fn update_positions(&mut self, positions: Self::Positions) -> Result<(), Self::Error>;
}
pub async fn place_order<H: PlaceOrderHost>(
    host: &mut H,
    options: H::Options,
) -> Result<H::Result, H::Error> {
    host.resolve_caller(&options)?;
    let attempt = async {
        host.hydrate(&options).await?;
        let payloads = host.format(&options)?;
        host.batch(payloads).await
    }
    .await;
    match attempt {
        Ok(result) => Ok(result),
        Err(error) => {
            let message = host.error_message(&error)?;
            let retry = if host.includes(&message, "Missing positionId")? {
                true
            } else {
                let lower = host.lowercase(&message)?;
                host.includes(&lower, "not exist")?
            };
            if !retry {
                return Err(error);
            }
            let mut warning = "[Bitunix] ".encode_utf16().collect::<Vec<_>>();
            warning.extend(host.message_string(&message)?);
            warning.extend(" - refreshing positions and retrying".encode_utf16());
            host.warn(warning)?;
            for suffix in [":long", ":short"] {
                let mut key = host.symbol(&options)?;
                key.extend(suffix.encode_utf16());
                host.delete_position_id(key)?;
            }
            let positions = host.fetch_positions().await?;
            host.update_positions(positions)?;
            let payloads = host.format(&options)?;
            host.batch(payloads).await
        }
    }
}

pub trait BatchOrderHost {
    type Value: Clone;
    type Iterator;
    type Error;
    fn input_length(&mut self) -> Result<usize, Self::Error>;
    fn input_at(&mut self, index: usize) -> Result<Option<Self::Value>, Self::Error>;
    fn field(&mut self, value: &Self::Value, path: &[&str]) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn zero(&self, value: &Self::Value) -> bool;
    fn limit(&self, value: &Self::Value) -> bool;
    fn has_own(&mut self, value: &Self::Value, key: &'static str) -> Result<bool, Self::Error>;
    fn identical(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn symbol_key(&mut self, value: &Self::Value) -> Result<Option<Vec<u16>>, Self::Error>;
    fn omit(&mut self, value: &Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn batch_payload(
        &mut self,
        symbol: Vec<u16>,
        rows: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn post(
        &mut self,
        endpoint: &'static str,
        payload: Self::Value,
    ) -> impl Future<Output = Result<Self::Value, Self::Error>>;
    fn sleep(&mut self, delay: f64) -> impl Future<Output = Result<(), Self::Error>>;
    fn uuid(&mut self) -> Result<Self::Value, Self::Error>;
    fn string(&mut self, text: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn to_string(&mut self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn error(&mut self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
    fn emit(&mut self, message: Self::Value) -> Result<(), Self::Error>;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn array_length(&mut self, value: &Self::Value) -> Result<usize, Self::Error>;
    fn array_at(
        &mut self,
        value: &Self::Value,
        index: usize,
    ) -> Result<Option<Self::Value>, Self::Error>;
    fn array(&mut self, values: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, value: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&mut self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn nullish(&self, value: &Self::Value) -> bool;
}
fn endpoint(key: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == key)
        .unwrap()
        .1
}
fn index_key(key: &[u16]) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key[0] == 48) {
        return None;
    }
    let mut n = 0u64;
    for c in key {
        if !(48..=57).contains(c) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(*c - 48))?;
        if n >= u64::from(u32::MAX) {
            return None;
        }
    }
    Some(n as u32)
}
fn message<H: BatchOrderHost>(
    host: &mut H,
    body: &H::Value,
    prefix: &'static str,
) -> Result<H::Value, H::Error> {
    let msg = host.field(body, &["msg"])?;
    if host.truthy(&msg) {
        return Ok(msg);
    }
    let code = host.field(body, &["code"])?;
    let mut text = prefix.encode_utf16().collect::<Vec<_>>();
    text.extend(host.to_string(&code)?);
    host.string(text)
}
async fn tpsl<H: BatchOrderHost>(
    host: &mut H,
    order: &H::Value,
    position: bool,
) -> Result<H::Value, H::Error> {
    let payload = if position {
        host.omit(order, "isPositionOrder")?
    } else {
        order.clone()
    };
    let body = host
        .post(
            endpoint(if position {
                "POSITION_TP_SL_ORDER"
            } else {
                "TP_SL_ORDER"
            }),
            payload,
        )
        .await?;
    let code = host.field(&body, &["code"])?;
    if !host.zero(&code) {
        let msg = host.field(&body, &["msg"])?;
        let msg = if host.truthy(&msg) {
            msg
        } else {
            host.string("Unknown error".encode_utf16().collect())?
        };
        return Err(host.error(msg)?);
    }
    let id = host.field(&body, &["data", "orderId"])?;
    if host.truthy(&id) {
        return Ok(id);
    }
    let client = host.field(order, &["clientId"])?;
    if host.truthy(&client) {
        Ok(client)
    } else {
        host.uuid()
    }
}
// Ordinary row property access throws on nullish values; the surrounding response
// envelope keeps its separate optional-chain reads through `field`.
fn row_field<H: BatchOrderHost>(
    host: &mut H,
    row: &H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    if host.nullish(row) {
        let mut message = "Cannot read properties of "
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend(host.to_string(row)?);
        message.extend(format!(" (reading '{key}')").encode_utf16());
        let message = host.string(message)?;
        return Err(host.error(message)?);
    }
    host.field(row, &[key])
}
fn legacy_rows<H: BatchOrderHost>(
    host: &mut H,
    array: &H::Value,
    failure: bool,
) -> Result<H::Value, H::Error> {
    let mut rows = Vec::new();
    for i in 0..host.array_length(array)? {
        if let Some(row) = host.array_at(array, i)? {
            let code = row_field(host, &row, "code")?;
            if host.truthy(&code) == failure {
                rows.push(row);
            }
        }
    }
    host.array(rows)
}

pub async fn place_order_batch<H: BatchOrderHost>(host: &mut H) -> Result<Vec<H::Value>, H::Error> {
    let mut positions = Vec::new();
    for i in 0..host.input_length()? {
        if let Some(row) = host.input_at(i)? {
            let flag = host.field(&row, &["isPositionOrder"])?;
            if host.truthy(&flag) {
                positions.push(row);
            }
        }
    }
    let mut remaining = Vec::new();
    for i in 0..host.input_length()? {
        if let Some(row) = host.input_at(i)? {
            let flag = host.field(&row, &["isPositionOrder"])?;
            if !host.truthy(&flag) {
                remaining.push(row);
            }
        }
    }
    let mut protections = Vec::new();
    for row in &remaining {
        let kind = host.field(row, &["orderType"])?;
        if !host.limit(&kind)
            && (host.has_own(row, "tpPrice")?
                || host.has_own(row, "slPrice")?
                || host.has_own(row, "tpQty")?
                || host.has_own(row, "slQty")?)
        {
            protections.push(row.clone());
        }
    }
    let regular = remaining
        .into_iter()
        .filter(|row| !protections.iter().any(|tp| host.identical(tp, row)))
        .collect::<Vec<_>>();
    let mut ids = Vec::new();
    for row in &positions {
        match tpsl(host, row, true).await {
            Ok(id) => ids.push(id),
            Err(error) => {
                host.report(&error)?;
                return Err(error);
            }
        }
    }
    for row in &protections {
        match tpsl(host, row, false).await {
            Ok(id) => ids.push(id),
            Err(error) => {
                host.report(&error)?;
                return Err(error);
            }
        }
    }
    let mut groups: Vec<(Vec<u16>, Vec<H::Value>)> = Vec::new();
    for row in regular {
        if let Some(key) = host.symbol_key(&row)? {
            if let Some((_, rows)) = groups.iter_mut().find(|(k, _)| *k == key) {
                rows.push(row);
            } else {
                groups.push((key, vec![row]));
            }
        }
    }
    groups.sort_by(|(a, _), (b, _)| match (index_key(a), index_key(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    for (symbol, rows) in groups {
        for (chunk, lot) in rows.chunks(5).enumerate() {
            if chunk > 0 {
                host.sleep(1000.0).await?;
            }
            if lot.len() == 1 {
                let result = async {
                    let body = host.post(endpoint("ORDER"), lot[0].clone()).await?;
                    let code = host.field(&body, &["code"])?;
                    if host.truthy(&code) {
                        let code = host.field(&body, &["code"])?;
                        if !host.zero(&code) {
                            let msg = message(host, &body, "Order failed with code ")?;
                            return Err(host.error(msg)?);
                        }
                    }
                    let id = host.field(&body, &["data", "orderId"])?;
                    if host.truthy(&id) {
                        return Ok(id);
                    }
                    let id = host.field(&lot[0], &["clientId"])?;
                    if host.truthy(&id) {
                        Ok(id)
                    } else {
                        host.uuid()
                    }
                }
                .await;
                match result {
                    Ok(id) => ids.push(id),
                    Err(error) => {
                        host.report(&error)?;
                        return Err(error);
                    }
                }
            }
            if lot.len() > 1 {
                let result = async {
                    let mut stripped = Vec::new();
                    for row in lot {
                        stripped.push(host.omit(row, "symbol")?);
                    }
                    let payload = host.batch_payload(symbol.clone(), stripped)?;
                    let body = host.post(endpoint("BATCH_ORDERS"), payload).await?;
                    let code = host.field(&body, &["code"])?;
                    if host.truthy(&code) {
                        let code = host.field(&body, &["code"])?;
                        if !host.zero(&code) {
                            let msg = message(host, &body, "Batch order failed with code ")?;
                            return Err(host.error(msg)?);
                        }
                    }
                    let results = host.field(&body, &["data"])?;
                    let successes = host.field(&results, &["successList"])?;
                    let successes = if host.is_array(&successes) {
                        host.field(&results, &["successList"])?
                    } else if host.is_array(&results) {
                        legacy_rows(host, &results, false)?
                    } else {
                        host.array(vec![])?
                    };
                    let failures = host.field(&results, &["failureList"])?;
                    let failures = if host.is_array(&failures) {
                        host.field(&results, &["failureList"])?
                    } else if host.is_array(&results) {
                        legacy_rows(host, &results, true)?
                    } else {
                        host.array(vec![])?
                    };
                    let mut iter = host.iterator(successes)?;
                    while let Some(row) = host.next(&mut iter)? {
                        let id = row_field(host, &row, "orderId")?;
                        let id = if host.truthy(&id) {
                            id
                        } else {
                            let id = host.field(&row, &["clientId"])?;
                            if host.truthy(&id) {
                                id
                            } else {
                                let id = host.field(&row, &["clientOrderId"])?;
                                if host.truthy(&id) { id } else { host.uuid()? }
                            }
                        };
                        ids.push(id);
                    }
                    let mut iter = host.iterator(failures)?;
                    while let Some(row) = host.next(&mut iter)? {
                        let msg = row_field(host, &row, "errorMsg")?;
                        let msg = if host.truthy(&msg) {
                            msg
                        } else {
                            let msg = host.field(&row, &["msg"])?;
                            if host.truthy(&msg) {
                                msg
                            } else {
                                let code = host.field(&row, &["errorCode"])?;
                                let code = if host.nullish(&code) {
                                    row_field(host, &row, "code")?
                                } else {
                                    code
                                };
                                let mut text =
                                    "Order rejected (code ".encode_utf16().collect::<Vec<_>>();
                                text.extend(host.to_string(&code)?);
                                text.push(41);
                                host.string(text)?
                            }
                        };
                        host.emit(msg)?;
                    }
                    Ok::<_, H::Error>(())
                }
                .await;
                if let Err(error) = result {
                    host.report(&error)?;
                }
            }
        }
    }
    Ok(ids)
}
