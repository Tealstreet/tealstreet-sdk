use std::{future::Future, pin::Pin, rc::Rc};

pub type AwaitValue<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;

pub struct OrderReadOps<'a, V, E> {
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, E> + 'a>,
    pub capture: Rc<dyn Fn(&str) -> Result<V, E> + 'a>,
    pub invoke: Rc<dyn Fn(V, Vec<V>) -> Result<V, E> + 'a>,
    pub await_value: Rc<dyn Fn(V) -> AwaitValue<'a, V, E> + 'a>,
    pub intrinsic: Rc<dyn Fn(&str, Vec<V>) -> Result<V, E> + 'a>,
    pub for_of: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<(), E> + 'a>) -> Result<(), E> + 'a>,
    pub truthy: Rc<dyn Fn(&V) -> bool + 'a>,
    pub nullish: Rc<dyn Fn(&V) -> bool + 'a>,
    pub equal: Rc<dyn Fn(&V, &V) -> bool + 'a>,
    pub literal: Rc<dyn Fn(&str) -> V + 'a>,
    pub number: Rc<dyn Fn(f64) -> V + 'a>,
    pub undefined: Rc<dyn Fn() -> V + 'a>,
    pub array: Rc<dyn Fn(Vec<V>) -> V + 'a>,
    pub object: Rc<dyn Fn(Vec<(&'static str, V)>) -> V + 'a>,
    pub error_message: Rc<dyn Fn(&E) -> Result<V, E> + 'a>,
    pub parallel_map:
        Rc<dyn Fn(V, V, Rc<dyn Fn(V) -> AwaitValue<'a, V, E> + 'a>) -> AwaitValue<'a, V, E> + 'a>,
    pub map_position: Rc<dyn Fn(V, V, V, V) -> Result<V, E> + 'a>,
}

pub async fn fetch_orders<'a, V: Clone + 'a, E: 'a>(h: Rc<OrderReadOps<'a, V, E>>) -> Result<V, E> {
    let loaded = async {
        let normal = (h.capture)("this.fetchNormalOrders")?;
        let normal = (h.await_value)((h.invoke)(normal, vec![])?).await?;
        let mut groups = vec![normal];
        for kind in ["oco", "conditional", "trigger", "move_order_stop"] {
            let callee = (h.capture)("this.fetchAlgoOrders")?;
            groups.push((h.await_value)((h.invoke)(callee, vec![(h.literal)(kind)])?).await?);
        }
        (h.intrinsic)(
            "array-spread",
            vec![
                groups[0].clone(),
                groups[1].clone(),
                groups[4].clone(),
                groups[2].clone(),
                groups[3].clone(),
            ],
        )
    }
    .await;
    match loaded {
        Ok(value) => Ok(value),
        Err(error) => {
            let emitter = (h.capture)("this.emitAndLogError")?;
            let message = (h.error_message)(&error)?;
            (h.invoke)(emitter, vec![message])?;
            (h.capture)("this.store.orders")
        }
    }
}

pub async fn fetch_normal_instrument<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    instrument: V,
) -> Result<V, E> {
    let mut orders = (h.array)(vec![]);
    loop {
        let callee = (h.capture)("xhr.get:UNFILLED_ORDERS")?;
        let after = if (h.truthy)(&(h.get)(&orders, "length")?) {
            let last = (h.intrinsic)("last-index", vec![orders.clone()])?;
            (h.get)(&last, "ordId")?
        } else {
            (h.undefined)()
        };
        let params = (h.object)(vec![("instType", instrument.clone()), ("after", after)]);
        let reply = (h.await_value)((h.invoke)(
            callee,
            vec![(h.object)(vec![("params", params)])],
        )?)
        .await?;
        let data = (h.intrinsic)("response-data-data", vec![reply])?;
        let full = (h.equal)(&(h.get)(&data, "length")?, &(h.number)(100.0));
        orders = (h.intrinsic)("array-spread", vec![orders, data])?;
        if !full {
            return Ok(orders);
        }
    }
}

