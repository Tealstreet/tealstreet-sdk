use super::*;
use crate::wire::not_sent;
use safe_cex_engine::num::{adjust, format};
use safe_cex_engine::{ErrorKind, Outcome, VenueError};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Placement<'a> {
    client_oid: String,
    symbol: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    side: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_only: Option<bool>,
}
struct Write {
    receipt: Option<OrderIdentityReceipt>,
    method: Method,
    path: String,
    body: Option<String>,
    kind: WriteKind,
}
enum WriteKind {
    Place,
    Amend(String),
    Cancel,
}
pub struct Job {
    pub(super) command: CommandId,
    requests: VecDeque<Write>,
    ids: Vec<String>,
    current: WriteKind,
    returns_ids: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
// Keep entropy failure fallible so batch preflight cannot dispatch a partial write.
fn client_oid(
    supplied: Option<&str>,
    mint: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    match supplied {
        Some(id)
            if !id.is_empty()
                && id.len() <= 40
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') =>
        {
            Ok(id.to_owned())
        }
        Some(_) => Err("KuCoin Spot clientOrderId must be 1-40 letters, digits, '_' or '-'".into()),
        None => mint(),
    }
}
fn placements(
    orders: Vec<PlaceOrderOpts>,
    mut prepare: impl FnMut(&PlaceOrderOpts) -> Result<Write, String>,
) -> Result<VecDeque<Write>, String> {
    let mut seen = HashSet::new();
    let mut requests = VecDeque::new();
    for order in orders {
        if let Some(id) = &order.client_order_id
            && !seen.insert(id.clone())
        {
            return Err("duplicate KuCoin Spot caller clientOrderId".into());
        }
        requests.push_back(prepare(&order)?);
    }
    Ok(requests)
}
impl KucoinSpot {
    fn placement(&mut self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<Write, String> {
        if !matches!(o.kind, OrderType::Limit | OrderType::Market)
            || o.stop_loss.is_some()
            || o.take_profit.is_some()
            || o.trigger_price.is_some()
        {
            return Err("KuCoin Spot supports only plain limit and market orders".into());
        }
        if o.stp_mode.is_some() {
            return Err("KuCoin Spot does not support per-order STP overrides".into());
        }
        if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
            return Err("KuCoin Spot does not support Bybit order options".into());
        }
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or("KuCoin Spot market not found")?;
        if !o.amount.is_finite()
            || o.amount <= 0.0
            || o.price.is_some_and(|p| !p.is_finite() || p <= 0.0)
        {
            return Err("invalid KuCoin Spot order amount or price".into());
        }
        let amount = adjust(o.amount, m.precision.amount);
        if amount <= 0.0 || amount > m.limits.amount.max {
            return Err("KuCoin Spot order exceeds native size limits".into());
        }
        let client_oid = client_oid(o.client_order_id.as_deref(), || {
            safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
        })?;
        let price = if o.kind == OrderType::Limit {
            Some(format(adjust(
                o.price.ok_or("KuCoin Spot limit order needs a price")?,
                m.precision.price,
            )))
        } else {
            None
        };
        let payload = Placement {
            client_oid,
            symbol: &m.id,
            kind: if o.kind == OrderType::Market {
                "market"
            } else {
                "limit"
            },
            side: if o.side == OrderSide::Sell {
                "sell"
            } else {
                "buy"
            },
            price,
            size: format(amount),
            time_in_force: (o.kind == OrderType::Limit).then_some(match o.time_in_force {
                Some(TimeInForce::ImmediateOrCancel) => "IOC",
                Some(TimeInForce::FillOrKill) => "FOK",
                _ => "GTC",
            }),
            post_only: (o.time_in_force == Some(TimeInForce::PostOnly)).then_some(true),
        };
        Ok(Write {
            receipt: Some(OrderIdentityReceipt {
                symbol: o.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: payload.client_oid.clone(),
            }),
            method: Method::Post,
            path: "/api/v1/hf/orders".into(),
            body: Some(serde_json::to_string(&payload).unwrap()),
            kind: WriteKind::Place,
        })
    }
    fn next_write(&self, cx: &mut Cx<'_, Pending>, mut job: Job) {
        if let Some(w) = job.requests.pop_front() {
            if let Some(receipt) = w.receipt {
                cx.command_progress(job.command, vec![receipt]);
            }
            job.current = w.kind;
            self.request(cx, w.method, &w.path, w.body, true, Pending::Write(job));
        } else {
            cx.done(
                job.command,
                if job.returns_ids {
                    serde_json::to_string(&job.ids).unwrap()
                } else {
                    "[]".into()
                },
            );
        }
    }
    pub(super) fn write_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut job: Job,
        data: &RawValue,
    ) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Ack<'a> {
            #[serde(default)]
            order_id: Option<&'a str>,
            #[serde(default)]
            new_order_id: Option<&'a str>,
        }
        match &job.current {
            WriteKind::Place => match serde_json::from_str::<Ack>(data.get())
                .ok()
                .and_then(|a| a.order_id.filter(|s| !s.is_empty()))
            {
                Some(id) => job.ids.push(id.into()),
                None => {
                    let error =
                        VenueError::new("KuCoin Spot placement acknowledgement missing orderId");
                    self.write_failed(cx, job, error.outcome(Outcome::Unknown));
                    return;
                }
            },
            WriteKind::Amend(old) => match serde_json::from_str::<Ack>(data.get()) {
                Ok(ack) => job.ids.push(
                    ack.new_order_id
                        .filter(|id| !id.is_empty())
                        .unwrap_or(old)
                        .into(),
                ),
                Err(e) => {
                    let error = VenueError::new(e.to_string()).outcome(Outcome::Unknown);
                    self.write_failed(cx, job, error);
                    return;
                }
            },
            WriteKind::Cancel => {}
        }
        job.effective = true;
        self.revision += 1;
        self.heal(cx, false);
        self.next_write(cx, job);
    }
    /// Fails the write job; after a write took effect it is never Rejected or NotSent.
    pub(super) fn write_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        job: Job,
        mut error: VenueError,
    ) {
        if (job.effective || !job.ids.is_empty())
            && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
        {
            error.outcome = None;
        }
        cx.fail(job.command, error);
    }
    pub(super) fn trade(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let command = match command {
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                let Some(order) = cx.store.order(&id) else {
                    cx.fail(command, not_sent("KuCoin Spot amend order not found"));
                    return;
                };
                Command::AmendOrderWithSymbol {
                    command,
                    id,
                    symbol: order.symbol.clone(),
                    price,
                    amount,
                }
            }
            other => other,
        };
        let (id, build): (CommandId, Result<(VecDeque<Write>, bool), String>) = match command {
            Command::PlaceOrders { command, orders } => (
                command,
                placements(orders, |o| self.placement(cx, o)).map(|requests| (requests, true)),
            ),
            Command::CancelOrders { command, ids } => {
                let out = ids
                    .into_iter()
                    .map(|id| {
                        let o = cx.store.order(&id).ok_or_else(|| {
                            format!("KuCoin Spot cancel needs order symbol for {id}")
                        })?;
                        Ok(Write {
                            receipt: None,
                            method: Method::Delete,
                            path: format!(
                                "/api/v1/hf/orders/{}?symbol={}",
                                crate::encode(&id),
                                crate::api_symbol(cx.store, &o.symbol)
                            ),
                            body: None,
                            kind: WriteKind::Cancel,
                        })
                    })
                    .collect::<Result<VecDeque<_>, String>>();
                (command, out.map(|r| (r, false)))
            }
            Command::CancelOrdersWithSymbols { command, orders } => {
                let out = orders
                    .into_iter()
                    .map(|o| Write {
                        receipt: None,
                        method: Method::Delete,
                        path: format!(
                            "/api/v1/hf/orders/{}?symbol={}",
                            crate::encode(&o.id),
                            crate::api_symbol(cx.store, &o.symbol)
                        ),
                        body: None,
                        kind: WriteKind::Cancel,
                    })
                    .collect();
                (command, Ok((out, false)))
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let path = if symbol.is_empty() {
                    "/api/v1/hf/orders/cancelAll".into()
                } else {
                    format!(
                        "/api/v1/hf/orders?symbol={}",
                        crate::api_symbol(cx.store, &symbol)
                    )
                };
                (
                    command,
                    Ok((
                        VecDeque::from([Write {
                            receipt: None,
                            method: Method::Delete,
                            path,
                            body: None,
                            kind: WriteKind::Cancel,
                        }]),
                        false,
                    )),
                )
            }
            Command::AmendOrderWithSymbol {
                command,
                id,
                symbol,
                price,
                amount,
            } => {
                let result = (|| {
                    let market = cx
                        .store
                        .market(&symbol)
                        .ok_or("KuCoin Spot amend market not found")?;
                    if price.is_none() && amount.is_none() {
                        return Err("KuCoin Spot amend has no changes".into());
                    }
                    if price.is_some_and(|n| !n.is_finite() || n <= 0.0)
                        || amount.is_some_and(|n| !n.is_finite() || n <= 0.0)
                    {
                        return Err("invalid KuCoin Spot amend amount or price".into());
                    }
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Alter<'a> {
                        symbol: &'a str,
                        order_id: &'a str,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        new_price: Option<String>,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        new_size: Option<String>,
                    }
                    let body = serde_json::to_string(&Alter {
                        symbol: &market.id,
                        order_id: &id,
                        new_price: price.map(|n| format(adjust(n, market.precision.price))),
                        new_size: amount.map(|n| format(adjust(n, market.precision.amount))),
                    })
                    .unwrap();
                    Ok((
                        VecDeque::from([Write {
                            receipt: None,
                            method: Method::Post,
                            path: "/api/v1/hf/orders/alter".into(),
                            body: Some(body),
                            kind: WriteKind::Amend(id),
                        }]),
                        true,
                    ))
                })();
                (command, result)
            }
            Command::SetLeverage {
                command, symbol, ..
            } => {
                cx.store.update_setting(&symbol, |s| {
                    s.leverage = 1.0;
                    s.buy_leverage = Some(1.0);
                    s.sell_leverage = Some(1.0);
                });
                cx.done(command, "[]".into());
                return;
            }
            Command::FetchLeverage { symbol, .. } => {
                let Ok(context) = cx.admit_http_command(crate::local_read_command_policy()) else {
                    return;
                };
                if cx.validate_read_root_context(&context).is_err() {
                    return;
                }
                cx.store.update_setting(&symbol, |s| {
                    s.leverage = 1.0;
                    s.buy_leverage = Some(1.0);
                    s.sell_leverage = Some(1.0);
                });
                let _ = cx.done_context(&context, "1".into());
                return;
            }
            Command::SetMarginType { command, .. } | Command::SetPositionMode { command, .. } => {
                cx.done(command, "[]".into());
                return;
            }
            Command::FetchAccount { command } => {
                if self.credentials.is_none() {
                    let Ok(context) = cx.admit_http_command(crate::local_read_command_policy())
                    else {
                        return;
                    };
                    let _ = cx.fail_context(
                        &context,
                        not_sent("KuCoin Spot requires credentials").kind(ErrorKind::InvalidInput),
                    );
                } else {
                    self.request(
                        cx,
                        Method::Get,
                        "/api/v2/user-info",
                        None,
                        true,
                        Pending::Account(command),
                    );
                }
                return;
            }
            Command::FetchStpMode { command }
            | Command::FetchOrderBook { command, .. }
            | Command::SetStpMode { command, .. } => {
                cx.fail(
                    command,
                    not_sent("unsupported KuCoin Spot command").kind(ErrorKind::Unsupported),
                );
                return;
            }
            _ => return,
        };
        if self.credentials.is_none() {
            cx.fail(
                id,
                not_sent("KuCoin Spot requires credentials").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        match build {
            Ok((requests, returns_ids)) => self.next_write(
                cx,
                Job {
                    command: id,
                    requests,
                    ids: vec![],
                    current: WriteKind::Cancel,
                    returns_ids,
                    effective: false,
                },
            ),
            Err(e) => cx.fail(id, not_sent(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_id_bypasses_entropy_and_entropy_failure_discards_staged_batch() {
        let caller = "caller-exact_1";
        assert_eq!(
            client_oid(Some(caller), || panic!("caller ID must not use entropy")).unwrap(),
            caller
        );
        let order: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTC-USDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        let mut calls = 0;
        let result = placements(vec![order.clone(), order], |order| {
            calls += 1;
            let id = client_oid(order.client_order_id.as_deref(), || {
                if calls == 1 {
                    Ok("generated-first".into())
                } else {
                    Err("entropy unavailable".into())
                }
            })?;
            Ok(Write {
                receipt: None,
                method: Method::Post,
                path: "/api/v1/hf/orders".into(),
                body: Some(id),
                kind: WriteKind::Place,
            })
        });
        assert_eq!(calls, 2);
        assert_eq!(result.err().as_deref(), Some("entropy unavailable"));
    }
}
