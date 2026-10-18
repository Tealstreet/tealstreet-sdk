//! Order preflight, native batch/bracket execution and construction boundaries.
use crate::account_rest::AccountRestHost;
pub type BookCleanup<'a, V, E> = Box<dyn FnMut() -> Result<V, E> + 'a>;
pub trait OrderExecutionHost: AccountRestHost {
    fn map_indexed(
        &self,
        value: Self::Value,
        callback: &mut dyn FnMut(Self::Value, f64) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn find(
        &self,
        value: Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn arguments(&self, value: Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn sync_call(
        &self,
        selected: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn write_state(&self, name: &str, value: Self::Value) -> Result<(), Self::Error>;
    fn observe_rejection(
        &self,
        promise: Self::Value,
        symbol: Self::Value,
    ) -> Result<(), Self::Error>;
}
macro_rules! invoke {
    ($h:expr,$name:expr,$args:expr) => {{
        let selected = $h.select($name)?;
        $h.invoke(selected, $args)?.await?
    }};
}
macro_rules! sync {
    ($h:expr,$name:expr,$args:expr) => {{
        let selected = $h.select($name)?;
        $h.sync_call(selected, $args)?
    }};
}
macro_rules! log{($h:expr,$level:expr,$($part:expr),+ $(,)?)=>{{let logger=$h.select_log()?;let mut text=vec![];$(text.extend($h.template(&$part)?);)+$h.log(logger,text,$level)}};}
fn call<H: OrderExecutionHost>(
    h: &H,
    name: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.intrinsic(name, args)
}
fn cat<H: OrderExecutionHost>(h: &H, values: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let mut units = vec![];
    for v in values {
        units.extend(h.template(&v)?);
    }
    call(
        h,
        "text",
        vec![h.array(units.into_iter().map(|v| h.number(v as f64)).collect())],
    )
}
fn eq<H: OrderExecutionHost>(h: &H, v: H::Value, s: &str) -> bool {
    h.equal(&v, &h.literal(s))
}
fn number<H: OrderExecutionHost>(h: &H, v: H::Value) -> Result<f64, H::Error> {
    h.numeric(&v)
}
fn finite<H: OrderExecutionHost>(h: &H, v: &H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&call(h, "Number.isFinite", vec![v.clone()])?))
}
fn positive<H: OrderExecutionHost>(h: &H, v: &H::Value) -> Result<bool, H::Error> {
    Ok(h.kind(v) == "number" && finite(h, v)? && number(h, v.clone())? > 0.0)
}
fn len<H: OrderExecutionHost>(h: &H, v: &H::Value) -> Result<f64, H::Error> {
    number(h, h.get(v, "length")?)
}
fn either<H: OrderExecutionHost>(
    h: &H,
    v: H::Value,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { f() }
}
fn nullish<H: OrderExecutionHost>(
    h: &H,
    v: H::Value,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.nullish(&v) { f() } else { Ok(v) }
}
fn state_get<H: OrderExecutionHost>(h: &H, state: &str, key: &str) -> Result<H::Value, H::Error> {
    h.get(&h.state(state)?, key)
}
fn member<H: OrderExecutionHost>(
    h: &H,
    value: H::Value,
    key: H::Value,
) -> Result<H::Value, H::Error> {
    call(h, "member", vec![value, key])
}
fn string<H: OrderExecutionHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    call(h, "String", vec![v])
}
fn upper<H: OrderExecutionHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    call(h, "String.upper", vec![v])
}
fn to_string<H: OrderExecutionHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    call(h, "method.toString", vec![v])
}
fn omit<H: OrderExecutionHost>(
    h: &H,
    fields: Vec<(&'static str, H::Value)>,
) -> Result<H::Value, H::Error> {
    call(h, "omitUndefined", vec![h.object(fields)])
}
fn fail<H: OrderExecutionHost, T>(h: &H, text: H::Value) -> Result<T, H::Error> {
    Err(h.make_error(h.template(&text)?)?)
}
fn reject<H: OrderExecutionHost, T>(h: &H, text: H::Value) -> Result<T, H::Error> {
    h.emit(text.clone())?;
    fail(h, text)
}
fn fraction<H: OrderExecutionHost>(h: &H, value: H::Value) -> Result<H::Value, H::Error> {
    let n = number(h, call(h, "Number", vec![value])?)?;
    Ok(if !n.is_finite() || n <= 0.0 {
        h.undefined()
    } else {
        h.number(if n > 1.0 { n / 100.0 } else { n })
    })
}
fn rounded<H: OrderExecutionHost>(
    h: &H,
    price: f64,
    step: f64,
    up: bool,
) -> Result<H::Value, H::Error> {
    call(
        h,
        "roundPriceToStep",
        vec![
            h.number(price),
            h.number(step),
            h.literal(if up { "up" } else { "down" }),
        ],
    )
}
fn trigger<H: OrderExecutionHost>(h: &H, value: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&call(h, "isTrigger", vec![value])?))
}
fn headers_args<H: OrderExecutionHost>(
    h: &H,
    route: H::Value,
    payload: H::Value,
    headers: H::Value,
) -> Result<Vec<H::Value>, H::Error> {
    let mut args = vec![route, payload];
    args.extend(h.arguments(call(h, "signerHeaders", vec![headers])?)?);
    Ok(args)
}