pub async fn fetch_normal_orders<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
) -> Result<V, E> {
    let flatten = (h.capture)("lodash.flatten")?;
    let parallel = (h.capture)("p-iteration.map")?;
    let callee = (h.capture)("this.getInstrumentTypes")?;
    let instruments = (h.invoke)(callee, vec![])?;
    let shared = h.clone();
    let response = (h.parallel_map)(
        parallel,
        instruments,
        Rc::new(move |instrument| Box::pin(fetch_normal_instrument(shared.clone(), instrument))),
    )
    .await?;
    let flat = (h.invoke)(flatten, vec![response])?;
    let map = (h.capture)("this.mapOrders")?;
    (h.invoke)(map, vec![flat])
}

pub async fn fetch_algo_instrument<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    instrument: V,
    kind: V,
) -> Result<V, E> {
    let mut orders = (h.array)(vec![]);
    loop {
        let callee = (h.capture)("xhr.get:UNFILLED_ALGO_ORDERS")?;
        let after = if (h.truthy)(&(h.get)(&orders, "length")?) {
            let last = (h.intrinsic)("last-index", vec![orders.clone()])?;
            (h.get)(&last, "algoId")?
        } else {
            (h.undefined)()
        };
        let params = (h.object)(vec![
            ("instType", instrument.clone()),
            ("ordType", kind.clone()),
            ("after", after),
        ]);
        let reply = (h.await_value)((h.invoke)(
            callee,
            vec![(h.object)(vec![("params", params)])],
        )?)
        .await?;
        let data = (h.intrinsic)("response-data-data", vec![reply])?;
        let full = (h.equal)(&(h.get)(&data, "length")?, &(h.number)(100.0));
        orders = (h.intrinsic)("array-spread", vec![orders, data])?;
        if !full {
            return Ok(orders);
        }
    }
}

pub async fn fetch_algo_orders<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    kind: V,
) -> Result<V, E> {
    let flatten = (h.capture)("lodash.flatten")?;
    let parallel = (h.capture)("p-iteration.map")?;
    let callee = (h.capture)("this.getInstrumentTypes")?;
    let instruments = (h.invoke)(callee, vec![])?;
    let shared = h.clone();
    let response = (h.parallel_map)(
        parallel,
        instruments,
        Rc::new(move |instrument| {
            Box::pin(fetch_algo_instrument(
                shared.clone(),
                instrument,
                kind.clone(),
            ))
        }),
    )
    .await?;
    let flat = (h.invoke)(flatten, vec![response])?;
    let map = (h.capture)("this.mapOrders")?;
    (h.invoke)(map, vec![flat])
}

