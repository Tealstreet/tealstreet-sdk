//! Cancellation control retains opaque order references across transport awaits.
use std::future::Future;
pub trait CancellationHost {
    type Input;
    type Order: Clone;
    type Body;
    type Options;
    type Error;
    fn input(&mut self, input: &Self::Input) -> Result<Vec<Self::Order>, Self::Error>;
    fn orders(&mut self) -> Result<Vec<Self::Order>, Self::Error>;
    fn attached_leg(&mut self, order: &Self::Order) -> Result<Option<&'static str>, Self::Error>;
    fn id(&mut self, order: &Self::Order) -> Result<Vec<u16>, Self::Error>;
    fn parent_id(&mut self, id: Vec<u16>) -> Result<Vec<u16>, Self::Error>;
    fn kind(&mut self, order: &Self::Order) -> Result<Option<&'static str>, Self::Error>;
    fn symbol(&mut self, order: &Self::Order) -> Result<Vec<u16>, Self::Error>;
    fn parent_body(&mut self, parent: &Self::Order, id: &[u16]) -> Result<Self::Body, Self::Error>;
    fn standalone_body(&mut self, order: &Self::Order) -> Result<Self::Body, Self::Error>;
    fn group_body(
        &mut self,
        symbol: &[u16],
        orders: &[Self::Order],
    ) -> Result<Self::Body, Self::Error>;
    fn post(
        &mut self,
        endpoint: &'static str,
        body: Self::Body,
    ) -> impl Future<Output = Result<(), Self::Error>>;
    fn update_status(
        &mut self,
        ids: Vec<Vec<u16>>,
        status: &'static str,
    ) -> Result<(), Self::Error>;
    fn recreate_options(
        &mut self,
        parent: &Self::Order,
        tp: Option<&Self::Order>,
        sl: Option<&Self::Order>,
    ) -> Result<Self::Options, Self::Error>;
    fn place(&mut self, options: Self::Options) -> impl Future<Output = Result<(), Self::Error>>;
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
}
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}
fn contains(id: &[u16], text: &str) -> bool {
    let needle: Vec<_> = text.encode_utf16().collect();
    id.windows(needle.len()).any(|w| w == needle)
}
fn index(key: &[u16]) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key[0] == 48) {
        return None;
    }
    let mut number = 0u64;
    for unit in key {
        if !(48..=57).contains(unit) {
            return None;
        }
        number = number.checked_mul(10)?.checked_add(u64::from(*unit - 48))?;
    }
    (number < u64::from(u32::MAX)).then_some(number as u32)
}
pub async fn cancel_orders<H: CancellationHost>(
    host: &mut H,
    input: H::Input,
) -> Result<(), H::Error> {
    let result = async {
        let mut virtual_orders = Vec::new();
        for order in host.input(&input)? {
            if host.attached_leg(&order)?.is_some() {
                virtual_orders.push(order);
            }
        }
        let mut regular = Vec::new();
        for order in host.input(&input)? {
            if host.attached_leg(&order)?.is_none() {
                regular.push(order);
            }
        }
        let mut parents: Vec<(Vec<u16>, H::Order, bool, bool)> = Vec::new();
        for order in &virtual_orders {
            let id = host.id(order)?;
            let parent_id = host.parent_id(id)?;
            let is_tp = host.attached_leg(order)? == Some("tp");
            let mut parent = None;
            for candidate in host.orders()? {
                if host.id(&candidate)? == parent_id {
                    parent = Some(candidate);
                    break;
                }
            }
            let Some(parent) = parent else {
                continue;
            };
            let mut prefix = parent_id.clone();
            prefix.extend("__".encode_utf16());
            let mut related = Vec::new();
            for candidate in host.orders()? {
                let id = host.id(&candidate)?;
                if id == parent_id || id.starts_with(&prefix) {
                    related.push(candidate);
                }
            }
            let mut tp = None;
            let mut sl = None;
            for candidate in &related {
                if contains(&host.id(candidate)?, "__tp") {
                    tp = Some(candidate);
                    break;
                }
            }
            for candidate in &related {
                if contains(&host.id(candidate)?, "__sl") {
                    sl = Some(candidate);
                    break;
                }
            }
            let mut keep_tp = false;
            let mut keep_sl = false;
            if let Some(tp) = tp {
                if !is_tp {
                    keep_tp = true;
                    for order in &virtual_orders {
                        if host.id(order)? == host.id(tp)? {
                            keep_tp = false;
                            break;
                        }
                    }
                }
            }
            if let Some(sl) = sl {
                if is_tp {
                    keep_sl = true;
                    for order in &virtual_orders {
                        if host.id(order)? == host.id(sl)? {
                            keep_sl = false;
                            break;
                        }
                    }
                }
            }
            if let Some(existing) = parents.iter_mut().find(|(id, _, _, _)| id == &parent_id) {
                *existing = (parent_id, parent, keep_tp, keep_sl);
            } else {
                parents.push((parent_id, parent, keep_tp, keep_sl));
            }
        }
        for (id, parent, _, _) in &parents {
            let body = host.parent_body(parent, id)?;
            host.post(endpoint("CANCEL_ORDER"), body).await?;
        }
        let mut standalone = Vec::new();
        for order in &regular {
            if matches!(
                host.kind(order)?,
                Some("take_profit_market" | "stop_market")
            ) {
                standalone.push(order);
            }
        }
        for order in standalone {
            let body = host.standalone_body(order)?;
            host.post(endpoint("CANCEL_ORDER_TP_SL"), body).await?;
        }
        let mut limit_market = Vec::new();
        for order in &regular {
            if matches!(host.kind(order)?, Some("limit" | "market")) {
                limit_market.push(order.clone());
            }
        }
        let mut groups: Vec<(Vec<u16>, Vec<H::Order>)> = Vec::new();
        for order in limit_market {
            let symbol = host.symbol(&order)?;
            if let Some((_, orders)) = groups.iter_mut().find(|(key, _)| key == &symbol) {
                orders.push(order);
            } else {
                groups.push((symbol, vec![order]));
            }
        }
        groups.sort_by(|(a, _), (b, _)| match (index(a), index(b)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });
        for (symbol, orders) in groups {
            let body = host.group_body(&symbol, &orders)?;
            host.post(endpoint("CANCEL_ORDER"), body).await?;
        }
        let mut ids = Vec::new();
        for order in host.input(&input)? {
            ids.push(host.id(&order)?);
        }
        host.update_status(ids, "canceled")?;
        for (id, parent, keep_tp, keep_sl) in &parents {
            let mut canceled = false;
            for order in &regular {
                if host.id(order)? == *id {
                    canceled = true;
                    break;
                }
            }
            if canceled || (!keep_tp && !keep_sl) {
                continue;
            }
            let mut tp_id = id.clone();
            tp_id.extend("__tp".encode_utf16());
            let mut sl_id = id.clone();
            sl_id.extend("__sl".encode_utf16());
            let mut tp = None;
            let mut sl = None;
            for order in host.orders()? {
                if host.id(&order)? == tp_id {
                    tp = Some(order);
                    break;
                }
            }
            for order in host.orders()? {
                if host.id(&order)? == sl_id {
                    sl = Some(order);
                    break;
                }
            }
            let options = host.recreate_options(
                parent,
                if *keep_tp { tp.as_ref() } else { None },
                if *keep_sl { sl.as_ref() } else { None },
            )?;
            host.place(options).await?;
        }
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
