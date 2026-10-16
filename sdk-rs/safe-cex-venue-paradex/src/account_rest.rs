//! Signed account REST requests, cursor history and account preferences.
use crate::market_data::{DataFuture, MarketDataHost};
pub trait AccountRestHost: MarketDataHost {
    fn make_error(&self, message: Vec<u16>) -> Result<Self::Error, Self::Error>;
    fn kind(&self, value: &Self::Value) -> &'static str;
    fn select(&self, name: &str) -> Result<Self::Value, Self::Error>;
    fn invoke(
        &self,
        selected: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<DataFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn console(&self, args: Vec<Self::Value>) -> Result<(), Self::Error>;
    fn emit(&self, value: Self::Value) -> Result<(), Self::Error>;
    fn income(
        &self,
        row: Self::Value,
        resolve: &mut dyn FnMut(Self::Value) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn historical(&self, row: Self::Value) -> Result<Self::Value, Self::Error>;
    fn history_error(&self, error: &Self::Error) -> Result<bool, Self::Error>;
}
macro_rules! await_call {
    ($h:expr,$name:expr,$args:expr) => {{
        let selected = $h.select($name)?;
        $h.invoke(selected, $args)?.await?
    }};
}
macro_rules! log{($h:expr,$severity:expr,$($part:expr),+ $(,)?)=>{{let logger=$h.select_log()?;let mut text=Vec::new();$(text.extend($h.template(&$part)?);)+$h.log(logger,text,$severity)}};}
fn call<H: AccountRestHost>(h: &H, name: &str, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    h.intrinsic(name, args)
}
fn cat<H: AccountRestHost>(h: &H, values: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let mut units = vec![];
    for v in values {
        units.extend(h.template(&v)?);
    }
    call(
        h,
        "text",
        vec![h.array(units.into_iter().map(|n| h.number(n as f64)).collect())],
    )
}
fn either<H: AccountRestHost>(
    h: &H,
    v: H::Value,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { f() }
}
fn nullish<H: AccountRestHost>(
    h: &H,
    v: H::Value,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.nullish(&v) { f() } else { Ok(v) }
}
fn number<H: AccountRestHost>(h: &H, v: H::Value) -> Result<f64, H::Error> {
    h.numeric(&v)
}
fn length<H: AccountRestHost>(h: &H, v: &H::Value) -> Result<f64, H::Error> {
    number(h, h.get(v, "length")?)
}
fn state_get<H: AccountRestHost>(h: &H, state: &str, key: &str) -> Result<H::Value, H::Error> {
    h.get(&h.state(state)?, key)
}
fn same<H: AccountRestHost>(h: &H, v: H::Value, s: &str) -> bool {
    h.equal(&v, &h.literal(s))
}
fn string<H: AccountRestHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    call(h, "String", vec![v])
}
fn finite<H: AccountRestHost>(h: &H, v: &H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&call(h, "Number.isFinite", vec![v.clone()])?))
}
fn pf<H: AccountRestHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    call(h, "parseFloat", vec![v])
}
fn p0<H: AccountRestHost>(h: &H, row: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    pf(h, either(h, h.get(row, key)?, || Ok(h.literal("0")))?)
}
fn member<H: AccountRestHost>(h: &H, v: H::Value, key: H::Value) -> Result<H::Value, H::Error> {
    call(h, "member", vec![v, key])
}
fn omit<H: AccountRestHost>(
    h: &H,
    fields: Vec<(&'static str, H::Value)>,
) -> Result<H::Value, H::Error> {
    call(h, "omitUndefined", vec![h.object(fields)])
}
fn fail<H: AccountRestHost, T>(h: &H, message: H::Value) -> Result<T, H::Error> {
    Err(call_error(h, message)?)
}
fn call_error<H: AccountRestHost>(h: &H, message: H::Value) -> Result<H::Error, H::Error> {
    let units = h.template(&message)?;
    h.make_error(units)
}
fn domain<H: AccountRestHost>(h: &H, names: &[&'static str]) -> H::Value {
    h.array(
        names
            .iter()
            .map(|name| h.object(vec![("name", h.literal(name)), ("type", h.literal("felt"))]))
            .collect(),
    )
}

pub async fn sign_request<H: AccountRestHost>(
    h: &H,
    method: H::Value,
    path: H::Value,
    timestamp: H::Value,
    body: H::Value,
) -> Result<H::Value, H::Error> {
    let module = await_call!(h, "loadStarknet", vec![]);
    let full = cat(h, vec![method, timestamp, path, body])?;
    let message = if length(h, &full)? > 31.0 {
        let hash = call(h, "starknetKeccak", vec![module, full])?;
        cat(h, vec![h.literal("0x"), call(h, "radix16", vec![hash])?])?
    } else {
        full
    };
    let selected = h.select("signMessage")?;
    let payload = h.object(vec![
        (
            "domain",
            h.object(vec![
                ("name", h.literal("Paradex")),
                ("version", h.literal("1")),
                (
                    "chainId",
                    h.get(&state_get(h, "sdkAccount", "config")?, "paradexChainId")?,
                ),
            ]),
        ),
        (
            "types",
            h.object(vec![
                ("StarkNetDomain", domain(h, &["name", "version", "chainId"])),
                ("Request", domain(h, &["message"])),
            ]),
        ),
        ("primaryType", h.literal("Request")),
        ("message", h.object(vec![("message", message)])),
    ]);
    let signature = h.invoke(selected, vec![payload])?.await?;
    if h.is_array(&signature) {
        h.map(signature, &mut |s| {
            if h.kind(&s) == "bigint" {
                call(h, "bigint.toString", vec![s])
            } else {
                string(h, s)
            }
        })
    } else {
        Ok(h.array(vec![
            string(h, h.get(&signature, "r")?)?,
            string(h, h.get(&signature, "s")?)?,
        ]))
    }
}
pub fn to_chain_id_hex<H: AccountRestHost>(h: &H, id: H::Value) -> Result<H::Value, H::Error> {
    if matches!(h.kind(&id), "number" | "bigint") {
        return cat(h, vec![h.literal("0x"), call(h, "BigInt.hex", vec![id])?]);
    }
    let normalized = call(h, "trim", vec![string(h, id)?])?;
    if !h.truthy(&normalized) {
        return fail(h, h.literal("Invalid empty chain id for signing"));
    }
    if h.truthy(&call(h, "regex.hex", vec![normalized.clone()])?) {
        return Ok(normalized);
    }
    if h.truthy(&call(h, "regex.decimal", vec![normalized.clone()])?) {
        return cat(
            h,
            vec![h.literal("0x"), call(h, "BigInt.hex", vec![normalized])?],
        );
    }
    let bytes = call(h, "TextEncoder.encode", vec![normalized])?;
    let mut integer = call(h, "BigInt.zero", vec![])?;
    h.for_each(bytes, &mut |byte| {
        integer = call(h, "BigInt.shift8.or", vec![integer.clone(), byte])?;
        Ok(())
    })?;
    cat(h, vec![h.literal("0x"), call(h, "radix16", vec![integer])?])
}
pub async fn generate_new_token<H: AccountRestHost>(h: &H) -> Result<H::Value, H::Error> {
    let result = async {
        let timestamp = (number(h, call(h, "Date.now", vec![])?)? / 1000.0).floor();
        let expiry = timestamp + 86400.0;
        let endpoint = cat(
            h,
            vec![
                h.state("apiBaseUrl")?,
                h.literal("/auth/"),
                h.state("subkeyAddress")?,
            ],
        )?;
        let selected = h.select("signTypedData")?;
        let sdk = h.state("sdkAccount")?;
        let data = h.object(vec![
            (
                "domain",
                h.object(vec![
                    ("name", h.literal("Paradex")),
                    (
                        "chainId",
                        h.get(&state_get(h, "sdkAccount", "config")?, "paradexChainId")?,
                    ),
                    ("version", h.literal("1")),
                ]),
            ),
            (
                "types",
                h.object(vec![
                    ("StarkNetDomain", domain(h, &["name", "chainId", "version"])),
                    (
                        "Request",
                        domain(h, &["method", "path", "body", "timestamp", "expiration"]),
                    ),
                ]),
            ),
            ("primaryType", h.literal("Request")),
            (
                "message",
                h.object(vec![
                    ("method", h.literal("POST")),
                    ("path", h.literal("/v1/auth")),
                    ("body", h.literal("")),
                    ("timestamp", h.number(timestamp)),
                    ("expiration", h.number(expiry)),
                ]),
            ),
        ]);
        let signed = h
            .invoke(selected, vec![sdk, data, h.state("options")?])?
            .await?;
        let signature = h.get(&signed, "signature")?;
        let signer_headers = h.get(&signed, "headers")?;
        let headers = call(h, "spread", vec![signer_headers])?;
        for (name, value) in [
            ("Content-Type", h.literal("application/json")),
            ("Accept", h.literal("application/json")),
            ("PARADEX-STARKNET-ACCOUNT", h.state("mainAccountAddress")?),
            ("PARADEX-STARKNET-SIGNATURE", signature),
            (
                "PARADEX-TIMESTAMP",
                call(h, "number.toString", vec![h.number(timestamp)])?,
            ),
            (
                "PARADEX-SIGNATURE-EXPIRATION",
                call(h, "number.toString", vec![h.number(expiry)])?,
            ),
        ] {
            h.write(&headers, name, value)?;
        }
        let response = await_call!(
            h,
            "fetch",
            vec![
                endpoint,
                h.object(vec![("method", h.literal("POST")), ("headers", headers)])
            ]
        );
        if !h.truthy(&h.get(&response, "ok")?) {
            let error_text = await_call!(h, "textResponse", vec![response.clone()]);
            h.console(vec![h.literal("[Paradex JWT] ✗ Authentication failed")])?;
            h.console(vec![
                h.literal("[Paradex JWT] Status:"),
                h.get(&response, "status")?,
                h.get(&response, "statusText")?,
            ])?;
            h.console(vec![
                h.literal("[Paradex JWT] Response body:"),
                error_text.clone(),
            ])?;
            let parsed = (|| {
                let value = call(h, "JSON.parse", vec![error_text.clone()])?;
                h.console(vec![h.literal("[Paradex JWT] Error details:"), value])
            })();
            let message = cat(
                h,
                vec![
                    h.literal("Failed to generate JWT token: "),
                    h.get(&response, "status")?,
                    h.literal(" "),
                    error_text,
                ],
            )?;
            // Response text is already visible; parse/detail failure retains the HTTP failure.
            return match parsed {
                Ok(()) | Err(_) => fail(h, message),
            };
        }
        let data = await_call!(h, "jsonResponse", vec![response]);
        let token = either(h, h.get(&data, "jwt_token")?, || h.get(&data, "token"))?;
        if !h.truthy(&token) {
            h.console(vec![h.literal("[Paradex JWT] ✗ No token in response")])?;
            h.console(vec![h.literal("[Paradex JWT] Response data:"), data])?;
            return fail(h, h.literal("No JWT token in authentication response"));
        }
        Ok(token)
    }
    .await;
    if let Err(error) = &result {
        h.console(vec![
            h.literal("[Paradex JWT] ✗ Token generation failed with exception:"),
            h.error_value(error),
        ])?;
    }
    result
}
fn error_message<H: AccountRestHost>(h: &H, error: &H::Error) -> Result<H::Value, H::Error> {
    let v = h.error_value(error);
    either(
        h,
        h.optional(
            &h.optional(&h.optional(&v, "response")?, "data")?,
            "message",
        )?,
        || h.optional(&v, "message"),
    )
}
fn market<H: AccountRestHost>(h: &H, symbol: H::Value) -> Result<H::Value, H::Error> {
    member(h, state_get(h, "store", "marketsBySymbol")?, symbol)
}
fn request_params<H: AccountRestHost>(
    h: &H,
    fields: Vec<(&'static str, H::Value)>,
) -> Result<H::Value, H::Error> {
    Ok(h.object(vec![("params", omit(h, fields)?)]))
}
fn next_cursor<H: AccountRestHost>(h: &H, body: &H::Value) -> Result<H::Value, H::Error> {
    if h.kind(&h.optional(body, "next")?) == "string" && h.truthy(&h.get(body, "next")?) {
        h.get(body, "next")
    } else {
        Ok(h.undefined())
    }
}
fn sorted<H: AccountRestHost>(h: &H, values: H::Value, key: &str) -> Result<H::Value, H::Error> {
    h.sort(values, &mut |a, b| {
        Ok(number(h, h.get(&b, key)?)? - number(h, h.get(&a, key)?)?)
    })
}

pub async fn fetch_income_history<H: AccountRestHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.state("isPublicDataOnly")?) {
        return Ok(h.array(vec![]));
    }
    if h.truthy(&h.state("sdkInitPromise")?) {
        await_call!(h, "awaitSDK", vec![h.state("sdkInitPromise")?]);
    }
    if !h.truthy(&h.state("jwtManager")?) {
        log!(
            h,
            "warning",
            h.literal("Skipping fetchIncomeHistory: JWT manager not initialized")
        )?;
        return Ok(h.array(vec![]));
    }
    let end = nullish(h, h.optional(&opts, "endTime")?, || {
        call(h, "Date.now", vec![])
    })?;
    let start = nullish(h, h.optional(&opts, "startTime")?, || {
        Ok(h.number(number(h, end.clone())? - 2592000000.0))
    })?;
    let m = if h.truthy(&h.optional(&opts, "symbol")?) {
        market(h, h.get(&opts, "symbol")?)?
    } else {
        h.undefined()
    };
    if h.truthy(&h.optional(&opts, "symbol")?) && !h.truthy(&m) {
        return Ok(h.array(vec![]));
    }
    let result=async{
  let records=h.array(vec![]);let seen=call(h,"new Set",vec![])?;let mut cursor=h.undefined();let mut page=0;
  while page<200{
   let response=await_call!(h,"get",vec![h.literal("/fills"),request_params(h,vec![("cursor",cursor.clone()),("market",h.optional(&m,"symbol")?),("page_size",h.number(500.0)),("start_at",start.clone()),("end_at",end.clone())])?]);
   let body=h.optional(&response,"data")?;
   let rows=if h.is_array(&h.optional(&body,"results")?){h.get(&body,"results")?}else if h.is_array(&body){body.clone()}else{h.array(vec![])};
   if length(h,&rows)?==0.0{break;}
   h.for_each(rows,&mut |row|{
    let id=string(h,nullish(h,h.get(&row,"id")?,||Ok(h.literal("")))?)?;
    if h.truthy(&id)&&h.truthy(&call(h,"Set.has",vec![seen.clone(),id.clone()])?){return Ok(());}
    if h.truthy(&id){call(h,"Set.add",vec![seen.clone(),id])?;}
    let mapped=h.income(row,&mut |id|h.optional(&member(h,state_get(h,"store","marketsById")?,id)?,"symbol"))?;
    call(h,"push.spread",vec![records.clone(),mapped])?;Ok(())
   })?;
   let next=next_cursor(h,&body)?;if !h.truthy(&next)||h.equal(&next,&cursor){break;}cursor=next;page+=1;
  }
  if page>=200{log!(h,"warning",h.literal("fetchIncomeHistory stopped at the 200-page cap; older fills in this window were not fetched"))?;}
  sorted(h,records,"timestamp")
 }.await;
    match result {
        Ok(records) => Ok(records),
        Err(e) => {
            h.emit(error_message(h, &e)?)?;
            Ok(h.array(vec![]))
        }
    }
}
pub async fn fetch_historical_orders<H: AccountRestHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.state("isPublicDataOnly")?) {
        return Ok(h.array(vec![]));
    }
    let result=async{
  await_call!(h,"ensureAuth",vec![h.literal("fetchHistoricalOrders")]);
  let orders=h.array(vec![]);let seen=call(h,"new Set",vec![])?;let mut cursor=h.undefined();let mut page=0;
  while page<50{
   let response=await_call!(h,"get",vec![h.literal("/orders-history"),request_params(h,vec![("market",h.optional(&opts,"symbol")?),("start_at",h.optional(&opts,"from")?),("end_at",h.optional(&opts,"to")?),("page_size",h.number(100.0)),("cursor",cursor.clone())])?]);
   let data=h.get(&response,"data")?;
   let rows=if h.is_array(&h.optional(&data,"results")?){h.get(&data,"results")?}else{h.array(vec![])};
   if length(h,&rows)?==0.0{break;}
   h.for_each(rows,&mut |row|{let order=h.historical(row)?;if !h.truthy(&order)||h.truthy(&call(h,"Set.has",vec![seen.clone(),h.get(&order,"id")?])?){return Ok(());}call(h,"Set.add",vec![seen.clone(),h.get(&order,"id")?])?;call(h,"push",vec![orders.clone(),order])?;Ok(())})?;
   cursor=next_cursor(h,&data)?;if !h.truthy(&cursor){break;}page+=1;
  }
  if page>=50{log!(h,"warning",h.literal("fetchHistoricalOrders stopped at the 50-page cap; older orders were not fetched"))?;}
  let ordered=sorted(h,orders,"createdTime")?;
  if h.truthy(&h.optional(&opts,"limit")?)&&number(h,h.get(&opts,"limit")?)?>0.0{call(h,"slice",vec![ordered,h.number(0.0),h.get(&opts,"limit")?])}else{Ok(ordered)}
 }.await;
    match result {
        Ok(records) => Ok(records),
        Err(e) => {
            if h.history_error(&e)? {
                log!(h, "warning", h.get(&h.error_value(&e), "message")?)?;
                return Err(e);
            }
            h.emit(error_message(h, &e)?)?;
            Ok(h.array(vec![]))
        }
    }
}
fn milliseconds<H: AccountRestHost>(h: &H, value: H::Value) -> Result<H::Value, H::Error> {
    if h.nullish(&value) || !finite(h, &value)? {
        return Ok(h.undefined());
    }
    let n = number(h, value)?;
    Ok(h.number(if n > 1e12 {
        n.floor()
    } else {
        (n * 1000.0).floor()
    }))
}
fn seconds<H: AccountRestHost>(h: &H, value: H::Value) -> Result<H::Value, H::Error> {
    let n = number(
        h,
        call(
            h,
            "parseInt10",
            vec![string(h, nullish(h, value, || Ok(h.literal("0")))?)?],
        )?,
    )?;
    if !n.is_finite() || n <= 0.0 {
        Ok(h.undefined())
    } else {
        Ok(h.number(if n > 1e12 { (n / 1000.0).floor() } else { n }))
    }
}
fn extract_rows<H: AccountRestHost>(h: &H, payload: H::Value) -> Result<H::Value, H::Error> {
    let body = nullish(h, h.optional(&payload, "data")?, || Ok(payload.clone()))?;
    let candidates = [
        h.optional(&body, "results")?,
        h.optional(&body, "fills")?,
        h.optional(&body, "data")?,
        body,
    ];
    Ok(candidates
        .into_iter()
        .find(|v| h.is_array(v))
        .unwrap_or_else(|| h.array(vec![])))
}
fn filled<H: AccountRestHost>(h: &H, row: &H::Value) -> Result<f64, H::Error> {
    Ok(number(h, p0(h, row, "size")?)?
        - number(
            h,
            pf(
                h,
                either(h, h.get(row, "remaining_size")?, || {
                    either(h, h.get(row, "remainingSize")?, || Ok(h.literal("0")))
                })?,
            )?,
        )?)
}
fn fill_rows<H: AccountRestHost>(
    h: &H,
    rows: H::Value,
    m: &H::Value,
    order_id: &H::Value,
) -> Result<H::Value, H::Error> {
    h.reduce(rows, h.array(vec![]), &mut |acc, row| {
        let symbol = string(
            h,
            either(h, h.get(&row, "market")?, || {
                either(h, h.get(&row, "symbol")?, || Ok(h.literal("")))
            })?,
        )?;
        if !h.equal(&symbol, &h.get(m, "symbol")?) {
            return Ok(acc);
        }
        let side = call(
            h,
            "ORDER_SIDE",
            vec![call(
                h,
                "String.upper",
                vec![either(h, h.get(&row, "side")?, || Ok(h.literal("")))?],
            )?],
        )?;
        if !h.truthy(&side) {
            return Ok(acc);
        }
        let price = pf(
            h,
            either(h, h.get(&row, "price")?, || {
                either(h, h.get(&row, "avg_fill_price")?, || {
                    either(h, h.get(&row, "average_fill_price")?, || Ok(h.literal("0")))
                })
            })?,
        )?;
        let filled = filled(h, &row)?;
        let qty = pf(
            h,
            either(h, h.get(&row, "fill_size")?, || {
                either(h, h.get(&row, "filled_size")?, || {
                    either(h, h.get(&row, "quantity")?, || {
                        Ok(h.number(if filled > 0.0 { filled } else { 0.0 }))
                    })
                })
            })?,
        )?;
        if !finite(h, &price)?
            || !finite(h, &qty)?
            || number(h, price.clone())? <= 0.0
            || number(h, qty.clone())? <= 0.0
        {
            return Ok(acc);
        }
        let ts = seconds(
            h,
            either(h, h.get(&row, "created_at")?, || {
                either(h, h.get(&row, "timestamp")?, || h.get(&row, "updated_at"))
            })?,
        )?;
        if !h.truthy(&ts) {
            return Ok(acc);
        }
        let id = string(
            h,
            either(h, h.get(&row, "order_id")?, || {
                either(h, h.get(&row, "orderId")?, || {
                    either(h, h.get(&row, "client_id")?, || Ok(h.literal("")))
                })
            })?,
        )?;
        if h.truthy(order_id) && h.truthy(&id) && !h.equal(&id, &string(h, order_id.clone())?) {
            return Ok(acc);
        }
        let fee = p0(h, &row, "fee")?;
        let pnl = pf(
            h,
            nullish(h, h.get(&row, "realized_pnl")?, || Ok(h.literal("")))?,
        )?;
        let original_id = if h.truthy(&h.get(&row, "id")?) {
            string(h, h.get(&row, "id")?)?
        } else {
            h.undefined()
        };
        let order = either(h, id, || Ok(h.undefined()))?;
        let commission = if finite(h, &fee)? {
            call(h, "Math.abs", vec![fee])?
        } else {
            h.undefined()
        };
        let asset = either(h, h.get(&row, "fee_currency")?, || h.get(m, "quote"))?;
        let pnl = if finite(h, &pnl)? { pnl } else { h.undefined() };
        let maker = if h.truthy(&h.get(&row, "liquidity")?) {
            h.boolean(same(
                h,
                call(h, "String.upper", vec![h.get(&row, "liquidity")?])?,
                "MAKER",
            ))
        } else {
            h.undefined()
        };
        let execution = omit(
            h,
            vec![
                ("id", original_id),
                ("timestamp", ts),
                ("symbol", symbol),
                ("orderId", order),
                ("price", price),
                ("qty", qty),
                ("side", side),
                ("commission", commission),
                ("commissionAsset", asset),
                ("realizedPnl", pnl),
                ("maker", maker),
            ],
        )?;
        call(h, "push", vec![acc.clone(), execution])?;
        Ok(acc)
    })
}
pub async fn fetch_historical_fills<H: AccountRestHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.state("isPublicDataOnly")?) {
        return Ok(h.array(vec![]));
    }
    let opts = call(h, "destructureExecution", vec![opts])?;
    let symbol = h.get(&opts, "symbol")?;
    let order_id = h.get(&opts, "orderId")?;
    let from = h.get(&opts, "from")?;
    let to = h.get(&opts, "to")?;
    let limit = h.get(&opts, "limit")?;
    let limit = if h.is_undefined(&limit) {
        h.number(100.0)
    } else {
        limit
    };
    if !h.truthy(&symbol) {
        return Ok(h.array(vec![]));
    }
    let m = market(h, symbol)?;
    if !h.truthy(&m) {
        return Ok(h.array(vec![]));
    }
    let max_limit = call(
        h,
        "Math.min",
        vec![
            call(h, "Math.max", vec![limit, h.number(1.0)])?,
            h.number(500.0),
        ],
    )?;
    let from_ms = milliseconds(h, from)?;
    let to_ms = milliseconds(h, to)?;
    let result = async {
        await_call!(h, "ensureAuth", vec![h.literal("fetchHistoricalFills")]);
        let rows = h.array(vec![]);
        let mut cursor = h.undefined();
        let mut page = 0;
        while page < 5 && length(h, &rows)? < number(h, max_limit.clone())? {
            let response = await_call!(
                h,
                "get",
                vec![
                    h.literal("/fills"),
                    request_params(
                        h,
                        vec![
                            ("cursor", cursor.clone()),
                            ("market", h.get(&m, "symbol")?),
                            ("page_size", max_limit.clone()),
                            ("end_at", to_ms.clone())
                        ]
                    )?
                ]
            );
            let body = h.optional(&response, "data")?;
            call(
                h,
                "push.spread",
                vec![rows.clone(), extract_rows(h, body.clone())?],
            )?;
            cursor = next_cursor(h, &body)?;
            if !h.truthy(&cursor) {
                break;
            }
            page += 1;
        }
        let fallback = if length(h, &rows)? > 0.0 {
            h.array(vec![])
        } else {
            let response = await_call!(
                h,
                "get",
                vec![
                    h.literal("/orders-history"),
                    request_params(
                        h,
                        vec![
                            ("market", h.get(&m, "symbol")?),
                            ("limit", max_limit.clone())
                        ]
                    )?
                ]
            );
            let extracted = extract_rows(h, h.optional(&response, "data")?)?;
            h.filter(extracted, &mut |row| Ok(filled(h, &row)? > 0.0))?
        };
        let primary = fill_rows(h, rows, &m, &order_id)?;
        let fallback = fill_rows(h, fallback, &m, &order_id)?;
        let executions = call(h, "spread.concat", vec![primary, fallback])?;
        let map = call(h, "new Map", vec![])?;
        h.for_each(executions, &mut |execution| {
            let ts = number(h, h.get(&execution, "timestamp")?)? * 1000.0;
            if h.truthy(&from_ms) && ts < number(h, from_ms.clone())? {
                return Ok(());
            }
            if h.truthy(&to_ms) && ts > number(h, to_ms.clone())? {
                return Ok(());
            }
            let key = cat(
                h,
                vec![
                    either(h, h.get(&execution, "orderId")?, || {
                        Ok(h.literal("no-order"))
                    })?,
                    h.literal(":"),
                    h.get(&execution, "timestamp")?,
                    h.literal(":"),
                    h.get(&execution, "price")?,
                    h.literal(":"),
                    h.get(&execution, "qty")?,
                    h.literal(":"),
                    h.get(&execution, "side")?,
                ],
            )?;
            call(h, "Map.set", vec![map.clone(), key, execution])?;
            Ok(())
        })?;
        let values = call(h, "Array.from.values", vec![map])?;
        call(
            h,
            "slice",
            vec![sorted(h, values, "timestamp")?, h.number(0.0), max_limit],
        )
    }
    .await;
    match result {
        Ok(records) => Ok(records),
        Err(e) => {
            if h.history_error(&e)? {
                log!(h, "warning", h.get(&h.error_value(&e), "message")?)?;
                return Err(e);
            }
            log!(
                h,
                "error",
                h.literal("Error fetching executions: "),
                either(h, h.optional(&h.error_value(&e), "message")?, || Ok(
                    h.error_value(&e)
                ))?
            )?;
            Ok(h.array(vec![]))
        }
    }
}
fn setting_error<H: AccountRestHost>(
    h: &H,
    error: &H::Error,
    symbol: Option<H::Value>,
) -> Result<(), H::Error> {
    let v = h.error_value(error);
    let message = either(h, h.optional(&v, "message")?, || {
        Ok(h.literal("Unknown error"))
    })?;
    if let Some(symbol) = &symbol {
        log!(
            h,
            "error",
            h.literal("Error setting leverage for "),
            symbol.clone(),
            h.literal(": "),
            message.clone()
        )?;
    } else {
        log!(
            h,
            "error",
            h.literal("Error setting max slippage: "),
            message.clone()
        )?;
    }
    log!(
        h,
        "debug",
        h.literal("Full error object: "),
        call(h, "JSON.stringify.ownNames", vec![v.clone()])?
    )?;
    if h.truthy(&h.optional(&v, "response")?) {
        log!(
            h,
            "debug",
            h.literal("Error response data: "),
            call(
                h,
                "JSON.stringify",
                vec![h.get(&h.get(&v, "response")?, "data")?]
            )?
        )?;
        log!(
            h,
            "debug",
            h.literal("Error response status: "),
            h.get(&h.get(&v, "response")?, "status")?
        )?;
    }
    let value = if let Some(symbol) = symbol {
        cat(
            h,
            vec![
                h.literal("Failed to set leverage for "),
                symbol,
                h.literal(": "),
                message,
            ],
        )?
    } else {
        cat(h, vec![h.literal("Failed to set max slippage: "), message])?
    };
    h.emit(value)
}
pub async fn set_leverage<H: AccountRestHost>(
    h: &H,
    symbol: H::Value,
    input: H::Value,
) -> Result<H::Value, H::Error> {
    let result = async {
        let m = market(h, symbol.clone())?;
        if !h.truthy(&m) {
            let message = cat(
                h,
                vec![
                    h.literal("Market "),
                    symbol.clone(),
                    h.literal(" not found"),
                ],
            )?;
            log!(h, "error", message.clone())?;
            return fail(h, message);
        }
        let min = either(
            h,
            h.optional(&h.get(&h.get(&m, "limits")?, "leverage")?, "min")?,
            || Ok(h.number(1.0)),
        )?;
        let max = either(
            h,
            h.optional(&h.get(&h.get(&m, "limits")?, "leverage")?, "max")?,
            || Ok(h.number(100.0)),
        )?;
        let lev = call(
            h,
            "Math.min",
            vec![
                call(
                    h,
                    "Math.max",
                    vec![call(h, "Math.floor", vec![input.clone()])?, min.clone()],
                )?,
                max.clone(),
            ],
        )?;
        if !h.equal(&lev, &input) {
            log!(
                h,
                "warning",
                h.literal("Leverage "),
                input,
                h.literal(" adjusted to "),
                lev.clone(),
                h.literal(" (min: "),
                min,
                h.literal(", max: "),
                max,
                h.literal(")")
            )?;
        }
        log!(
            h,
            "info",
            h.literal("Setting leverage for "),
            symbol.clone(),
            h.literal(" to "),
            lev.clone(),
            h.literal("x")
        )?;
        let endpoint = call(
            h,
            "replace",
            vec![
                h.literal("/account/margin/{market}"),
                h.literal("{market}"),
                symbol.clone(),
            ],
        )?;
        let payload = h.object(vec![
            ("leverage", lev.clone()),
            ("margin_type", h.literal("CROSS")),
        ]);
        log!(
            h,
            "debug",
            h.literal("POST "),
            endpoint.clone(),
            h.literal(" with payload: "),
            call(h, "JSON.stringify", vec![payload.clone()])?
        )?;
        let response = await_call!(h, "post", vec![endpoint, payload]);
        log!(
            h,
            "debug",
            h.literal("Response status: "),
            h.get(&response, "status")?,
            h.literal(", data: "),
            call(h, "JSON.stringify", vec![h.get(&response, "data")?])?
        )?;
        call(h, "leverage.set", vec![symbol.clone(), lev.clone()])?;
        let existing = h.filter(state_get(h, "store", "positions")?, &mut |p| {
            Ok(h.equal(&h.get(&p, "symbol")?, &symbol))
        })?;
        if length(h, &existing)? > 0.0 {
            let updated = h.map(existing, &mut |p| {
                let next = call(h, "spread", vec![p])?;
                h.write(&next, "leverage", lev.clone())?;
                Ok(next)
            })?;
            call(h, "addOrUpdatePositions", vec![updated])?;
            log!(
                h,
                "debug",
                h.literal("Updated leverage in existing positions for "),
                symbol.clone()
            )?;
        } else {
            let p = h.object(vec![
                ("id", symbol.clone()),
                ("symbol", symbol.clone()),
                ("side", h.literal("long")),
                ("entryPrice", h.number(0.0)),
                ("notional", h.number(0.0)),
                ("leverage", lev.clone()),
                ("unrealizedPnl", h.number(0.0)),
                ("contracts", h.number(0.0)),
                ("liquidationPrice", h.number(0.0)),
                ("marginType", h.literal("cross")),
                ("positionMode", h.literal("oneway")),
            ]);
            call(h, "addOrUpdatePositions", vec![h.array(vec![p])])?;
            log!(
                h,
                "debug",
                h.literal("Created fake position with leverage "),
                lev.clone(),
                h.literal("x for "),
                symbol.clone()
            )?;
        }
        log!(
            h,
            "info",
            h.literal("Successfully set leverage for "),
            symbol.clone(),
            h.literal(" to "),
            lev,
            h.literal("x")
        )?;
        Ok(h.undefined())
    }
    .await;
    if let Err(e) = &result {
        setting_error(h, e, Some(symbol))?;
    }
    result
}
pub async fn set_max_slippage<H: AccountRestHost>(
    h: &H,
    slippage: H::Value,
    market: H::Value,
) -> Result<H::Value, H::Error> {
    let result = async {
        if !h.truthy(&market) {
            return fail(
                h,
                h.literal("Paradex max slippage requires a market symbol"),
            );
        }
        log!(
            h,
            "info",
            h.literal("Setting account max slippage for "),
            market.clone(),
            h.literal(" to "),
            slippage.clone()
        )?;
        let endpoint = call(
            h,
            "replace",
            vec![
                h.literal("/account/profile/market_max_slippage/{market}"),
                h.literal("{market}"),
                market.clone(),
            ],
        )?;
        let payload = h.object(vec![("max_slippage", slippage.clone())]);
        log!(
            h,
            "debug",
            h.literal("POST "),
            endpoint.clone(),
            h.literal(" with payload: "),
            call(h, "JSON.stringify", vec![payload.clone()])?
        )?;
        let response = await_call!(h, "post", vec![endpoint, payload]);
        log!(
            h,
            "debug",
            h.literal("Response status: "),
            h.get(&response, "status")?,
            h.literal(", data: "),
            call(h, "JSON.stringify", vec![h.get(&response, "data")?])?
        )?;
        log!(
            h,
            "info",
            h.literal("Successfully set account max slippage for "),
            market,
            h.literal(" to "),
            slippage
        )?;
        Ok(h.undefined())
    }
    .await;
    if let Err(e) = &result {
        setting_error(h, e, None)?;
    }
    result
}