pub async fn fetch_historical_orders<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    opts: V,
) -> Result<V, E> {
    if (h.truthy)(&(h.capture)("this.isPublicDataOnly")?) {
        return Ok((h.array)(vec![]));
    }
    let symbol = (h.intrinsic)("opts-optional-symbol", vec![opts.clone()])?;
    let market = if (h.truthy)(&symbol) {
        (h.intrinsic)("market-by-opts-symbol", vec![opts.clone()])?
    } else {
        (h.undefined)()
    };
    if (h.truthy)(&(h.intrinsic)("opts-optional-symbol", vec![opts.clone()])?)
        && !(h.truthy)(&market)
    {
        return Ok((h.array)(vec![]));
    }
    let loaded = async {
        let orders = (h.array)(vec![]);
        let seen = (h.intrinsic)("new-Set", vec![])?;
        let mut after = (h.undefined)();
        let mut page = 0;
        while page < 50 {
            let omit = (h.capture)("omitUndefined")?;
            let params = (h.object)(vec![("instType",(h.literal)("SWAP")),("instId",(h.intrinsic)("optional-id",vec![market.clone()])?),("begin",(h.intrinsic)("opts-optional-from",vec![opts.clone()])?),("end",(h.intrinsic)("opts-optional-to",vec![opts.clone()])?),("limit",(h.number)(100.0)),("after",after.clone())]);
            let params = (h.invoke)(omit,vec![params])?;
            let callee = (h.capture)("xhr.get:ORDERS_HISTORY")?;
            let response = (h.await_value)((h.invoke)(callee,vec![(h.object)(vec![("params",params)])])?).await?;
            let raw = (h.intrinsic)("response-data-data",vec![response])?;
            let rows = (h.intrinsic)("Array.isArray-or-empty",vec![raw])?;
            if (h.equal)(&(h.get)(&rows,"length")?,&(h.number)(0.0)) { break; }
        let ops=h.clone();
let orders=orders.clone();
let seen=seen.clone();
let market=market.clone();
(h.for_of)(rows.clone(),Rc::new(move |row| {
                let row_market = if (ops.nullish)(&market) { (ops.intrinsic)("market-by-row-instId",vec![row.clone()])? } else { market.clone() };
                let mapper = (ops.capture)("mapOkxHistoricalOrder")?;
                let to_base = (ops.capture)("this.contractsToBase:unbound")?;
                let order = (ops.invoke)(mapper,vec![row,row_market,to_base])?;
                if !(ops.truthy)(&order) { return Ok(()); }
                let has = (ops.intrinsic)("Set.has-order-id",vec![seen.clone(),order.clone()])?;
                if (ops.truthy)(&has) { return Ok(()); }
                (ops.intrinsic)("Set.add-order-id",vec![seen.clone(),order.clone()])?;
                (ops.intrinsic)("array-push",vec![orders.clone(),order])?;

Ok(())
}))?;
            if (h.truthy)(&(h.intrinsic)("less-than",vec![(h.get)(&rows,"length")?,(h.number)(100.0)])?) { break; }
            let cursor = (h.capture)("oldestCursorId")?;
            let cursor = (h.invoke)(cursor,vec![rows,(h.literal)("ordId")])?;
            let next_after = if (h.nullish)(&cursor) {(h.literal)("")}else{cursor};
            if !(h.truthy)(&next_after)||(h.equal)(&next_after,&after) { break; }
            after = next_after;
            let sleep = (h.capture)("sleep")?;
            (h.await_value)((h.invoke)(sleep,vec![(h.number)(100.0)])?).await?;
            page += 1;
        }
        if page >= 50 {let log=(h.capture)("this.log")?;(h.invoke)(log,vec![(h.literal)("fetchHistoricalOrders stopped at the 50-page cap; older orders were not fetched"),(h.capture)("LogSeverity.Warning")?])?;}
        let sorted=(h.intrinsic)("sort-createdTime-desc",vec![orders])?;
        let limit=(h.intrinsic)("opts-optional-limit",vec![opts.clone()])?;
        if (h.truthy)(&limit)&&(h.truthy)(&(h.intrinsic)("greater-than",vec![(h.get)(&opts,"limit")?,(h.number)(0.0)])?) {(h.intrinsic)("slice-zero-current-limit",vec![sorted,opts.clone()])}else{Ok(sorted)}
    }.await;
    match loaded {
        Ok(v) => Ok(v),
        Err(e) => {
            let emitter = (h.capture)("this.emitAndLogError")?;
            let message = (h.error_message)(&e)?;
            (h.invoke)(emitter, vec![message])?;
            Ok((h.array)(vec![]))
        }
    }
}