pub async fn resolve_market_ioc_price<H: OrderExecutionHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if !eq(h, h.get(&opts, "type")?, "market")
        || h.kind(&h.get(&opts, "maxSlippagePct")?) != "number"
        || !finite(h, &h.get(&opts, "maxSlippagePct")?)?
    {
        return Ok(h.undefined());
    }
    let tif = h.get(&opts, "timeInForce")?;
    if !h.is_undefined(&tif) && !eq(h, tif.clone(), "ImmediateOrCancel") && !eq(h, tif, "IOC") {
        return fail(
            h,
            h.literal("Paradex maxSlippagePct market orders require IOC time-in-force"),
        );
    }
    let book = invoke!(
        h,
        "fetchOrderBook",
        vec![h.get(&opts, "symbol")?, h.number(5.0)]
    );
    let touch = if eq(h, h.get(&opts, "side")?, "buy") {
        h.optional(&h.get(&h.get(&book, "asks")?, "0")?, "price")?
    } else {
        h.optional(&h.get(&h.get(&book, "bids")?, "0")?, "price")?
    };
    if !finite(h, &touch)? || !h.truthy(&touch) || number(h, touch.clone())? <= 0.0 {
        return fail(
            h,
            cat(
                h,
                vec![
                    h.literal("Paradex "),
                    h.get(&opts, "side")?,
                    h.literal(" market IOC order requires a live "),
                    h.literal(if eq(h, h.get(&opts, "side")?, "buy") {
                        "ask"
                    } else {
                        "bid"
                    }),
                ],
            )?,
        );
    }
    let m = member(
        h,
        state_get(h, "store", "marketsBySymbol")?,
        h.get(&opts, "symbol")?,
    )?;
    let ticker = h.find(state_get(h, "store", "tickers")?, &mut |t| {
        Ok(h.equal(&h.get(&t, "symbol")?, &h.get(&opts, "symbol")?)
            || h.equal(&h.get(&t, "id")?, &h.get(&opts, "symbol")?))
    })?;
    let midpoint = if positive(h, &h.optional(&ticker, "bid")?)?
        && positive(h, &h.optional(&ticker, "ask")?)?
    {
        h.number(
            number(
                h,
                call(
                    h,
                    "add",
                    vec![h.get(&ticker, "bid")?, h.get(&ticker, "ask")?],
                )?,
            )? / 2.0,
        )
    } else {
        h.undefined()
    };
    let candidates = h.array(vec![
        h.optional(&ticker, "mark")?,
        h.optional(&ticker, "index")?,
        h.optional(&ticker, "midPrice")?,
        h.optional(&ticker, "last")?,
        midpoint,
        touch.clone(),
    ]);
    let fair = h.find(candidates, &mut |v| positive(h, &v))?;
    let band = nullish(
        h,
        fraction(
            h,
            h.optional(&h.optional(&m, "info")?, "price_bands_width")?,
        )?,
        || fraction(h, h.optional(&h.optional(&m, "info")?, "max_slippage")?),
    )?;
    let slip = number(
        h,
        call(
            h,
            "Math.max",
            vec![h.number(0.0), h.get(&opts, "maxSlippagePct")?],
        )?,
    )? / 100.0;
    let step = number(
        h,
        call(
            h,
            "Number",
            vec![either(
                h,
                h.optional(&h.optional(&m, "precision")?, "price")?,
                || Ok(h.number(0.000001)),
            )?],
        )?,
    )?;
    let limit = if !h.is_undefined(&fair) && !h.is_undefined(&band) {
        Some(
            number(h, fair.clone())?
                * if eq(h, h.get(&opts, "side")?, "buy") {
                    1.0 + number(h, band)?
                } else {
                    1.0 - number(h, band)?
                },
        )
    } else {
        None
    };
    let mut price = number(
        h,
        if h.nullish(&fair) {
            touch.clone()
        } else {
            fair
        },
    )? * if eq(h, h.get(&opts, "side")?, "buy") {
        1.0 + slip
    } else {
        1.0 - slip
    };
    if let Some(limit) = limit {
        price = number(
            h,
            call(
                h,
                if eq(h, h.get(&opts, "side")?, "buy") {
                    "Math.min"
                } else {
                    "Math.max"
                },
                vec![h.number(price), h.number(limit)],
            )?,
        )?;
    }
    let inside = match limit {
        None => true,
        Some(limit) => {
            if eq(h, h.get(&opts, "side")?, "buy") {
                number(h, touch.clone())? <= limit
            } else {
                number(h, touch.clone())? >= limit
            }
        }
    };
    if inside {
        let misses = if eq(h, h.get(&opts, "side")?, "buy") {
            price < number(h, touch.clone())?
        } else {
            price > number(h, touch.clone())?
        };
        if misses {
            price = number(h, touch)?;
        }
    }
    let mut result = rounded(h, price, step, eq(h, h.get(&opts, "side")?, "buy"))?;
    if let Some(limit) = limit {
        if eq(h, h.get(&opts, "side")?, "buy") && number(h, result.clone())? > limit {
            result = rounded(h, limit, step, false)?;
        }
        if eq(h, h.get(&opts, "side")?, "sell") && number(h, result.clone())? < limit {
            result = rounded(h, limit, step, true)?;
        }
    }
    if !finite(h, &result)? || number(h, result.clone())? <= 0.0 {
        return fail(
            h,
            cat(
                h,
                vec![
                    h.literal("Paradex "),
                    h.get(&opts, "side")?,
                    h.literal(" market IOC order produced invalid execution price "),
                    result,
                ],
            )?,
        );
    }
    Ok(result)
}
pub async fn build_main_order_payload<H: OrderExecutionHost>(
    h: &H,
    opts: H::Value,
    client: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.state("requestSigner")?) {
        return fail(
            h,
            h.literal("Cannot place order: Request signer not initialized"),
        );
    }
    if eq(h, h.get(&opts, "type")?, "trailing_stop_market") {
        return reject(
            h,
            h.literal("Paradex trailing-stop orders are not supported by the current order schema"),
        );
    }
    let opening = eq(h, h.get(&opts, "type")?, "conditional");
    if opening {
        for (key, text) in [
            (
                "price",
                "Paradex opening trigger orders require a finite positive execution price",
            ),
            (
                "triggerPrice",
                "Paradex opening trigger orders require a finite positive trigger price",
            ),
        ] {
            if !finite(h, &h.get(&opts, key)?)?
                || !h.truthy(&h.get(&opts, key)?)
                || number(h, h.get(&opts, key)?)? <= 0.0
            {
                return reject(h, h.literal(text));
            }
        }
        if h.truthy(&h.get(&opts, "reduceOnly")?) {
            return reject(h,h.literal("Paradex opening trigger orders cannot be reduce-only; use a stop-loss order instead"));
        }
        if !h.is_undefined(&h.get(&opts, "takeProfit")?)
            || !h.is_undefined(&h.get(&opts, "stopLoss")?)
        {
            return reject(h,h.literal("Paradex cannot safely attach TP/SL children before an opening trigger order fills"));
        }
    }
    let side = h.literal(if eq(h, h.get(&opts, "side")?, "buy") {
        "BUY"
    } else {
        "SELL"
    });
    let mut kind = sync!(h, "mapOrderType", vec![h.get(&opts, "type")?]);
    let mut price = h.get(&opts, "price")?;
    let mut tif = h.get(&opts, "timeInForce")?;
    let ioc = invoke!(h, "resolveIocPrice", vec![opts.clone()]);
    if !h.is_undefined(&ioc) {
        kind = h.literal("LIMIT");
        price = ioc;
        tif = h.literal("ImmediateOrCancel");
    }
    let stop = trigger(h, kind.clone())?;
    let limit = eq(h, kind.clone(), "LIMIT")
        || eq(h, kind.clone(), "STOP_LIMIT")
        || h.truthy(&call(
            h,
            "endsWith",
            vec![kind.clone(), h.literal("_LIMIT")],
        )?);
    let trigger_price = if opening {
        h.get(&opts, "triggerPrice")?
    } else if stop {
        price.clone()
    } else {
        h.get(&opts, "triggerPrice")?
    };
    let params = h.object(vec![
        ("market", h.get(&opts, "symbol")?),
        ("side", side.clone()),
        ("type", kind.clone()),
        ("size", to_string(h, h.get(&opts, "amount")?)?),
    ]);
    if limit && h.truthy(&price) {
        h.write(&params, "price", to_string(h, price.clone())?)?;
    }
    let signed = invoke!(h, "signOrder", vec![params]);
    let signature = h.get(&signed, "signature")?;
    let timestamp = h.get(&signed, "signature_timestamp")?;
    let headers = h.get(&signed, "headers")?;
    let instruction = sync!(h, "mapTimeInForce", vec![tif]);
    let reduce = either(h, h.get(&opts, "reduceOnly")?, || {
        Ok(h.boolean(stop && !opening))
    })?;
    let payload = omit(
        h,
        vec![
            ("market", h.get(&opts, "symbol")?),
            ("side", side),
            ("type", kind),
            ("size", to_string(h, h.get(&opts, "amount")?)?),
            (
                "price",
                if limit && h.truthy(&price) {
                    to_string(h, price)?
                } else {
                    h.undefined()
                },
            ),
            (
                "trigger_price",
                if h.nullish(&trigger_price) {
                    h.undefined()
                } else {
                    to_string(h, trigger_price)?
                },
            ),
            ("instruction", instruction),
            (
                "flags",
                if h.truthy(&reduce) {
                    h.array(vec![h.literal("REDUCE_ONLY")])
                } else {
                    h.undefined()
                },
            ),
            ("stp", h.literal("EXPIRE_TAKER")),
            ("signature", signature),
            ("signature_timestamp", timestamp),
            ("client_id", client),
        ],
    )?;
    Ok(h.object(vec![("payload", payload), ("headers", headers)]))
}
fn error_message<H: OrderExecutionHost>(
    h: &H,
    e: &H::Error,
    api: bool,
) -> Result<H::Value, H::Error> {
    let v = h.error_value(e);
    if api {
        let data = h.optional(&h.optional(&v, "response")?, "data")?;
        let message = h.optional(&data, "message")?;
        if h.truthy(&message) {
            return Ok(message);
        }
        let error = h.optional(&h.optional(&h.optional(&v, "response")?, "data")?, "error")?;
        if h.truthy(&error) {
            return Ok(error);
        }
    }
    either(h, h.optional(&v, "message")?, || string(h, v.clone()))
}
fn malformed<H: OrderExecutionHost, T>(h: &H, message: H::Value) -> Result<T, H::Error> {
    reject(
        h,
        cat(
            h,
            vec![
                h.literal("Paradex batch order response malformed: "),
                message,
            ],
        )?,
    )
}
pub async fn place_native_batch_chunk<H: OrderExecutionHost>(
    h: &H,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    if len(h, &orders)? < 1.0 || len(h, &orders)? > 10.0 {
        return reject(h,cat(h,vec![h.literal("Paradex native order batch must contain between 1 and 10 orders; received "),h.get(&orders,"length")?])?);
    }
    if h.truthy(&h.state("sdkInitPromise")?) {
        invoke!(h, "awaitSDK", vec![h.state("sdkInitPromise")?]);
    }
    let clients = h.array(vec![]);
    let payloads = h.array(vec![]);
    let mut headers = h.undefined();
    let build=async{
  if h.truthy(&state_get(h,"options","payloadSigner")?)&&len(h,&orders)?>1.0{return fail(h,h.literal("Paradex batch orders are not supported with payloadSigner, which signs one order per request"));}
  h.for_each(orders.clone(),&mut |order|{
   if !h.truthy(&h.get(&order,"symbol")?)||!finite(h,&h.get(&order,"amount")?)?||number(h,h.get(&order,"amount")?)?<=0.0{return fail(h,h.literal("Paradex batch LIMIT order requires a symbol and a finite positive amount"));}
   if !finite(h,&h.get(&order,"price")?)?||!h.truthy(&h.get(&order,"price")?)||number(h,h.get(&order,"price")?)?<=0.0{return fail(h,h.literal("Paradex batch LIMIT order requires a finite positive price"));}Ok(())
  })?;
  let seen=call(h,"new Set",vec![])?;let mut index=0usize;
  while (index as f64)<len(h,&orders)?{
   let client=nullish(h,h.get(&h.get(&orders,&index.to_string())?,"clientOrderId")?,||Ok(sync!(h,"generateClientId",vec![])))?;
   if h.truthy(&call(h,"Set.has",vec![seen.clone(),client.clone()])?){return fail(h,cat(h,vec![h.literal("Paradex batch generated an invalid or duplicate client_id at index "),h.number(index as f64)])?);}
   call(h,"Set.add",vec![seen.clone(),client.clone()])?;call(h,"push",vec![clients.clone(),client])?;index+=1;
  }
  index=0;
  while (index as f64)<len(h,&orders)?{
   let built=invoke!(h,"buildOrder",vec![h.get(&orders,&index.to_string())?,h.get(&clients,&index.to_string())?]);
   let payload=h.get(&built,"payload")?;let head=h.get(&built,"headers")?;
   call(h,"push",vec![payloads.clone(),payload])?;headers=head;index+=1;
  }
  Ok::<_,H::Error>(())
 }.await;
    if let Err(error) = build {
        h.emit(error_message(h, &error, false)?)?;
        return Err(error);
    }
    let post = async {
        let selected = h.select("post")?;
        let args = headers_args(h, h.literal("/orders/batch"), payloads, headers)?;
        let response = h.invoke(selected, args)?.await?;
        h.get(&response, "data")
    }
    .await;
    let data = match post {
        Ok(data) => data,
        Err(error) => {
            h.emit(error_message(h, &error, true)?)?;
            return Err(error);
        }
    };
    if !h.truthy(&data) || !h.is_object(&data) || h.is_array(&data) {
        return malformed(h, h.literal("expected an object envelope"));
    }
    for key in ["orders", "errors"] {
        if !h.is_undefined(&h.get(&data, key)?) && !h.is_array(&h.get(&data, key)?) {
            return malformed(
                h,
                h.literal(&format!("{key} must be an array when present")),
            );
        }
    }
    let expected = call(h, "Set.from", vec![clients.clone()])?;
    let map = call(h, "new Map", vec![])?;
    let order_rows = either(h, h.get(&data, "orders")?, || Ok(h.array(vec![])))?;
    let entries = call(h, "entries", vec![order_rows])?;
    h.for_each(entries, &mut |entry| {
        let index = h.get(&entry, "0")?;
        let row = h.get(&entry, "1")?;
        let prefix = cat(h, vec![h.literal("orders["), index, h.literal("]")])?;
        if !h.truthy(&row) || !h.is_object(&row) || h.is_array(&row) {
            return malformed(h, cat(h, vec![prefix, h.literal(" must be an object")])?);
        }
        if h.kind(&h.get(&row, "client_id")?) != "string"
            || !h.truthy(&call(
                h,
                "Set.has",
                vec![expected.clone(), h.get(&row, "client_id")?],
            )?)
        {
            return malformed(
                h,
                cat(
                    h,
                    vec![prefix, h.literal(" has an unknown or missing client_id")],
                )?,
            );
        }
        if h.truthy(&call(
            h,
            "Map.has",
            vec![map.clone(), h.get(&row, "client_id")?],
        )?) {
            return malformed(
                h,
                cat(
                    h,
                    vec![
                        prefix,
                        h.literal(" repeats client_id "),
                        h.get(&row, "client_id")?,
                    ],
                )?,
            );
        }
        if h.kind(&h.get(&row, "id")?) != "string" || len(h, &h.get(&row, "id")?)? == 0.0 {
            return malformed(
                h,
                cat(
                    h,
                    vec![prefix, h.literal(" is missing its Paradex order id")],
                )?,
            );
        }
        call(
            h,
            "Map.set",
            vec![map.clone(), h.get(&row, "client_id")?, h.get(&row, "id")?],
        )?;
        Ok(())
    })?;
    let errors = either(h, h.get(&data, "errors")?, || Ok(h.array(vec![])))?;
    let decoded = h.map_indexed(errors, &mut |row, index| {
        let prefix = cat(
            h,
            vec![h.literal("errors["), h.number(index), h.literal("]")],
        )?;
        if !h.truthy(&row) || !h.is_object(&row) || h.is_array(&row) {
            return malformed(h, cat(h, vec![prefix, h.literal(" must be an object")])?);
        }
        let error = h.get(&row, "error")?;
        let message = h.get(&row, "message")?;
        if (!h.is_undefined(&error) && h.kind(&error) != "string")
            || (!h.is_undefined(&message) && h.kind(&message) != "string")
        {
            return malformed(
                h,
                cat(
                    h,
                    vec![
                        prefix,
                        h.literal(" must contain only string error/message fields"),
                    ],
                )?,
            );
        }
        if !h.truthy(&error) && !h.truthy(&message) {
            return malformed(
                h,
                cat(
                    h,
                    vec![prefix, h.literal(" does not contain an error or message")],
                )?,
            );
        }
        call(
            h,
            "filter.join",
            vec![h.array(vec![error, message]), h.literal(": ")],
        )
    })?;
    let missing = h.filter(clients.clone(), &mut |client| {
        Ok(!h.truthy(&call(h, "Map.has", vec![map.clone(), client])?))
    })?;
    if len(h, &decoded)? > 0.0 {
        if len(h, &missing)? != len(h, &decoded)? {
            return malformed(
                h,
                cat(
                    h,
                    vec![
                        h.get(&map, "size")?,
                        h.literal(" successes plus "),
                        h.get(&decoded, "length")?,
                        h.literal(" errors do not account for "),
                        h.get(&clients, "length")?,
                        h.literal(" requests"),
                    ],
                )?,
            );
        }
        return reject(
            h,
            cat(
                h,
                vec![
                    h.literal("Paradex batch rejected client IDs ["),
                    call(h, "join", vec![missing, h.literal(", ")])?,
                    h.literal("]: "),
                    call(h, "join", vec![decoded, h.literal("; ")])?,
                ],
            )?,
        );
    }
    if len(h, &missing)? > 0.0 || number(h, h.get(&map, "size")?)? != len(h, &clients)? {
        return malformed(
            h,
            cat(
                h,
                vec![
                    h.literal("missing results for client IDs ["),
                    call(h, "join", vec![missing, h.literal(", ")])?,
                    h.literal("]"),
                ],
            )?,
        );
    }
    h.map(clients, &mut |client| {
        call(h, "Map.get", vec![map.clone(), client])
    })
}
pub async fn place_take_profit_stop_loss<H: OrderExecutionHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let ids = h.array(vec![]);
    if !h.truthy(&h.get(&opts, "amount")?) || h.equal(&h.get(&opts, "amount")?, &h.number(0.0)) {
        let message=h.literal("Paradex does not support infinite TP/SL orders. Please specify an explicit order size for TP/SL.");
        log!(h, "error", message.clone())?;
        return reject(h, message);
    }
    if !h.truthy(&sync!(h, "validateTpSl", vec![opts.clone()])) {
        log!(
            h,
            "warning",
            h.literal("Invalid TP/SL prices, skipping bracket orders")
        )?;
        return Ok(ids);
    }
    if !h.truthy(&h.state("requestSigner")?) {
        log!(
            h,
            "warning",
            h.literal("Cannot place TP/SL orders: Request signer not initialized")
        )?;
        return Ok(ids);
    }
    let side = h.literal(if eq(h, h.get(&opts, "side")?, "buy") {
        "SELL"
    } else {
        "BUY"
    });
    let result = async {
        for (key, kind, label) in [
            ("takeProfit", "TAKE_PROFIT_MARKET", "Take profit"),
            ("stopLoss", "STOP_LOSS_MARKET", "Stop loss"),
        ] {
            if !h.truthy(&h.get(&opts, key)?) {
                continue;
            }
            let params = h.object(vec![
                ("market", h.get(&opts, "symbol")?),
                ("side", side.clone()),
                ("type", h.literal(kind)),
                ("size", to_string(h, h.get(&opts, "amount")?)?),
            ]);
            let signed = invoke!(h, "signOrder", vec![params]);
            let signature = h.get(&signed, "signature")?;
            let timestamp = h.get(&signed, "signature_timestamp")?;
            let headers = h.get(&signed, "headers")?;
            let payload = omit(
                h,
                vec![
                    ("market", h.get(&opts, "symbol")?),
                    ("side", side.clone()),
                    ("type", h.literal(kind)),
                    ("size", to_string(h, h.get(&opts, "amount")?)?),
                    ("trigger_price", to_string(h, h.get(&opts, key)?)?),
                    ("instruction", h.literal("GTC")),
                    ("flags", h.array(vec![h.literal("REDUCE_ONLY")])),
                    ("signature", signature),
                    ("signature_timestamp", timestamp),
                ],
            )?;
            let selected = h.select("post")?;
            let args = headers_args(h, h.literal("/orders"), payload, headers)?;
            let response = h.invoke(selected, args)?.await?;
            let data = h.get(&response, "data")?;
            call(h, "push", vec![ids.clone(), h.get(&data, "id")?])?;
            log!(
                h,
                "info",
                h.literal(&format!("{label} order placed: ")),
                h.get(&data, "id")?,
                h.literal(" @ "),
                h.get(&opts, key)?
            )?;
        }
        Ok::<_, H::Error>(())
    }
    .await;
    if let Err(error) = result {
        log!(
            h,
            "warning",
            h.literal("Error placing TP/SL orders: "),
            h.error_value(&error)
        )?;
        let v = h.error_value(&error);
        if h.truthy(&h.optional(&h.optional(&v, "response")?, "data")?) {
            let mut message = h.get(&v, "message")?;
            if h.truthy(&h.get(&h.get(&h.get(&v, "response")?, "data")?, "error")?)
                && h.truthy(&h.get(&h.get(&h.get(&v, "response")?, "data")?, "message")?)
            {
                message = cat(
                    h,
                    vec![
                        h.get(&h.get(&h.get(&v, "response")?, "data")?, "error")?,
                        h.literal(": "),
                        h.get(&h.get(&h.get(&v, "response")?, "data")?, "message")?,
                    ],
                )?;
            } else if h.truthy(&h.get(&h.get(&h.get(&v, "response")?, "data")?, "message")?) {
                message = h.get(&h.get(&h.get(&v, "response")?, "data")?, "message")?;
            } else if h.truthy(&h.get(&h.get(&h.get(&v, "response")?, "data")?, "error")?) {
                message = h.get(&h.get(&h.get(&v, "response")?, "data")?, "error")?;
            }
            h.emit(cat(h, vec![h.literal("TP/SL order failed: "), message])?)?;
        }
    }
    Ok(ids)
}
fn update_error<H: OrderExecutionHost, T>(
    h: &H,
    order: &H::Value,
    suffix: &str,
) -> Result<T, H::Error> {
    reject(
        h,
        cat(
            h,
            vec![
                h.literal("Paradex cannot safely amend "),
                h.get(order, "id")?,
                h.literal(": "),
                h.literal(suffix),
            ],
        )?,
    )
}
pub async fn update_order<H: OrderExecutionHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let opts = call(h, "destructureUpdate", vec![opts])?;
    let order = h.get(&opts, "order")?;
    let update = h.get(&opts, "update")?;
    if !h.truthy(&h.state("requestSigner")?) {
        return reject(
            h,
            h.literal("Paradex cannot amend an order before the request signer is initialized"),
        );
    }
    let m = member(
        h,
        state_get(h, "store", "marketsBySymbol")?,
        h.get(&order, "symbol")?,
    )?;
    if !h.truthy(&m) {
        return reject(
            h,
            cat(
                h,
                vec![
                    h.literal("Paradex cannot safely amend "),
                    h.get(&order, "id")?,
                    h.literal(": market "),
                    h.get(&order, "symbol")?,
                    h.literal(" is not reconciled"),
                ],
            )?,
        );
    }
    let raw = h.get(&order, "info")?;
    let id = if h.is_undefined(&h.optional(&raw, "id")?)
        || h.equal(&h.optional(&raw, "id")?, &call(h, "null", vec![])?)
    {
        h.literal("")
    } else {
        string(h, h.get(&raw, "id")?)?
    };
    let market = string(
        h,
        either(h, h.optional(&raw, "market")?, || Ok(h.literal("")))?,
    )?;
    let side = upper(
        h,
        either(h, h.optional(&raw, "side")?, || Ok(h.literal("")))?,
    )?;
    let kind = upper(
        h,
        either(h, h.optional(&raw, "type")?, || Ok(h.literal("")))?,
    )?;
    let status = upper(
        h,
        either(h, h.optional(&raw, "status")?, || Ok(h.literal("")))?,
    )?;
    let expected = h.literal(if eq(h, h.get(&order, "side")?, "buy") {
        "BUY"
    } else {
        "SELL"
    });
    let triggered = trigger(h, kind.clone())?;
    let limit = eq(h, kind.clone(), "LIMIT")
        && (eq(h, status.clone(), "NEW") || eq(h, status.clone(), "OPEN"));
    let tpsl = triggered && eq(h, status.clone(), "UNTRIGGERED");
    if !limit && !tpsl {
        return reject(h,cat(h,vec![h.literal("Paradex native amendment supports only exactly reconciled open LIMIT or untriggered TPSL orders; "),either(h,kind.clone(),||Ok(h.literal("unknown")))?,h.literal(" "),either(h,status,||Ok(h.literal("unknown")))?,h.literal(" is unsupported")])?);
    }
    let reduce = if h.kind(&h.optional(&raw, "reduce_only")?) == "boolean" {
        h.get(&raw, "reduce_only")?
    } else if h.is_array(&h.optional(&raw, "flags")?) {
        call(
            h,
            "array.includes",
            vec![h.get(&raw, "flags")?, h.literal("REDUCE_ONLY")],
        )?
    } else {
        h.undefined()
    };
    if !h.equal(&id, &h.get(&order, "id")?)
        || !h.equal(&market, &h.get(&order, "symbol")?)
        || !h.equal(&side, &expected)
        || !h.equal(
            &sync!(h, "reverseMapOrderType", vec![kind.clone(), reduce.clone()]),
            &h.get(&order, "type")?,
        )
    {
        return update_error(
            h,
            &order,
            "order identity or immutable shape is not reconciled",
        );
    }
    if h.truthy(&h.optional(&raw, "on_behalf_of_account")?) {
        return update_error(h, &order, "isolated-margin order routing is unsupported");
    }
    let instruction = upper(
        h,
        either(h, h.optional(&raw, "instruction")?, || Ok(h.literal("")))?,
    )?;
    let known = h.truthy(&call(
        h,
        "literal.includes.instructions",
        vec![instruction],
    )?);
    let legacy = h.kind(&h.optional(&raw, "post_only")?) == "boolean"
        || h.truthy(&call(
            h,
            "literal.includes.legacy",
            vec![string(
                h,
                either(h, h.optional(&raw, "time_in_force")?, || Ok(h.literal("")))?,
            )?],
        )?);
    if !known && !legacy {
        return update_error(h, &order, "time-in-force/post-only state is not reconciled");
    }
    if h.is_undefined(&reduce) || !h.equal(&reduce, &h.get(&order, "reduceOnly")?) {
        return update_error(h, &order, "reduce-only state is not reconciled");
    }
    let size = call(h, "Number", vec![h.optional(&raw, "size")?])?;
    let remaining = call(h, "Number", vec![h.optional(&raw, "remaining_size")?])?;
    let price = call(h, "Number", vec![h.optional(&raw, "price")?])?;
    let trigger_price = call(h, "Number", vec![h.optional(&raw, "trigger_price")?])?;
    let filled = number(h, size.clone())? - number(h, remaining.clone())?;
    if !finite(h, &size)?
        || number(h, size.clone())? <= 0.0
        || !finite(h, &remaining)?
        || number(h, remaining.clone())? < 0.0
        || number(h, remaining.clone())? > number(h, size.clone())?
        || !h.equal(&size, &h.get(&order, "amount")?)
        || !h.equal(&remaining, &h.get(&order, "remaining")?)
        || !h.equal(&h.number(filled), &h.get(&order, "filled")?)
    {
        return update_error(
            h,
            &order,
            "total/filled/remaining base size is not reconciled",
        );
    }
    if !finite(h, &price)?
        || number(h, price.clone())? < 0.0
        || (limit && number(h, price.clone())? <= 0.0)
    {
        return update_error(h, &order, "existing limit price is not reconciled");
    }
    if limit && !h.equal(&price, &h.get(&order, "price")?) {
        return update_error(h, &order, "displayed limit price is not reconciled");
    }
    if tpsl
        && (!finite(h, &trigger_price)?
            || number(h, trigger_price.clone())? <= 0.0
            || !h.equal(&trigger_price, &h.get(&order, "price")?))
    {
        return update_error(h, &order, "TPSL trigger price is not reconciled");
    }
    let amended_size = call(
        h,
        "adjust",
        vec![
            if h.truthy(&call(h, "has", vec![update.clone(), h.literal("amount")])?) {
                h.get(&update, "amount")?
            } else {
                size
            },
            h.get(&h.get(&m, "precision")?, "amount")?,
        ],
    )?;
    let amended_price = if limit {
        call(
            h,
            "adjust",
            vec![
                if h.truthy(&call(h, "has", vec![update.clone(), h.literal("price")])?) {
                    h.get(&update, "price")?
                } else {
                    price.clone()
                },
                h.get(&h.get(&m, "precision")?, "price")?,
            ],
        )?
    } else {
        price
    };
    let amended_trigger = if tpsl {
        call(
            h,
            "adjust",
            vec![
                if h.truthy(&call(h, "has", vec![update.clone(), h.literal("price")])?) {
                    h.get(&update, "price")?
                } else {
                    trigger_price
                },
                h.get(&h.get(&m, "precision")?, "price")?,
            ],
        )?
    } else {
        h.undefined()
    };
    if !finite(h, &amended_size)? || number(h, amended_size.clone())? <= 0.0 {
        return reject(
            h,
            h.literal("Paradex amended total base size must be greater than zero"),
        );
    }
    if filled > 0.0 && number(h, amended_size.clone())? <= filled {
        return reject(
            h,
            h.literal("Paradex amended total base size must exceed the already-filled base size"),
        );
    }
    if !finite(h, &amended_price)?
        || number(h, amended_price.clone())? < 0.0
        || (limit && number(h, amended_price.clone())? <= 0.0)
    {
        return reject(h, h.literal("Paradex amended order price is invalid"));
    }
    if tpsl
        && (h.is_undefined(&amended_trigger)
            || !finite(h, &amended_trigger)?
            || number(h, amended_trigger.clone())? <= 0.0)
    {
        return reject(
            h,
            h.literal("Paradex amended TPSL trigger price must be greater than zero"),
        );
    }
    let result = async {
        let fields = h.object(vec![
            ("id", h.get(&order, "id")?),
            ("market", market),
            ("side", side),
            ("type", kind),
            ("size", string(h, amended_size)?),
            ("price", string(h, amended_price)?),
        ]);
        let signed = invoke!(h, "signModifyOrder", vec![fields.clone()]);
        let signature = h.get(&signed, "signature")?;
        let stamp = h.get(&signed, "signature_timestamp")?;
        let headers = h.get(&signed, "headers")?;
        let payload = call(h, "spread", vec![fields])?;
        h.write(
            &payload,
            "trigger_price",
            if h.is_undefined(&amended_trigger) {
                h.undefined()
            } else {
                string(h, amended_trigger)?
            },
        )?;
        h.write(&payload, "signature", signature)?;
        h.write(&payload, "signature_timestamp", stamp)?;
        let payload = call(h, "omitUndefined", vec![payload])?;
        let selected = h.select("put")?;
        let route = call(
            h,
            "replace",
            vec![
                h.literal("/orders/{id}"),
                h.literal("{id}"),
                h.get(&order, "id")?,
            ],
        )?;
        let args = headers_args(h, route, payload, headers)?;
        let response = h.invoke(selected, args)?.await?;
        let data = h.get(&response, "data")?;
        if !h.truthy(&data)
            || !h.equal(
                &string(h, either(h, h.get(&data, "id")?, || Ok(h.literal("")))?)?,
                &h.get(&order, "id")?,
            )
        {
            return fail(
                h,
                cat(
                    h,
                    vec![
                        h.literal("Paradex modify response did not confirm stable order id "),
                        h.get(&order, "id")?,
                    ],
                )?,
            );
        }
        Ok(h.array(vec![h.get(&order, "id")?]))
    }
    .await;
    if let Err(e) = &result {
        h.emit(error_message(h, e, true)?)?;
    }
    result
}
pub fn initialize_exchange<H: OrderExecutionHost>(
    h: &H,
    name: H::Value,
    opts: H::Value,
    store: H::Value,
) -> Result<H::Value, H::Error> {
    sync!(h, "super", vec![name, opts.clone(), store]);
    let xhr = sync!(
        h,
        "rateLimit",
        vec![
            sync!(h, "createAPI", vec![opts.clone(), h.state("this")?]),
            h.object(vec![("maxRPS", h.number(10.0))])
        ]
    );
    h.write_state("xhr", xhr)?;
    let public = sync!(h, "newPublicWebSocket", vec![h.state("this")?]);
    h.write_state("publicWebsocket", public)?;
    let private = sync!(h, "newPrivateWebSocket", vec![h.state("this")?]);
    h.write_state("privateWebsocket", private)?;
    if h.truthy(&h.get(&opts, "paradexAddress")?) {
        let promise = sync!(h, "initializeSDKPromise", vec![]);
        h.write_state("sdkInitPromise", promise)?;
        call(h, "observeInit", vec![h.state("sdkInitPromise")?])?;
    }
    Ok(h.undefined())
}
pub fn listen_order_book<'a, H: OrderExecutionHost>(
    h: &'a H,
    symbol: H::Value,
    callback: H::Value,
) -> Result<BookCleanup<'a, H::Value, H::Error>, H::Error> {
    let callbacks = register_order_book_listener(h, symbol.clone(), callback.clone())?;
    Ok(Box::new(move || {
        cleanup_order_book_listener(h, symbol.clone(), callbacks.clone(), callback.clone())
    }))
}
pub(crate) fn register_order_book_listener<H: OrderExecutionHost>(
    h: &H,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&call(h, "callbacks.has", vec![symbol.clone()])?) {
        call(h, "callbacks.setNew", vec![symbol.clone()])?;
    }
    let callbacks = call(h, "callbacks.get", vec![symbol.clone()])?;
    call(h, "Set.add", vec![callbacks.clone(), callback.clone()])?;
    if number(h, h.get(&callbacks, "size")?)? == 1.0 {
        sync!(h, "subscribeBook", vec![symbol.clone()]);
        if !h.truthy(&h.get(&h.state("parent")?, "isPublicDataOnly")?)
            && h.truthy(&h.get(&h.state("parent")?, "jwtManager")?)
        {
            let promise = sync!(
                h,
                "setSlippagePromise",
                vec![h.literal("0.2"), symbol.clone()]
            );
            h.observe_rejection(promise, symbol.clone())?;
        }
    }
    Ok(callbacks)
}
pub(crate) fn cleanup_order_book_listener<H: OrderExecutionHost>(
    h: &H,
    symbol: H::Value,
    callbacks: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    call(h, "Set.delete", vec![callbacks.clone(), callback.clone()])?;
    if number(h, h.get(&callbacks, "size")?)? == 0.0 {
        call(h, "callbacks.delete", vec![symbol.clone()])?;
        sync!(h, "unsubscribeBook", vec![symbol.clone()]);
        sync!(
            h,
            "cleanupStats",
            vec![cat(h, vec![h.literal("order_book."), symbol.clone()])?]
        );
    }
    Ok(h.undefined())
}
pub fn allocate_history_auth_error<P, V, E>(
    allocate: impl FnOnce(&'static str) -> Result<P, E>,
    undefined: impl FnOnce() -> V,
    define: impl FnOnce(&P, &'static str, V, bool, bool, bool) -> Result<(), E>,
) -> Result<P, E> {
    let parent = allocate("ParadexPrivateHistoryAuthNotReadyError")?;
    define(&parent, "originalError", undefined(), true, true, true)?;
    Ok(parent)
}

pub fn book_slippage_rejected<H: OrderExecutionHost>(
    h: &H,
    symbol: H::Value,
    error: H::Error,
) -> Result<H::Value, H::Error> {
    log!(
        h,
        "warning",
        h.literal("Failed to set max slippage for "),
        symbol,
        h.literal(": "),
        h.error_value(&error)
    )?;
    Ok(h.undefined())
}
