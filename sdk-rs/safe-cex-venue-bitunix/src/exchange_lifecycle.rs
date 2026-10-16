//! Exchange orchestration over opaque websocket and request values.
use std::{cell::RefCell, future::Future, rc::Rc};

pub fn dispose<W, E, Get: FnMut(usize) -> Result<Option<W>, E>>(
    base: impl FnOnce() -> Result<(), E>,
    public: impl FnOnce() -> Result<(), E>,
    secondary: impl FnOnce() -> Result<(), E>,
    shards: impl FnOnce() -> Result<(usize, Get), E>,
    mut dispose_shard: impl FnMut(W) -> Result<(), E>,
    futures: impl FnOnce() -> Result<(), E>,
    private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    base()?;
    public()?;
    secondary()?;
    let (length, mut get) = shards()?;
    for index in 0..length {
        if let Some(websocket) = get(index)? {
            dispose_shard(websocket)?;
        }
    }
    futures()?;
    private()
}

pub fn configure_public_websocket_shards<
    M,
    S,
    W,
    E,
    Get: FnMut(usize) -> Result<Option<Rc<W>>, E>,
>(
    old_shards: impl FnOnce() -> Result<(usize, Get), E>,
    mut dispose_shard: impl FnMut(Rc<W>) -> Result<(), E>,
    replace_shards: impl FnOnce() -> Result<(), E>,
    (markets, mut symbol): (
        impl FnOnce() -> Result<Vec<M>, E>,
        impl FnMut(M) -> Result<S, E>,
    ),
    (mut public, mut secondary): (
        impl FnMut() -> Result<Rc<W>, E>,
        impl FnMut() -> Result<Rc<W>, E>,
    ),
    mut set_symbols: impl FnMut(&Rc<W>, &Rc<RefCell<Vec<S>>>) -> Result<(), E>,
    mut create: impl FnMut(bool, &Rc<RefCell<Vec<S>>>) -> Result<Rc<W>, E>,
    mut push_pair: impl FnMut(Rc<W>, Rc<W>) -> Result<(), E>,
    active_shards: impl FnOnce() -> Result<Vec<Rc<W>>, E>,
) -> Result<Vec<Rc<W>>, E> {
    let (length, mut get) = old_shards()?;
    for index in 0..length {
        if let Some(websocket) = get(index)? {
            dispose_shard(websocket)?;
        }
    }
    replace_shards()?;
    let mut symbols = markets()?
        .into_iter()
        .map(&mut symbol)
        .collect::<Result<Vec<_>, E>>()?
        .into_iter();
    let mut chunks = Vec::new();
    loop {
        let shard = symbols
            .by_ref()
            .take(crate::websocket_public::BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION as usize)
            .collect::<Vec<_>>();
        if shard.is_empty() {
            break;
        }
        chunks.push(Rc::new(RefCell::new(shard)));
    }
    let first = chunks.first().cloned().unwrap_or_default();
    set_symbols(&public()?, &first)?;
    set_symbols(&secondary()?, &first)?;
    for symbols in chunks.into_iter().skip(1) {
        let primary = create(false, &symbols)?;
        let secondary = create(true, &symbols)?;
        push_pair(primary, secondary)?;
    }
    let mut result = vec![public()?, secondary()?];
    result.extend(active_shards()?);
    Ok(result)
}

pub fn listen_order_book<S, C, R, E>(
    symbol: S,
    callback: C,
    listen: impl FnOnce(S, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(symbol, callback)
}

pub struct IncomeHistoryRange<I> {
    pub start_time: f64,
    pub end_time: f64,
    pub income_type: Option<I>,
}

pub async fn fetch_income_history_range<I, R, E, F: Future<Output = Result<R, E>>>(
    start_time: f64,
    end_time: f64,
    income_type: Option<I>,
    fetch: impl FnOnce(IncomeHistoryRange<I>) -> F,
) -> Result<R, E> {
    fetch(IncomeHistoryRange {
        start_time,
        end_time,
        income_type,
    })
    .await
}