pub async fn fetch_closed_position_rows<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    opts: V,
) -> Result<V, E> {
    if (h.truthy)(&(h.capture)("this.isPublicDataOnly")?) {
        return Ok((h.array)(vec![]));
    }
    let symbol = (h.get)(&opts, "symbol")?;
    let market = if (h.truthy)(&symbol) {
        (h.intrinsic)("market-by-opts-symbol", vec![opts.clone()])?
    } else {
        (h.undefined)()
    };
    if (h.truthy)(&(h.get)(&opts, "symbol")?) && !(h.truthy)(&market) {
        return Ok((h.array)(vec![]));
    }
    let collected = (h.array)(vec![]);
    let to = (h.get)(&opts, "to")?;
    let mut after = if (h.truthy)(&to) {
        (h.intrinsic)("String-current-to", vec![opts.clone()])?
    } else {
        (h.undefined)()
    };
    for _ in 0..100 {
        let omit = (h.capture)("omitUndefined")?;
        let params = (h.object)(vec![
            ("instType", (h.literal)("SWAP")),
            (
                "instId",
                (h.intrinsic)("optional-id", vec![market.clone()])?,
            ),
            ("before", (h.get)(&opts, "from")?),
            ("after", after.clone()),
            ("limit", (h.number)(100.0)),
        ]);
        let params = (h.invoke)(omit, vec![params])?;
        let callee = (h.capture)("xhr.get:POSITIONS_HISTORY")?;
        let response = (h.await_value)((h.invoke)(
            callee,
            vec![(h.object)(vec![("params", params)])],
        )?)
        .await?;
        let rows = (h.intrinsic)("response-data-data", vec![response])?;
        let rows = (h.intrinsic)("Array.isArray-or-empty", vec![rows])?;
        if (h.equal)(&(h.get)(&rows, "length")?, &(h.number)(0.0)) {
            break;
        }
        (h.intrinsic)("array-push-spread", vec![collected.clone(), rows.clone()])?;
        if (h.truthy)(&(h.intrinsic)(
            "less-than",
            vec![(h.get)(&rows, "length")?, (h.number)(100.0)],
        )?) {
            break;
        }
        let oldest = (h.intrinsic)("reduce-oldest-positive-uTime", vec![rows])?;
        if !(h.truthy)(&(h.intrinsic)("Number.isFinite", vec![oldest.clone()])?) {
            break;
        }
        let next_after = (h.intrinsic)("String-oldest-minus-one", vec![oldest])?;
        if (h.equal)(&next_after, &after) {
            break;
        }
        after = next_after;
        let sleep = (h.capture)("sleep")?;
        (h.await_value)((h.invoke)(sleep, vec![(h.number)(100.0)])?).await?;
    }
    Ok(collected)
}

pub async fn fetch_historical_positions<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    opts: V,
) -> Result<V, E> {
    let reader = (h.capture)("this.fetchClosedPositionRows")?;
    let options = (h.object)(vec![
        ("from", (h.get)(&opts, "from")?),
        ("to", (h.get)(&opts, "to")?),
    ]);
    let rows = (h.await_value)((h.invoke)(reader, vec![options])?).await?;
    let records = (h.array)(vec![]);
    let ops = h.clone();
    let record_sink = records.clone();
    let opts = opts.clone();
    (h.for_of)(
        rows,
        Rc::new(move |row| {
            let market = (ops.intrinsic)("optional-market-by-row-instId", vec![row.clone()])?;
            if !(ops.truthy)(&market) || (ops.truthy)(&(ops.get)(&market, "isInverse")?) {
                return Ok(());
            }
            let mapper = (ops.capture)("mapOkxPositionHistory")?;
            let account = (ops.get)(&opts, "account")?;
            let record = (ops.map_position)(mapper, row, market, account)?;
            if (ops.truthy)(&record) {
                (ops.intrinsic)("array-push", vec![record_sink.clone(), record])?;
            }

            Ok(())
        }),
    )?;
    Ok(records)
}

