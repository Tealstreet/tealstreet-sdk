//! Cancellation and market lookup keep original inputs and results opaque.
use std::future::Future;
pub trait CancellationHost {
    type Input;
    type Order: Clone;
    type Value;
    type Reply;
    type Call;
    type Error;
    fn input_length(&mut self, input: &Self::Input) -> Result<usize, Self::Error>;
    fn input_at(
        &mut self,
        input: &Self::Input,
        index: usize,
    ) -> Result<Option<Self::Order>, Self::Error>;
    fn spread(&mut self, order: &Self::Order) -> Result<Self::Order, Self::Error>;
    fn id(&mut self, order: &Self::Order) -> Result<Vec<u16>, Self::Error>;
    fn set_id(&mut self, order: &mut Self::Order, id: Vec<u16>) -> Result<(), Self::Error>;
    fn missing_order(&mut self) -> Result<Self::Order, Self::Error>;
    fn position_side(
        &mut self,
        order: &Self::Order,
    ) -> impl Future<Output = Result<Self::Value, Self::Error>>;
    fn with_side(
        &mut self,
        order: &Self::Order,
        side: Self::Value,
    ) -> Result<Self::Order, Self::Error>;
    fn side(&mut self, order: &Self::Order) -> Result<Self::Value, Self::Error>;
    fn symbol(&mut self, order: &Self::Order) -> Result<Self::Value, Self::Error>;
    fn value_text(&mut self, value: Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn format(&mut self, symbol: Self::Value) -> Result<Self::Value, Self::Error>;
    fn format_value_ref(&mut self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn clone_value_ref(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn delete_target(&mut self) -> Result<Self::Call, Self::Error>;
    fn delete(
        &mut self,
        call: Self::Call,
        endpoint: &'static str,
        symbol: Self::Value,
        side: Self::Value,
        id: Vec<u16>,
    ) -> impl Future<Output = Result<Self::Reply, Self::Error>>;
    fn flatten(
        &mut self,
        replies: Vec<Self::Reply>,
        batch: bool,
    ) -> Result<Vec<Self::Reply>, Self::Error>;
    fn zero_code(&mut self, reply: &Self::Reply) -> Result<bool, Self::Error>;
    fn message(&mut self, reply: &Self::Reply) -> Result<Self::Value, Self::Error>;
    fn error(&mut self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn status(
        &mut self,
        ids: Vec<Option<Vec<u16>>>,
        status: &'static str,
    ) -> Result<(), Self::Error>;
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
}
fn endpoint(key: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
}
fn parent_id(id: Vec<u16>) -> Vec<u16> {
    let end = id
        .windows(2)
        .position(|w| w == [95, 95])
        .unwrap_or(id.len());
    id[..end].to_vec()
}
pub async fn cancel_orders<H: CancellationHost>(
    host: &mut H,
    input: H::Input,
) -> Result<(), H::Error> {
    let length = host.input_length(&input)?;
    let mut mapped = Vec::new();
    for index in 0..length {
        if let Some(order) = host.input_at(&input, index)? {
            let mut copy = host.spread(&order)?;
            let id = parent_id(host.id(&order)?);
            host.set_id(&mut copy, id.clone())?;
            mapped.push((Some(id), Some(copy)));
        } else {
            mapped.push((None, None));
        }
    }
    let mut seen = Vec::new();
    let mut orders = Vec::new();
    for (id, order) in mapped {
        if !seen.contains(&id) {
            seen.push(id);
            orders.push(match order {
                Some(order) => order,
                None => host.missing_order()?,
            });
        }
    }
    let result = async {
        let mut with_sides = Vec::new();
        for order in orders {
            let side = host.position_side(&order).await?;
            with_sides.push(host.with_side(&order, side)?);
        }
        let mut groups: Vec<(Vec<u16>, Vec<H::Order>)> = Vec::new();
        for order in with_sides {
            let side = host.side(&order)?;
            let mut key = host.value_text(side)?;
            key.push(58);
            let symbol = host.symbol(&order)?;
            key.extend(host.value_text(symbol)?);
            if let Some((_, orders)) = groups.iter_mut().find(|(k, _)| *k == key) {
                orders.push(order);
            } else {
                groups.push((key, vec![order]));
            }
        }
        let mut requests = Vec::new();
        for (_, orders) in groups {
            let symbol = host.symbol(&orders[0])?;
            let symbol = host.format(symbol)?;
            let side = host.side(&orders[0])?;
            let mut ids = Vec::new();
            for order in orders {
                ids.push(host.id(&order)?);
            }
            requests.push((symbol, side, ids));
        }
        let mut replies = Vec::new();
        for (symbol, side, ids) in requests {
            if ids.len() == 1 {
                let call = host.delete_target()?;
                let formatted = host.format(symbol)?;
                let reply = host
                    .delete(
                        call,
                        endpoint("CANCEL_ORDER"),
                        formatted,
                        side,
                        ids[0].clone(),
                    )
                    .await?;
                replies.extend(host.flatten(vec![reply], false)?);
            } else {
                let mut lots = Vec::new();
                for lot in ids.chunks(10) {
                    let mut id = Vec::new();
                    for (index, part) in lot.iter().enumerate() {
                        if index > 0 {
                            id.push(44);
                        }
                        id.extend(part);
                    }
                    let call = host.delete_target()?;
                    let formatted = host.format_value_ref(&symbol)?;
                    let position_side = host.clone_value_ref(&side)?;
                    lots.push(
                        host.delete(call, endpoint("BATCH_ORDERS"), formatted, position_side, id)
                            .await?,
                    );
                }
                replies.extend(host.flatten(lots, true)?);
            }
        }
        for reply in replies {
            if !host.zero_code(&reply)? {
                let message = host.message(&reply)?;
                return Err(host.error(message)?);
            }
        }
        let length = host.input_length(&input)?;
        let mut ids = Vec::new();
        for index in 0..length {
            ids.push(match host.input_at(&input, index)? {
                Some(order) => Some(host.id(&order)?),
                None => None,
            });
        }
        host.status(ids, "canceled")?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            host.report(&error)?;
            Err(error)
        }
    }
}
pub trait MarketLookupHost {
    type Symbol;
    type Market;
    type Markets;
    type Loaded;
    type Error;
    fn cached(&mut self, symbol: &Self::Symbol) -> Result<Option<Self::Market>, Self::Error>;
    fn fetch(&mut self) -> impl Future<Output = Result<Self::Markets, Self::Error>>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn is_array(&mut self, markets: &Self::Markets) -> Result<bool, Self::Error>;
    fn length(&mut self, markets: &Self::Markets) -> Result<usize, Self::Error>;
    fn loaded(&mut self) -> Result<Self::Loaded, Self::Error>;
    fn update(
        &mut self,
        markets: &Self::Markets,
        loaded: Self::Loaded,
        flag: (&'static str, bool),
    ) -> Result<(), Self::Error>;
    fn find(
        &mut self,
        markets: &Self::Markets,
        symbol: &Self::Symbol,
    ) -> Result<Option<Self::Market>, Self::Error>;
}
pub async fn get_or_fetch_market<H: MarketLookupHost>(
    host: &mut H,
    symbol: H::Symbol,
) -> Result<Option<H::Market>, H::Error> {
    if let Some(cached) = host.cached(&symbol)? {
        return Ok(Some(cached));
    }
    let markets = host.fetch().await?;
    if !host.disposed()? && host.is_array(&markets)? && host.length(&markets)? > 0 {
        let loaded = host.loaded()?;
        host.update(&markets, loaded, ("markets", true))?;
    }
    if let Some(cached) = host.cached(&symbol)? {
        return Ok(Some(cached));
    }
    host.find(&markets, &symbol)
}
#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn endpoint_lookup_preserves_selected_wire_paths() {
        assert_eq!(endpoint("BATCH_ORDERS"), "/g-orders");
        assert_eq!(endpoint("CANCEL_ORDER"), "/g-orders/cancel");
        assert_eq!(endpoint("ORDER"), "/g-orders/create");
        assert!(std::panic::catch_unwind(|| endpoint("not-an-endpoint")).is_err());
    }
}
