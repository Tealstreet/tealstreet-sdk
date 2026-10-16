//! Order command control flow over original HTTP, promise and property capabilities.
use std::{future::Future, pin::Pin};

// ts: ParadexExchange.placeOrder; ParadexExchange.placeOrders; ParadexExchange#placeMainOrder; ParadexExchange.cancelOrder; ParadexExchange#cancelNativeBatchChunk; ParadexExchange.cancelOrders; ParadexExchange.cancelAllOrders; ParadexExchange.cancelSymbolOrders
pub type CommandFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;

// ts: ParadexExchange.placeOrder; ParadexExchange.placeOrders; ParadexExchange#placeMainOrder; ParadexExchange.cancelOrders
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderCommand {
    Main,
    Brackets,
    NativeBatch,
    Single,
    Build,
    CancelBatch,
    CancelSingle,
}

// ts: ParadexExchange.placeOrder; ParadexExchange.placeOrders; ParadexExchange#placeMainOrder; ParadexExchange#cancelNativeBatchChunk; ParadexExchange.cancelOrders; ParadexExchange.cancelAllOrders; ParadexExchange.cancelSymbolOrders; ParadexExchange.cancelOrder
pub trait OrderCommandHost {
    type Value: Clone;
    type Error: Clone;
    type Log;
    type Iterator;
    fn iterator(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn close_iterator(&self, iterator: &Self::Iterator, error: Self::Error) -> Self::Error;
    fn entries(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next_entry(
        &self,
        iterator: &Self::Iterator,
    ) -> Result<Option<(Self::Value, Self::Value)>, Self::Error>;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional_get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn is_null(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn literal(&self, value: &str) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn template(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn text_value(&self, value: Vec<u16>) -> Self::Value;
    fn trim_length(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn length(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn at(&self, value: &Self::Value, index: f64) -> Result<Self::Value, Self::Error>;
    fn spread(&self, value: &Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn slice(&self, value: &Self::Value, start: f64, end: f64) -> Result<Self::Value, Self::Error>;
    fn map_ids(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn state(&self, key: &str) -> Result<Self::Value, Self::Error>;
    fn await_value(
        &self,
        value: Self::Value,
    ) -> Result<CommandFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn command(
        &self,
        command: OrderCommand,
        args: Vec<Self::Value>,
    ) -> Result<CommandFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn resolve_client_id(&self, opts: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn generate_client_id(&self) -> Result<Self::Value, Self::Error>;
    fn assert_client_ids(&self, orders: &Self::Value) -> Result<(), Self::Error>;
    fn native_batch_order(&self, order: &Self::Value) -> Result<bool, Self::Error>;
    fn post(
        &self,
        endpoint: &'static str,
        payload: Self::Value,
        headers: Self::Value,
    ) -> Result<CommandFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn delete(
        &self,
        endpoint: Vec<u16>,
        config: Option<Self::Value>,
    ) -> Result<CommandFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn select_log(&self) -> Result<Self::Log, Self::Error>;
    fn log(
        &self,
        logger: Self::Log,
        text: Vec<u16>,
        severity: &'static str,
    ) -> Result<(), Self::Error>;
    fn new_error(&self, message: Vec<u16>) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn error_is_instance(&self, error: &Self::Error) -> Result<bool, Self::Error>;
    fn emit(&self, message: Self::Value) -> Result<(), Self::Error>;
    fn stringify(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn replace_endpoint(
        &self,
        endpoint: &'static str,
        search: &str,
        value: &Self::Value,
    ) -> Result<Vec<u16>, Self::Error>;
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn cat(parts: &[&[u16]]) -> Vec<u16> {
    parts.iter().flat_map(|s| s.iter().copied()).collect()
}
fn num(n: f64) -> Vec<u16> {
    u(&safe_cex_num::js_number_string(n))
}
fn join<H: OrderCommandHost>(
    h: &H,
    values: &[H::Value],
    separator: &str,
) -> Result<Vec<u16>, H::Error> {
    let mut result = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            result.extend(u(separator));
        }
        if !h.is_null(v) && !h.is_undefined(v) {
            result.extend(h.template(v)?);
        }
    }
    Ok(result)
}
fn same<H: OrderCommandHost>(h: &H, v: &H::Value, s: &str) -> bool {
    h.equal(v, &h.literal(s))
}
fn raise<H: OrderCommandHost, T>(h: &H, message: Vec<u16>) -> Result<T, H::Error> {
    Err(h.new_error(message)?)
}
fn reject<H: OrderCommandHost, T>(h: &H, message: Vec<u16>) -> Result<T, H::Error> {
    h.emit(h.text_value(message.clone()))?;
    raise(h, message)
}
fn malformed<H: OrderCommandHost, T>(h: &H, detail: Vec<u16>) -> Result<T, H::Error> {
    raise(
        h,
        cat(&[&u("Paradex batch cancel response malformed: "), &detail]),
    )
}
fn error_message<H: OrderCommandHost>(h: &H, error: &H::Error) -> Result<H::Value, H::Error> {
    let value = h.error_value(error);
    if h.error_is_instance(error)? {
        h.get(&value, "message")
    } else {
        Ok(h.text_value(h.string(&value)?))
    }
}

// ts: ParadexExchange#generateClientId
pub fn generate_client_id<E>(
    uuid: impl FnOnce() -> Result<Vec<u16>, E>,
    error: impl FnOnce(&'static str) -> Result<E, E>,
) -> Result<Vec<u16>, E> {
    let id = cat(&[&u("ts_"), &uuid()?]);
    if id.len() <= 3 || id.len() > 64 {
        return Err(error("Paradex generated an invalid client_id")?);
    }
    Ok(id)
}

// ts: ParadexExchange.placeOrder
pub async fn place_order<H: OrderCommandHost>(
    h: &H,
    opts: H::Value,
) -> Result<Vec<H::Value>, H::Error> {
    h.resolve_client_id(&opts)?;
    let result = async {
        let main = h.command(OrderCommand::Main, vec![opts.clone()])?.await?;
        let mut ids = vec![main.clone()];
        if h.truthy(&h.get(&opts, "takeProfit")?) || h.truthy(&h.get(&opts, "stopLoss")?) {
            let children = h.command(OrderCommand::Brackets, vec![opts, main])?.await?;
            ids.extend(h.spread(&children)?);
        }
        Ok(ids)
    }
    .await;
    match result {
        Ok(ids) => Ok(ids),
        Err(e) => {
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Error placing order: "),
                    &h.template(&h.error_value(&e))?,
                ]),
                "error",
            )?;
            Err(e)
        }
    }
}

async fn flush<H: OrderCommandHost>(
    h: &H,
    batch: &mut Vec<H::Value>,
    results: &mut Vec<H::Value>,
) -> Result<(), H::Error> {
    if batch.is_empty() {
        return Ok(());
    }
    let pending = std::mem::take(batch);
    let returned = h
        .command(OrderCommand::NativeBatch, vec![h.array(pending)])?
        .await?;
    results.extend(h.spread(&returned)?);
    Ok(())
}

// ts: ParadexExchange.placeOrders
pub async fn place_orders<H: OrderCommandHost>(
    h: &H,
    orders: H::Value,
) -> Result<Vec<H::Value>, H::Error> {
    h.assert_client_ids(&orders)?;
    let mut results = Vec::new();
    let mut batch = Vec::new();
    let iterator = h.iterator(&orders)?;
    while let Some(order) = h.next(&iterator)? {
        let body: Result<(), H::Error> = async {
            if h.native_batch_order(&order)? {
                batch.push(order);
                if batch.len() == 10 {
                    flush(h, &mut batch, &mut results).await?;
                }
                return Ok(());
            }
            flush(h, &mut batch, &mut results).await?;
            let single = async {
                let ids = h.command(OrderCommand::Single, vec![order])?.await?;
                h.spread(&ids)
            }
            .await;
            match single {
                Ok(ids) => results.extend(ids),
                Err(e) => {
                    let logger = h.select_log()?;
                    h.log(
                        logger,
                        cat(&[
                            &u("Error placing order: "),
                            &h.template(&h.error_value(&e))?,
                        ]),
                        "error",
                    )?;
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = body {
            return Err(h.close_iterator(&iterator, error));
        }
    }
    flush(h, &mut batch, &mut results).await?;
    Ok(results)
}

// ts: ParadexExchange#placeMainOrder
pub async fn place_main_order<H: OrderCommandHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let result = async {
        if h.truthy(&h.state("sdkInitPromise")?) {
            h.await_value(h.state("sdkInitPromise")?)?.await?;
        }
        if !h.truthy(&h.state("requestSigner")?) {
            return raise(h, u("Cannot place order: Request signer not initialized"));
        }
        let logger = h.select_log()?;
        h.log(logger, u("Signing order..."), "debug")?;
        let id = h.resolve_client_id(&opts)?;
        let id = if h.is_undefined(&id) || h.is_null(&id) {
            h.generate_client_id()?
        } else {
            id
        };
        let built = h
            .command(OrderCommand::Build, vec![opts, id.clone()])?
            .await?;
        let payload = h.get(&built, "payload")?;
        let headers = h.get(&built, "headers")?;
        let logger = h.select_log()?;
        h.log(
            logger,
            cat(&[&u("Placing order: "), &h.stringify(&payload)?]),
            "debug",
        )?;
        let response = h.post("/orders", payload, headers)?.await?;
        let data = h.get(&response, "data")?;
        if !h.truthy(&data) || !h.is_object(&data) || h.is_array(&data) {
            return reject(h, u("Paradex order response malformed: expected an object"));
        }
        if !h.is_string(&h.get(&data, "id")?) || h.trim_length(&h.get(&data, "id")?)? == 0.0 {
            return reject(
                h,
                u("Paradex order response malformed: missing canonical order id"),
            );
        }
        if !h.equal(&h.get(&data, "client_id")?, &id) {
            return reject(
                h,
                cat(&[
                    &u("Paradex order response malformed: expected echoed client_id "),
                    &h.template(&id)?,
                ]),
            );
        }
        let logger = h.select_log()?;
        h.log(
            logger,
            cat(&[
                &u("Main order placed: "),
                &h.template(&h.get(&data, "id")?)?,
            ]),
            "info",
        )?;
        h.get(&data, "id")
    }
    .await;
    match result {
        Ok(id) => Ok(id),
        Err(e) => {
            let value = h.error_value(&e);
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Error in placeMainOrder: "),
                    &h.template(&h.get(&value, "message")?)?,
                ]),
                "error",
            )?;
            let mut message = h.get(&value, "message")?;
            let response = h.optional_get(&value, "response")?;
            if h.truthy(&h.optional_get(&response, "data")?) {
                let logger = h.select_log()?;
                let data = h.get(&h.get(&value, "response")?, "data")?;
                h.log(
                    logger,
                    cat(&[&u("API error response: "), &h.stringify(&data)?]),
                    "error",
                )?;
                let data = h.get(&h.get(&value, "response")?, "data")?;
                if h.truthy(&h.get(&data, "error")?)
                    && h.truthy(&h.get(&h.get(&h.get(&value, "response")?, "data")?, "message")?)
                {
                    let left =
                        h.template(&h.get(&h.get(&h.get(&value, "response")?, "data")?, "error")?)?;
                    let right = h.template(
                        &h.get(&h.get(&h.get(&value, "response")?, "data")?, "message")?,
                    )?;
                    message = h.text_value(cat(&[&left, &u(": "), &right]));
                } else if h.truthy(&h.get(&h.get(&h.get(&value, "response")?, "data")?, "message")?)
                {
                    message = h.get(&h.get(&h.get(&value, "response")?, "data")?, "message")?;
                } else if h.truthy(&h.get(&h.get(&h.get(&value, "response")?, "data")?, "error")?) {
                    message = h.get(&h.get(&h.get(&value, "response")?, "data")?, "error")?;
                }
                h.emit(message)?;
            }
            Err(e)
        }
    }
}

// ts: ParadexExchange.cancelOrder
pub async fn cancel_order<H: OrderCommandHost>(h: &H, id: H::Value) -> Result<(), H::Error> {
    let result = async {
        let endpoint = h.replace_endpoint("/orders/{id}", "{id}", &id)?;
        h.delete(endpoint, None)?.await?;
        let logger = h.select_log()?;
        h.log(
            logger,
            cat(&[&u("Cancelled order "), &h.template(&id)?]),
            "debug",
        )
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Error canceling order "),
                    &h.template(&id)?,
                    &u(": "),
                    &h.template(&h.error_value(&e))?,
                ]),
                "error",
            )?;
            Err(e)
        }
    }
}

// ts: ParadexExchange.cancelAllOrders
pub async fn cancel_all_orders<H: OrderCommandHost>(h: &H) -> Result<(), H::Error> {
    let result = async {
        h.delete(u("/orders"), None)?.await?;
        let logger = h.select_log()?;
        h.log(logger, u("Cancelled all orders"), "info")
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Error canceling all orders: "),
                    &h.template(&h.error_value(&e))?,
                ]),
                "error",
            )?;
            Err(e)
        }
    }
}