pub async fn fetch_closed_pnl<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    opts: V,
) -> Result<V, E> {
    if (h.truthy)(&(h.capture)("this.isPublicDataOnly")?) {
        return Ok((h.array)(vec![]));
    }
    let symbol = (h.get)(&opts, "symbol")?;
    let market = if (h.truthy)(&symbol) {
        (h.intrinsic)("market-by-opts-symbol", vec![opts.clone()])?
    } else {
        (h.undefined)()
    };
    if (h.truthy)(&(h.get)(&opts, "symbol")?) && !(h.truthy)(&market) {
        return Ok((h.array)(vec![]));
    }
    let loaded=async{
        let entries=(h.array)(vec![]);
        let to=(h.get)(&opts,"to")?;
        let mut after=if (h.truthy)(&to){(h.intrinsic)("String-current-to",vec![opts.clone()])?}else{(h.undefined)()};
        let mut page=0;
        while page<100 {
            let omit=(h.capture)("omitUndefined")?;
            let params=(h.object)(vec![("instType",(h.literal)("SWAP")),("instId",(h.intrinsic)("optional-id",vec![market.clone()])?),("before",(h.get)(&opts,"from")?),("after",after.clone()),("limit",(h.number)(100.0))]);
            let params=(h.invoke)(omit,vec![params])?;
            let callee=(h.capture)("xhr.get:POSITIONS_HISTORY")?;
            let response=(h.await_value)((h.invoke)(callee,vec![(h.object)(vec![("params",params)])])?).await?;
            let rows=(h.intrinsic)("response-data-data",vec![response])?;
            let rows=(h.intrinsic)("Array.isArray-or-empty",vec![rows])?;
            if (h.equal)(&(h.get)(&rows,"length")?,&(h.number)(0.0)){break;}
        let ops=h.clone();
let entries=entries.clone();
let market=market.clone();
let opts=opts.clone();
(h.for_of)(rows.clone(),Rc::new(move |row| {
                let row_market=if (ops.nullish)(&market){(ops.intrinsic)("market-by-row-instId",vec![row.clone()])?}else{market.clone()};
                let mapper=(ops.capture)("mapOkxClosedPosition")?;
                let to_base=(ops.intrinsic)("contractsToBase-bind",vec![])?;
                let account=(ops.get)(&opts,"accountName")?;
                let account=if (ops.truthy)(&account){account}else{(ops.literal)("")};
                let entry=(ops.invoke)(mapper,vec![row,row_market,to_base,account])?;
                if (ops.truthy)(&entry){(ops.intrinsic)("array-push",vec![entries.clone(),entry])?;}

Ok(())
}))?;
            if (h.truthy)(&(h.intrinsic)("less-than",vec![(h.get)(&rows,"length")?,(h.number)(100.0)])?){break;}
            let oldest=(h.intrinsic)("reduce-oldest-positive-uTime",vec![rows])?;
            if !(h.truthy)(&(h.intrinsic)("Number.isFinite",vec![oldest.clone()])?){break;}
            let next_after=(h.intrinsic)("String-oldest-minus-one",vec![oldest])?;
            if (h.equal)(&next_after,&after){break;}
            after=next_after;
            let sleep=(h.capture)("sleep")?;
            (h.await_value)((h.invoke)(sleep,vec![(h.number)(100.0)])?).await?;
            page+=1;
        }
        if page>=100{let log=(h.capture)("this.log")?;(h.invoke)(log,vec![(h.literal)("fetchClosedPnl stopped at the 100-page cap; older positions were not fetched"),(h.capture)("LogSeverity.Warning")?])?;}
        (h.intrinsic)("sort-closedTime-desc",vec![entries])
    }.await;
    match loaded {
        Ok(v) => Ok(v),
        Err(e) => {
            let emitter = (h.capture)("this.emitAndLogError")?;
            let message = (h.error_message)(&e)?;
            (h.invoke)(emitter, vec![message])?;
            Ok((h.array)(vec![]))
        }
    }
}

pub async fn fetch_income_history_range<'a, V: Clone + 'a, E: 'a>(
    h: Rc<OrderReadOps<'a, V, E>>,
    start: V,
    end: V,
    income_type: V,
) -> Result<V, E> {
    let dedupe = (h.intrinsic)("new-Map", vec![])?;
    let mut current = start;
    while (h.truthy)(&(h.intrinsic)(
        "less-than",
        vec![current.clone(), end.clone()],
    )?) {
        let current_end = (h.intrinsic)(
            "Math.min-current-plus-seven-days",
            vec![current.clone(), end.clone(), (h.number)(604800000.0)],
        )?;
        let reader = (h.capture)("this.fetchIncomeHistory")?;
        let opts = (h.object)(vec![
            ("startTime", current),
            ("endTime", current_end.clone()),
            ("incomeType", income_type.clone()),
        ]);
        let records = (h.await_value)((h.invoke)(reader, vec![opts])?).await?;
        let ops = h.clone();
        let dedupe = dedupe.clone();
        (h.for_of)(
            records,
            Rc::new(move |record| {
                (ops.intrinsic)("Map.set-tranId-record", vec![dedupe.clone(), record])?;

                Ok(())
            }),
        )?;
        current = (h.intrinsic)("add-one", vec![current_end])?;
        let sleep = (h.capture)("sleep")?;
        (h.await_value)((h.invoke)(sleep, vec![(h.number)(100.0)])?).await?;
    }
    let values = (h.intrinsic)("Map.values-spread", vec![dedupe])?;
    (h.intrinsic)("sort-timestamp-desc", vec![values])
}
