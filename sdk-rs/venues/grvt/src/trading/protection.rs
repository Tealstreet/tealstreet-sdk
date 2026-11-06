use super::*;

#[derive(Clone, Copy, Default)]
pub(crate) struct References([Option<(f64, f64)>; 3]);
impl References {
    pub(crate) fn observe(&mut self, now: f64, fields: [Option<f64>; 3]) {
        for (slot, field) in self.0.iter_mut().zip(fields) {
            if let Some(value) = field {
                *slot =
                    (now.is_finite() && value.is_finite() && value > 0.).then_some((value, now));
            }
        }
    }
    fn fresh(&self, kind: StopTriggerType, now: f64) -> Option<f64> {
        let index = match kind {
            StopTriggerType::LastPrice => 0,
            StopTriggerType::MarkPrice => 1,
            StopTriggerType::IndexPrice => 2,
        };
        self.0[index]
            .filter(|(_, at)| now.is_finite() && now >= *at && now - at <= 5_000.)
            .map(|(value, _)| value)
    }
}

pub(super) struct Edit {
    pub standalone: bool,
    pub observed: Order,
    pub replacement: Order,
    pub original: Order,
    pub execution: (bool, f64),
    pub native: String,
    pub inspected_updated_ns: u64,
    pub canceled: bool,
    pub restoring: bool,
    pub refusal: Option<VenueError>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeOrder<'a> {
    #[serde(borrow, alias = "sub_account_id")]
    sa: Str<'a>,
    #[serde(borrow, default, rename = "oi", alias = "order_id", skip_serializing)]
    _order_id: Option<&'a RawValue>,
    #[serde(alias = "is_market")]
    im: bool,
    #[serde(borrow, alias = "time_in_force")]
    ti: Str<'a>,
    #[serde(alias = "post_only")]
    po: bool,
    #[serde(alias = "reduce_only")]
    ro: bool,
    #[serde(borrow, alias = "legs")]
    l: Vec<NativeLeg<'a>>,
    #[serde(
        borrow,
        default,
        alias = "signature",
        skip_serializing_if = "Option::is_none"
    )]
    s: Option<&'a RawValue>,
    #[serde(borrow, alias = "metadata")]
    m: NativeMetadata<'a>,
    #[serde(borrow, default, rename = "s1", alias = "state", skip_serializing)]
    _state: Option<&'a RawValue>,
    #[serde(borrow, alias = "builder")]
    b: Str<'a>,
    #[serde(borrow, alias = "builder_fee")]
    bf: Str<'a>,
}
#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct NativeLeg<'a> {
    #[serde(borrow, alias = "instrument")]
    i: Str<'a>,
    #[serde(borrow, alias = "size")]
    s: Str<'a>,
    #[serde(
        borrow,
        default,
        alias = "limit_price",
        skip_serializing_if = "Option::is_none"
    )]
    lp: Option<Str<'a>>,
    #[serde(alias = "is_buying_asset")]
    ib: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeMetadata<'a> {
    #[serde(borrow, alias = "client_order_id")]
    co: Str<'a>,
    #[serde(
        borrow,
        default,
        alias = "create_time",
        skip_serializing_if = "Option::is_none"
    )]
    ct: Option<Str<'a>>,
    #[serde(borrow, alias = "trigger")]
    t: NativeTrigger<'a>,
}
#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct NativeTrigger<'a> {
    #[serde(borrow, alias = "trigger_type")]
    tt: Str<'a>,
    #[serde(borrow, alias = "tpsl")]
    t: NativeTriggerDetails<'a>,
}
#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct NativeTriggerDetails<'a> {
    #[serde(borrow, alias = "trigger_by")]
    tb: Str<'a>,
    #[serde(borrow, alias = "trigger_price")]
    tp: Str<'a>,
    #[serde(alias = "close_position")]
    cp: bool,
    #[serde(rename = "is", alias = "is_split_position")]
    split: bool,
}
#[derive(Deserialize, Serialize)]
struct RestoreBody<'a> {
    #[serde(borrow)]
    o: NativeOrder<'a>,
}
impl Grvt {
    pub(super) fn protective_reference(
        &self,
        cx: &Cx<'_, Pending>,
        order: &Order,
    ) -> Result<(), String> {
        let reference = order
            .trigger_type
            .and_then(|kind| {
                cx.store
                    .market_index(&order.symbol)
                    .and_then(|index| self.ticker_state.get(index as usize))
                    .and_then(|state| state.references.fresh(kind, cx.now))
            })
            .ok_or("GRVT stop not changed: reference stale")?;
        let greater = (order.kind == OrderType::StopLoss && order.side == OrderSide::Buy)
            || (order.kind == OrderType::TakeProfit && order.side == OrderSide::Sell);
        if !order.price.is_finite()
            || order.price <= 0.
            || if greater {
                order.price <= reference
            } else {
                order.price >= reference
            }
        {
            return Err("GRVT stop not changed: replacement trigger is crossed".into());
        }
        if (order.reduce_only || order.close == Some(true))
            && !cx.store.positions().iter().any(|p| {
                p.symbol == order.symbol
                    && p.contracts > 0.
                    && p.side
                        == if order.side == OrderSide::Sell {
                            PositionSide::Long
                        } else {
                            PositionSide::Short
                        }
            })
        {
            return Err("GRVT stop not changed: protected position is unavailable".into());
        }
        Ok(())
    }
    // Legacy sends, including session refresh, share this veto before transport.
    // Managed claims will close the remaining host-signing latency window later.
    pub(super) fn guard_protective_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        edit: &Edit,
    ) -> Result<(), String> {
        let current = cx
            .store
            .order(&edit.observed.id)
            .ok_or("GRVT stop not changed: original order disappeared")?;
        if !matches!(current.status, OrderStatus::Open | OrderStatus::Canceling)
            || current.filled != 0.
        {
            return Err("GRVT stop not changed: original order filled or is no longer open".into());
        }
        let mut snapshot = current.clone();
        snapshot.status = edit.observed.status;
        if snapshot != edit.observed
            || self
                .account
                .order_progress(&edit.observed.id)
                .is_some_and(|progress| progress.updated_ns > edit.inspected_updated_ns)
        {
            return Err("GRVT stop not changed: original order changed".into());
        }
        self.protective_reference(cx, &edit.replacement)
    }
    pub(super) fn protective_restore(
        &self,
        cx: &Cx<'_, Pending>,
        edit: &Edit,
    ) -> Result<Step, VenueError> {
        if !edit.standalone || edit.original.parent_id.is_some() || edit.original.filled != 0. {
            return Err("GRVT attached stop must be re-placed".into());
        }
        self.protective_reference(cx, &edit.original)?;
        let source = &edit.original;
        let opts = PlaceOrderOpts {
            symbol: source.symbol.clone(),
            kind: source.kind,
            side: source.side,
            amount: source.amount,
            price: Some(source.price),
            trigger_price: Some(source.price),
            stop_loss: None,
            take_profit: None,
            reduce_only: Some(source.reduce_only),
            time_in_force: source.time_in_force,
            trigger_type: source.trigger_type,
            tp_trigger_type: source.trigger_type,
            sl_trigger_type: source.trigger_type,
            client_order_id: None,
            max_slippage_pct: None,
            close: source.close,
            stp_mode: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
        };
        let mut step =
            self.placement(cx, &opts, None, Some(edit.execution), &mut HashSet::new())?;
        let original: NativeOrder<'_> = serde_json::from_str(&edit.native)
            .map_err(|_| "GRVT stop must be re-placed: exact standalone provenance unavailable")?;
        let mut body: RestoreBody<'_> =
            serde_json::from_str(&step.body).map_err(|_| "GRVT restore encoding unavailable")?;
        let wire = &mut body.o;
        if wire.sa != original.sa
            || wire.im != original.im
            || wire.ti != original.ti
            || wire.po != original.po
            || wire.ro != original.ro
            || wire.l != original.l
            || wire.b != original.b
            || wire.bf != original.bf
            || wire.m.t != original.m.t
        {
            return Err(
                "GRVT stop must be re-placed: exact standalone provenance unavailable".into(),
            );
        }
        if !valid_client(&original.m.co) {
            return Err("GRVT original client identity unavailable".into());
        }
        let client = original.m.co.to_string();
        wire.m.co = client.clone().into();
        let encoded = serde_json::to_string(&body).map_err(|_| "GRVT restore encoding failed")?;
        step.body = encoded;
        if let Effect::Place(order) = &mut step.effect {
            order.id = client.clone();
            order.client_order_id = Some(client);
        }
        Ok(step)
    }
}