// ts: ParadexExchange.cancelSymbolOrders
pub async fn cancel_symbol_orders<H: OrderCommandHost>(
    h: &H,
    symbol: H::Value,
) -> Result<(), H::Error> {
    let result = async {
        let config = h.object(vec![("params", h.object(vec![("market", symbol.clone())]))]);
        h.delete(u("/orders"), Some(config))?.await?;
        let logger = h.select_log()?;
        h.log(
            logger,
            cat(&[&u("Cancelled all orders for "), &h.template(&symbol)?]),
            "info",
        )
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Error canceling symbol orders: "),
                    &h.template(&h.error_value(&e))?,
                ]),
                "error",
            )?;
            Err(e)
        }
    }
}

// ts: ParadexExchange.cancelOrders
pub async fn cancel_orders<H: OrderCommandHost>(h: &H, orders: H::Value) -> Result<(), H::Error> {
    let mut failures = Vec::new();
    let mut offset = 0.0;
    while offset < h.length(&orders)? {
        let chunk = h.slice(&orders, offset, offset + 2.0)?;
        let ids = h.map_ids(&chunk)?;
        let result: Result<(), H::Error> = async {
            if h.length(&ids)? == 2.0 {
                h.command(OrderCommand::CancelBatch, vec![ids.clone()])?
                    .await?;
            } else {
                h.command(OrderCommand::CancelSingle, vec![h.at(&ids, 0.0)?])?
                    .await?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            failures.push((error.clone(), ids.clone()));
            let message = error_message(h, &error)?;
            let logger = h.select_log()?;
            h.log(
                logger,
                cat(&[
                    &u("Error canceling order chunk ["),
                    &join(h, &h.spread(&ids)?, ", ")?,
                    &u("]: "),
                    &h.template(&message)?,
                ]),
                "error",
            )?;
        }
        offset += 2.0;
    }
    if failures.len() == 1 {
        return Err(failures.remove(0).0);
    }
    if failures.len() > 1 {
        let mut details = Vec::new();
        for (error, ids) in &failures {
            let message = error_message(h, error)?;
            details.push(h.text_value(cat(&[
                &u("["),
                &join(h, &h.spread(ids)?, ", ")?,
                &u("]: "),
                &h.template(&message)?,
            ])));
        }
        return raise(
            h,
            cat(&[
                &u("Paradex cancellation failed for "),
                &num(failures.len() as f64),
                &u(" chunks after attempting all orders: "),
                &join(h, &details, "; ")?,
            ]),
        );
    }
    Ok(())
}

// ts: ParadexExchange#cancelNativeBatchChunk
pub async fn cancel_native_batch_chunk<H: OrderCommandHost>(
    h: &H,
    ids: H::Value,
) -> Result<(), H::Error> {
    if h.length(&ids)? != 2.0 {
        return raise(
            h,
            cat(&[
                &u("Paradex native cancel batch requires exactly 2 order IDs; received "),
                &num(h.length(&ids)?),
            ]),
        );
    }
    let requested = h.spread(&ids)?;
    for id in &requested {
        if !h.is_string(id) || h.trim_length(id)? == 0.0 {
            return raise(
                h,
                u("Paradex native cancel batch requires two non-empty order IDs"),
            );
        }
    }
    if requested
        .iter()
        .enumerate()
        .any(|(i, id)| requested[..i].iter().any(|old| h.equal(id, old)))
    {
        return raise(
            h,
            cat(&[
                &u("Paradex native cancel batch received duplicate order IDs ["),
                &join(h, &requested, ", ")?,
                &u("]"),
            ]),
        );
    }
    let config = h.object(vec![(
        "data",
        h.object(vec![("order_ids", h.array(requested.clone()))]),
    )]);
    let response = h.delete(u("/orders/batch"), Some(config))?.await?;
    let data = h.get(&response, "data")?;
    if !h.truthy(&data) || !h.is_object(&data) || h.is_array(&data) {
        return malformed(h, u("expected an object envelope"));
    }
    if !h.is_array(&h.get(&data, "results")?) {
        return malformed(h, u("results must be an array"));
    }
    let rows = h.get(&data, "results")?;
    let mut seen = Vec::new();
    let mut rejected = Vec::new();
    let mut anomalies = Vec::new();
    let iterator = h.entries(&rows)?;
    while let Some((index, row)) = h.next_entry(&iterator)? {
        let body: Result<(), H::Error> = (|| {
            let label = h.template(&index)?;
            if !h.truthy(&row) || !h.is_object(&row) || h.is_array(&row) {
                anomalies.push(h.text_value(cat(&[
                    &u("results["),
                    &label,
                    &u("] must be an object"),
                ])));
                return Ok(());
            }
            let status = h.get(&row, "status")?;
            let id = h.get(&row, "id")?;
            if !h.is_string(&id) || h.trim_length(&id)? == 0.0 {
                anomalies.push(h.text_value(cat(&[
                    &u("results["),
                    &label,
                    &u(if same(h, &status, "NOT_FOUND") {
                        "] has uncorrelated NOT_FOUND without an order id"
                    } else {
                        "] is missing its order id"
                    }),
                ])));
                return Ok(());
            }
            if !requested.iter().any(|v| h.equal(v, &id)) {
                anomalies.push(h.text_value(cat(&[
                    &u("results["),
                    &label,
                    &u("] has unknown order id "),
                    &h.template(&id)?,
                ])));
                return Ok(());
            }
            if seen.iter().any(|v| h.equal(v, &id)) {
                anomalies.push(h.text_value(cat(&[
                    &u("results["),
                    &label,
                    &u("] repeats order id "),
                    &h.template(&id)?,
                ])));
                return Ok(());
            }
            seen.push(id.clone());
            if same(h, &status, "QUEUED_FOR_CANCELLATION") || same(h, &status, "ALREADY_CLOSED") {
                return Ok(());
            }
            if same(h, &status, "NOT_FOUND") {
                rejected.push(id);
                return Ok(());
            }
            anomalies.push(h.text_value(cat(&[
                &u("results["),
                &label,
                &u("] has unsupported status "),
                &h.string(&status)?,
            ])));
            Ok(())
        })();
        if let Err(error) = body {
            return Err(h.close_iterator(&iterator, error));
        }
    }
    if h.length(&h.get(&data, "results")?)? != requested.len() as f64 {
        anomalies.push(h.text_value(cat(&[
            &u("received "),
            &num(h.length(&h.get(&data, "results")?)?),
            &u(" results for "),
            &num(requested.len() as f64),
            &u(" requested IDs"),
        ])));
    }
    let missing: Vec<_> = requested
        .iter()
        .filter(|id| !seen.iter().any(|v| h.equal(v, id)))
        .cloned()
        .collect();
    if !missing.is_empty() {
        anomalies.push(h.text_value(cat(&[
            &u("missing requested IDs ["),
            &join(h, &missing, ", ")?,
            &u("]"),
        ])));
    }
    if !anomalies.is_empty() {
        return malformed(h, join(h, &anomalies, "; ")?);
    }
    if !rejected.is_empty() {
        return raise(
            h,
            cat(&[
                &u("Paradex batch cancel rejected order IDs ["),
                &join(h, &rejected, ", ")?,
                &u("]: NOT_FOUND"),
            ]),
        );
    }
    let logger = h.select_log()?;
    h.log(
        logger,
        cat(&[
            &u("Cancelled order batch ["),
            &join(h, &requested, ", ")?,
            &u("]"),
        ]),
        "debug",
    )
}
