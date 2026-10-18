//! Cancellation method bodies; REST promises and receiver operations are supplied.
use std::{cell::RefCell, collections::HashSet, future::Future, pin::Pin, rc::Rc};

pub type CancelKey = Vec<u16>;
pub type CancelOrders<O> = Rc<RefCell<Vec<O>>>;
pub type CancelFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + 'a>>;
#[derive(Clone, Debug)]
pub enum CancelArgument<V> {
    Value(V),
    String(CancelKey),
    OrderIds(Vec<CancelKey>),
}
pub type CancelPayload<V> = Vec<(&'static str, CancelArgument<V>)>;
pub type CancelGroups<O> = Vec<(CancelKey, Vec<O>)>;

/// Source property/call sites plus object-spread/property language operations.
/// Futures are promise wait handles: the host owns started work independently
/// of waiter cancellation. Polling these bodies is their async execution boundary.
pub trait CancellationHost {
    type Error;
    type Order: Clone;
    type Value: Clone;
    /// Array element read after a dense array has shrunk yields undefined.
    fn undefined_order() -> Self::Order;
    fn ensure_account_type(&self) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn uses_uta(&self) -> Result<bool, Self::Error>;
    /// Capture the Promise constructor/all method before operand calls.
    fn promise_all_function(&self) -> Result<Self::Value, Self::Error>;
    /// Bind the source Promise.all intrinsic: original first rejection,
    /// fulfillment order and independently owned work; not a polling race approximation.
    fn promise_all<'a>(
        &'a self,
        function: Self::Value,
        normal: CancelFuture<'a, CancelOrders<Self::Order>, Self::Error>,
        strategy: CancelFuture<'a, CancelOrders<Self::Order>, Self::Error>,
    ) -> Result<
        CancelFuture<'a, (CancelOrders<Self::Order>, CancelOrders<Self::Order>), Self::Error>,
        Self::Error,
    >;
    fn fetch_uta_normal(
        &self,
    ) -> Result<CancelFuture<'_, CancelOrders<Self::Order>, Self::Error>, Self::Error>;
    fn fetch_uta_strategy(
        &self,
    ) -> Result<CancelFuture<'_, CancelOrders<Self::Order>, Self::Error>, Self::Error>;
    fn cancel_normal(
        &self,
        orders: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn cancel_algo(
        &self,
        orders: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn cancel_uta_normal(
        &self,
        orders: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn cancel_uta_strategy(
        &self,
        orders: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn cancel_orders(
        &self,
        orders: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn virtual_tpsl(&self, order: &Self::Order) -> Result<bool, Self::Error>;
    fn algo(&self, order: &Self::Order) -> Result<bool, Self::Error>;
    fn spread(&self, order: &Self::Order) -> Result<Self::Order, Self::Error>;
    fn id(&self, order: &Self::Order) -> Result<CancelKey, Self::Error>;
    fn with_id(&self, spread: Self::Order, id: CancelKey) -> Self::Order;
    fn is_terminal_order(&self, order: &Self::Order) -> Result<bool, Self::Error>;
    fn symbol(&self, order: &Self::Order) -> Result<CancelKey, Self::Error>;
    fn info_order_id(&self, order: &Self::Order) -> Result<Self::Value, Self::Error>;
    fn info_client_oid(&self, order: &Self::Order) -> Result<Self::Value, Self::Error>;
    fn truthy(value: &Self::Value) -> bool;
    fn is_undefined(value: &Self::Value) -> bool;
    fn undefined() -> Self::Value;
    fn key_value(key: CancelKey) -> Self::Value;
    fn equals_key(value: &Self::Value, key: &CancelKey) -> bool;
    fn product_type(&self) -> Result<Self::Value, Self::Error>;
    fn margin_coin(&self) -> Result<Self::Value, Self::Error>;
    fn uta_category(&self) -> Result<Self::Value, Self::Error>;
    fn plan_type(&self, order: &Self::Order) -> Result<Self::Value, Self::Error>;
    fn margin_coin_for_symbol(&self, symbol: &CancelKey) -> Result<Self::Value, Self::Error>;
    fn inverse_plan_type(&self, order: &Self::Order) -> Result<Self::Value, Self::Error>;
    fn market_bases(&self) -> Result<Vec<CancelKey>, Self::Error>;
    fn store_orders(&self) -> Result<CancelOrders<Self::Order>, Self::Error>;
    /// The ordinary-object reduce step: if(!acc[key]) acc[key]=[];
    /// acc[key].push(order). Keeps prototype/inherited property failures.
    fn inverse_group_prepare(
        &self,
        groups: &mut CancelGroups<Self::Order>,
        key: &CancelKey,
    ) -> Result<Self::Value, Self::Error>;
    fn inverse_group_push(
        &self,
        groups: &mut CancelGroups<Self::Order>,
        key: CancelKey,
        order: Self::Order,
        push: Self::Value,
    ) -> Result<(), Self::Error>;
    fn post(
        &self,
        endpoint: &'static str,
        payload: CancelPayload<Self::Value>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error>;
    fn schedule_write_check(&self) -> Result<(), Self::Error>;
    /// err?.response?.data?.msg || err?.message, including original failures.
    fn error_message(&self, error: &Self::Error) -> Result<Self::Value, Self::Error>;
    fn emit_error(&self, message: Self::Value) -> Result<(), Self::Error>;
}
fn endpoint(name: &str) -> &'static str {
    crate::protocol::ENDPOINTS
        .iter()
        .find(|p| p.name == name)
        .expect("maintained Bitget endpoint")
        .path
}
fn emit<H: CancellationHost>(h: &H, e: &H::Error) -> Result<(), H::Error> {
    h.emit_error(h.error_message(e)?)
}
fn normalized<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<Vec<H::Order>, H::Error> {
    let mut out = Vec::new();
    let length = orders.borrow().len();
    for index in 0..length {
        let o = orders.borrow().get(index).cloned();
        let Some(o) = o else {
            continue;
        };
        let spread = h.spread(&o)?;
        let id = h.id(&o)?;
        let end = id
            .windows(2)
            .position(|x| x == [95, 95])
            .unwrap_or(id.len());
        out.push(h.with_id(spread, id[..end].to_vec()));
    }
    Ok(out)
}
fn unique<H: CancellationHost>(h: &H, orders: Vec<H::Order>) -> Result<Vec<H::Order>, H::Error> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for o in orders {
        if seen.insert(h.id(&o)?) {
            out.push(o);
        }
    }
    Ok(out)
}
fn group<H: CancellationHost>(
    h: &H,
    orders: Vec<H::Order>,
) -> Result<CancelGroups<H::Order>, H::Error> {
    let mut groups: CancelGroups<H::Order> = Vec::new();
    for o in orders {
        let key = h.symbol(&o)?;
        if let Some((_, items)) = groups.iter_mut().find(|(k, _)| k == &key) {
            items.push(o);
        } else {
            groups.push((key, vec![o]));
        }
    }
    Ok(entries(groups))
}
fn index(key: &[u16]) -> Option<u32> {
    if key.is_empty()
        || key.len() > 10
        || key.iter().any(|v| !(48..=57).contains(v))
        || (key.len() > 1 && key[0] == 48)
    {
        return None;
    }
    let value = key.iter().try_fold(0u64, |a, n| {
        a.checked_mul(10)?.checked_add(u64::from(n - 48))
    })?;
    (value < u64::from(u32::MAX)).then_some(value as u32)
}
fn entries<O>(mut groups: CancelGroups<O>) -> CancelGroups<O> {
    groups.sort_by_key(|(k, _)| index(k).map_or((1, 0), |i| (0, i)));
    groups
}
fn ids<H: CancellationHost>(h: &H, orders: &[H::Order]) -> Result<Vec<CancelKey>, H::Error> {
    orders.iter().map(|o| h.id(o)).collect()
}
// ts: BitgetExchange.cancelAllOrders
pub async fn cancel_all_orders<H: CancellationHost>(h: &H) -> Result<(), H::Error> {
    let result = async {
        h.ensure_account_type()?.await?;
        if h.uses_uta()? {
            let all = h.promise_all_function()?;
            let a = h.fetch_uta_normal()?;
            let b = h.fetch_uta_strategy()?;
            let (normal, strategy) = h.promise_all(all, a, b)?.await?;
            h.cancel_uta_normal(normal)?.await?;
            h.cancel_uta_strategy(strategy)?.await?;
            return Ok(());
        }
        let payload = vec![
            ("productType", CancelArgument::Value(h.product_type()?)),
            ("marginCoin", CancelArgument::Value(h.margin_coin()?)),
        ];
        h.post(endpoint("CANCEL_ALL_ORDERS"), payload)?.await?;
        h.schedule_write_check()?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        emit(h, &e)?;
    }
    Ok(())
}

// ts: BitgetExchange.cancelOrders
pub async fn cancel_orders<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<(), H::Error> {
    let (mut virtuals, mut others) = (vec![], vec![]);
    let length = orders.borrow().len();
    for index in 0..length {
        let o = orders
            .borrow()
            .get(index)
            .cloned()
            .unwrap_or_else(H::undefined_order);
        if h.virtual_tpsl(&o)? {
            virtuals.push(o);
        } else {
            others.push(o);
        }
    }
    let (mut algos, mut normal) = (vec![], vec![]);
    for o in others.drain(..) {
        if h.algo(&o)? {
            algos.push(o);
        } else {
            normal.push(o);
        }
    }
    normal.extend(virtuals);
    if !normal.is_empty() {
        h.cancel_normal(Rc::new(RefCell::new(normal)))?.await?;
    }
    if !algos.is_empty() {
        h.cancel_algo(Rc::new(RefCell::new(algos)))?.await?;
    }
    Ok(())
}

// ts: BitgetExchange.cancelNormalOrders
pub async fn cancel_normal_orders<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<(), H::Error> {
    h.ensure_account_type()?.await?;
    if h.uses_uta()? {
        return h.cancel_uta_normal(orders)?.await;
    }
    let orders = unique(h, normalized(h, orders)?)?;
    for (symbol, symbol_orders) in group(h, orders.clone())? {
        for lot in symbol_orders.chunks(50) {
            let order_ids = ids(h, lot)?;
            let result = async {
                let payload = vec![
                    ("symbol", CancelArgument::String(symbol.clone())),
                    ("orderIdList", CancelArgument::OrderIds(order_ids)),
                    ("marginCoin", CancelArgument::Value(h.margin_coin()?)),
                    ("productType", CancelArgument::Value(h.product_type()?)),
                ];
                h.post(endpoint("CANCEL_ORDERS"), payload)?.await
            }
            .await;
            if let Err(e) = result {
                emit(h, &e)?;
                return Err(e);
            }
        }
    }
    if !orders.is_empty() {
        h.schedule_write_check()?;
    }
    Ok(())
}

// ts: BitgetExchange.cancelUtaNormalOrders
pub async fn cancel_uta_normal_orders<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<(), H::Error> {
    let orders = unique(h, normalized(h, orders)?)?;
    for o in &orders {
        let result = async {
            let initial = h.info_order_id(o)?;
            let order_id = if H::truthy(&initial) {
                initial
            } else {
                let client = h.info_client_oid(o)?;
                if H::equals_key(&client, &h.id(o)?) {
                    H::undefined()
                } else {
                    H::key_value(h.id(o)?)
                }
            };
            let initial = h.info_client_oid(o)?;
            let client_oid = if H::truthy(&initial) {
                initial
            } else if !H::truthy(&order_id) {
                H::key_value(h.id(o)?)
            } else {
                H::undefined()
            };
            let mut payload = vec![
                ("category", CancelArgument::Value(h.uta_category()?)),
                ("orderId", CancelArgument::Value(order_id)),
                ("clientOid", CancelArgument::Value(client_oid)),
            ];
            payload.retain(|(_, v)| !matches!(v,CancelArgument::Value(x)if H::is_undefined(x)));
            h.post(endpoint("UTA_CANCEL_ORDER"), payload)?.await
        }
        .await;
        if let Err(e) = result {
            emit(h, &e)?;
            return Err(e);
        }
    }
    if !orders.is_empty() {
        h.schedule_write_check()?;
    }
    Ok(())
}

// ts: BitgetExchange.cancelAlgoOrders
pub async fn cancel_algo_orders<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<(), H::Error> {
    h.ensure_account_type()?.await?;
    if h.uses_uta()? {
        return h.cancel_uta_strategy(orders)?.await;
    }
    let mut index = 0;
    loop {
        let o = orders.borrow().get(index).cloned();
        let Some(o) = o else {
            break;
        };
        index += 1;
        let result = async {
            let plan = h.plan_type(&o)?;
            let payload = vec![
                ("orderId", CancelArgument::String(h.id(&o)?)),
                ("symbol", CancelArgument::String(h.symbol(&o)?)),
                ("marginCoin", CancelArgument::Value(h.margin_coin()?)),
                ("productType", CancelArgument::Value(h.product_type()?)),
                ("planType", CancelArgument::Value(plan)),
            ];
            h.post(endpoint("CANCEL_ALGO_ORDER"), payload)?.await
        }
        .await;
        if let Err(e) = result {
            emit(h, &e)?;
            return Err(e);
        }
    }
    if !orders.borrow().is_empty() {
        h.schedule_write_check()?;
    }
    Ok(())
}

// ts: BitgetExchange.cancelSymbolOrders
pub async fn cancel_symbol_orders<H: CancellationHost>(
    h: &H,
    symbol: CancelKey,
) -> Result<(), H::Error> {
    let result = async {
        let mut orders = Vec::new();
        let stored = h.store_orders()?;
        let length = stored.borrow().len();
        for index in 0..length {
            let Some(o) = stored.borrow().get(index).cloned() else {
                continue;
            };
            if h.symbol(&o)? == symbol && !h.is_terminal_order(&o)? {
                orders.push(o);
            }
        }
        h.cancel_orders(Rc::new(RefCell::new(orders)))?.await
    }
    .await;
    if let Err(e) = result {
        emit(h, &e)?;
        return Err(e);
    }
    Ok(())
}

// ts: BitgetInverseExchange.cancelAllOrders
pub async fn inverse_cancel_all_orders<H: CancellationHost>(h: &H) -> Result<(), H::Error> {
    let result = async {
        let mut seen = HashSet::new();
        let mut coins = Vec::new();
        for coin in h.market_bases()? {
            if seen.insert(coin.clone()) {
                coins.push(coin);
            }
        }
        for coin in coins {
            let payload = vec![
                ("productType", CancelArgument::Value(h.product_type()?)),
                ("marginCoin", CancelArgument::String(coin)),
            ];
            h.post(endpoint("CANCEL_ALL_ORDERS"), payload)?.await?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        emit(h, &e)?;
    }
    Ok(())
}

// ts: BitgetInverseExchange.cancelNormalOrders
pub async fn inverse_cancel_normal_orders<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<(), H::Error> {
    let mut groups = vec![];
    let length = orders.borrow().len();
    for index in 0..length {
        let o = orders.borrow().get(index).cloned();
        let Some(o) = o else {
            continue;
        };
        let key = h.symbol(&o)?;
        let push = h.inverse_group_prepare(&mut groups, &key)?;
        let spread = h.spread(&o)?;
        let id = h.id(&o)?;
        let end = id
            .windows(2)
            .position(|x| x == [95, 95])
            .unwrap_or(id.len());
        let o = h.with_id(spread, id[..end].to_vec());
        h.inverse_group_push(&mut groups, key, o, push)?;
    }
    for (symbol, symbol_orders) in entries(groups) {
        let margin = h.margin_coin_for_symbol(&symbol)?;
        let mut unique_orders = Vec::new();
        for (index, o) in symbol_orders.iter().enumerate() {
            let mut first = None;
            for (i, other) in symbol_orders.iter().enumerate() {
                if h.id(other)? == h.id(o)? {
                    first = Some(i);
                    break;
                }
            }
            if first == Some(index) {
                unique_orders.push(o.clone());
            }
        }
        for lot in unique_orders.chunks(50) {
            let order_ids = ids(h, lot)?;
            let result = async {
                let payload = vec![
                    ("symbol", CancelArgument::String(symbol.clone())),
                    ("orderIdList", CancelArgument::OrderIds(order_ids)),
                    ("marginCoin", CancelArgument::Value(margin.clone())),
                    ("productType", CancelArgument::Value(h.product_type()?)),
                ];
                h.post(endpoint("CANCEL_ORDERS"), payload)?.await
            }
            .await;
            if let Err(e) = result {
                emit(h, &e)?;
            }
        }
    }
    Ok(())
}

// ts: BitgetInverseExchange.cancelAlgoOrders
pub async fn inverse_cancel_algo_orders<H: CancellationHost>(
    h: &H,
    orders: CancelOrders<H::Order>,
) -> Result<(), H::Error> {
    let mut index = 0;
    loop {
        let o = orders.borrow().get(index).cloned();
        let Some(o) = o else {
            break;
        };
        index += 1;
        let result = async {
            let margin = h.margin_coin_for_symbol(&h.symbol(&o)?)?;
            let plan = h.inverse_plan_type(&o)?;
            let payload = vec![
                ("orderId", CancelArgument::String(h.id(&o)?)),
                ("symbol", CancelArgument::String(h.symbol(&o)?)),
                ("marginCoin", CancelArgument::Value(margin)),
                ("productType", CancelArgument::Value(h.product_type()?)),
                ("planType", CancelArgument::Value(plan)),
            ];
            h.post(endpoint("CANCEL_ALGO_ORDER"), payload)?.await
        }
        .await;
        if let Err(e) = result {
            emit(h, &e)?;
        }
    }
    Ok(())
}
