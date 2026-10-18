//! Public market refresh and OHLCV recovery bodies with explicit maintained hosts.
pub use crate::ticker_readers::TickerValue as DataValue;
use std::{cell::RefCell, future::Future, rc::Rc};
pub type Topics = Rc<RefCell<Vec<String>>>;
pub type Callback<'a, E> = Rc<dyn Fn() -> Result<(), E> + 'a>;
#[derive(Clone, Debug)]
pub struct Market {
    pub symbol: String,
    pub quote: String,
}
pub type Markets = Rc<RefCell<Vec<Rc<RefCell<Market>>>>>;
#[derive(Clone, Debug)]
pub enum TopicSlot {
    One(String),
    Many(Topics),
}
#[derive(Debug)]
pub struct DiagnosticPayload {
    pub op: String,
    pub args: Topics,
}
pub type Payload = Rc<RefCell<DiagnosticPayload>>;
fn payload(op: &str, args: Topics) -> Payload {
    Rc::new(RefCell::new(DiagnosticPayload {
        op: op.into(),
        args,
    }))
}
fn truthy(v: &DataValue) -> bool {
    match v {
        DataValue::Undefined | DataValue::Null => false,
        DataValue::Bool(v) => *v,
        DataValue::Number(v) => *v != 0. && !v.is_nan(),
        DataValue::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn prop(v: &DataValue, key: &str) -> DataValue {
    match v {
        DataValue::Object(f) => f
            .borrow()
            .get(&key.encode_utf16().collect::<Vec<_>>())
            .cloned()
            .unwrap_or(DataValue::Undefined),
        _ => DataValue::Undefined,
    }
}
fn empty() -> DataValue {
    DataValue::Array(Rc::new(RefCell::new(vec![])))
}
fn matching(quote: &str, category: &str) -> bool {
    if category == "linear" {
        quote == "USDT" || quote == "USDC"
    } else {
        quote != "USDT" && quote != "USDC"
    }
}
fn filtered<E>(
    markets: &Markets,
    mut category: impl FnMut() -> Result<String, E>,
) -> Result<Vec<Rc<RefCell<Market>>>, E> {
    let length = markets.borrow().len();
    let mut selected = vec![];
    for i in 0..length {
        let market = markets.borrow().get(i).cloned();
        if let Some(market) = market {
            let category = category()?;
            if matching(&market.borrow().quote, &category) {
                selected.push(market);
            }
        }
    }
    Ok(selected)
}
pub fn on_open<E, R, H, Off, On, Set>(
    disposed: impl FnOnce() -> Result<bool, E>,
    markets: impl FnOnce() -> Result<Markets, E>,
    category: impl FnMut() -> Result<String, E>,
    ticker_setter: impl FnOnce() -> Result<Set, E>,
    subscribe: impl FnOnce() -> Result<R, E>,
    ping: impl FnOnce() -> Result<(), E>,
    arm_existing: impl FnOnce() -> Result<(), E>,
    mut handler: impl FnMut() -> Result<H, E>,
    off_op: impl FnOnce() -> Result<Off, E>,
    on_op: impl FnOnce() -> Result<On, E>,
    reconnecting: impl FnOnce() -> bool,
    emit: impl FnOnce() -> Result<(), E>,
) -> Result<(), E>
where
    Set: FnOnce(Topics) -> Result<(), E>,
    Off: FnOnce(&str, H) -> Result<(), E>,
    On: FnOnce(&str, H) -> Result<(), E>,
{
    if !disposed()? {
        let set_tickers = ticker_setter()?;
        let markets = markets()?;
        let selected = filtered(&markets, category)?;
        let topics = Rc::new(RefCell::new(
            selected
                .iter()
                .map(|m| format!("tickers.{}", m.borrow().symbol))
                .collect(),
        ));
        set_tickers(topics)?;
        let _started = subscribe()?;
        ping()?;
        arm_existing()?;
        let off = off_op()?;
        off("markets:changed", handler()?)?;
        let on = on_op()?;
        on("markets:changed", handler()?)?;
        if reconnecting() {
            emit()?;
        }
    }
    Ok(())
}
pub fn clear_ohlcv_no_data_timeout<E>(
    topic: &str,
    get: impl FnOnce(&str) -> Result<DataValue, E>,
    clear: impl FnOnce(DataValue) -> Result<(), E>,
    delete: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    let id = get(topic)?;
    if !truthy(&id) {
        return Ok(());
    }
    clear(id)?;
    delete(topic)
}
pub fn arm_ohlcv_no_data_timeout<'a, E: 'a, Put>(
    topic: String,
    secondary: Option<String>,
    clear: impl FnOnce(&str) -> Result<(), E>,
    handlers: Rc<dyn Fn(&str) -> Result<usize, E> + 'a>,
    set_timeout: impl FnOnce(Callback<'a, E>, f64) -> Result<DataValue, E>,
    setter: impl FnOnce() -> Result<Put, E>,
    delete: Rc<dyn Fn(&str) -> Result<(), E> + 'a>,
    disposed: Rc<dyn Fn() -> Result<bool, E> + 'a>,
    recover: Rc<dyn Fn(&str, Option<&str>, &str) -> Result<(), E> + 'a>,
    arm: Rc<dyn Fn(&str, Option<&str>) -> Result<(), E> + 'a>,
) -> Result<(), E>
where
    Put: FnOnce(&str, DataValue) -> Result<(), E>,
{
    clear(&topic)?;
    if handlers(&topic)? == 0 {
        return Ok(());
    }
    let put = setter()?;
    let captured = topic.clone();
    let callback: Callback<'a, E> = Rc::new(move || {
        delete(&captured)?;
        if disposed()? || handlers(&captured)? == 0 {
            return Ok(());
        }
        recover(
            &captured,
            secondary.as_deref(),
            "no OHLCV messages after 25000ms",
        )?;
        arm(&captured, secondary.as_deref())
    });
    let id = set_timeout(callback, 25000.)?;
    put(&topic, id)
}
pub fn arm_existing_ohlcv_no_data_timeouts<E>(
    keys: impl FnOnce() -> Result<Vec<String>, E>,
    mut handlers: impl FnMut(&str) -> Result<usize, E>,
    mut arm: impl FnMut(&str, &str) -> Result<(), E>,
) -> Result<(), E> {
    for topic in keys()? {
        if !topic.starts_with("kline.") || handlers(&topic)? == 0 {
            continue;
        }
        let symbol = topic.rsplit('.').next().unwrap_or("");
        arm(&topic, &format!("publicTrade.{symbol}"))?;
    }
    Ok(())
}
pub fn maybe_recover_ohlcv_stream<E, Recover>(
    interval: impl FnOnce() -> Result<String, E>,
    mut symbol: impl FnMut() -> Result<String, E>,
    reason: Option<&str>,
    recover_op: impl FnOnce() -> Result<Recover, E>,
) -> Result<(), E>
where
    Recover: FnOnce(&str, Option<&str>, &str) -> Result<(), E>,
{
    let interval = interval()?;
    let interval = crate::types::INTERVAL
        .get(&interval)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("undefined");
    let topic = format!("kline.{interval}.{}", symbol()?);
    let recover = recover_op()?;
    recover(
        &topic,
        Some(&format!("publicTrade.{}", symbol()?)),
        reason.unwrap_or("stale OHLCV stream"),
    )
}
pub fn maybe_recover_ohlcv_stream_from_topics<E>(
    topic: &str,
    secondary: Option<&str>,
    reason: &str,
    mut handlers: impl FnMut(&str) -> Result<usize, E>,
    now: impl FnOnce() -> Result<f64, E>,
    last: impl FnOnce(&str) -> Result<Option<f64>, E>,
    set_last: impl FnOnce(&str, f64) -> Result<(), E>,
    mut put_topic: impl FnMut(&str, &str) -> Result<(), E>,
    connected: impl FnOnce() -> Result<bool, E>,
    log: impl FnOnce(&str, &str) -> Result<(), E>,
    reconnect: impl FnOnce() -> Result<(), E>,
    mut send: impl FnMut(Payload) -> Result<(), E>,
) -> Result<(), E> {
    if handlers(topic)? == 0 {
        return Ok(());
    }
    let now = now()?;
    let previous = last(topic)?
        .filter(|v| *v != 0. && !v.is_nan())
        .unwrap_or(0.);
    if now - previous < 10000. {
        return Ok(());
    }
    set_last(topic, now)?;
    let mut active = vec![topic.to_owned()];
    if let Some(secondary) = secondary.filter(|s| !s.is_empty()) {
        if handlers(secondary)? != 0 {
            active.push(secondary.into());
        }
    }
    for active in &active {
        put_topic(active, active)?;
    }
    let args = Rc::new(RefCell::new(active));
    if !connected()? {
        log(
            &format!("[Bybit] Public WS OHLCV recovery reconnecting {topic} ({reason})."),
            "warning",
        )?;
        reconnect()?;
        return Ok(());
    }
    log(
        &format!("[Bybit] Public WS OHLCV recovery re-subscribing {topic} ({reason})."),
        "warning",
    )?;
    send(payload("unsubscribe", args.clone()))?;
    send(payload("subscribe", args))
}
fn subscribe_frame(args: &[String]) -> String {
    format!(
        r#"{{"op":"subscribe","args":{}}}"#,
        serde_json::to_string(args).unwrap()
    )
}
/// First send and first sleep factory run before the returned future is polled.
pub fn on_markets_changed<E, F: Future<Output = Result<(), E>>, Put>(
    added: Markets,
    ws: impl FnOnce() -> Result<bool, E>,
    disposed: impl FnOnce() -> Result<bool, E>,
    category: impl FnMut() -> Result<String, E>,
    tickers: impl FnOnce() -> Result<Option<TopicSlot>, E>,
    setter: impl FnOnce() -> Result<Put, E>,
    mut send: impl FnMut(&str) -> Result<(), E>,
    mut sleep: impl FnMut(u64) -> Result<F, E>,
) -> impl Future<Output = Result<(), E>>
where
    Put: FnOnce(Topics) -> Result<(), E>,
{
    let prepared = (|| {
        if added.borrow().is_empty() || !ws()? || disposed()? {
            return Ok(None);
        }
        let selected = filtered(&added, category)?;
        if selected.is_empty() {
            return Ok(None);
        }
        let new: Vec<String> = selected
            .iter()
            .map(|m| format!("tickers.{}", m.borrow().symbol))
            .collect();
        let put = setter()?;
        let mut all = match tickers()? {
            None => vec![],
            Some(TopicSlot::One(s)) => s.chars().map(|c| c.to_string()).collect(),
            Some(TopicSlot::Many(a)) => a.borrow().clone(),
        };
        all.extend(new.iter().cloned());
        put(Rc::new(RefCell::new(all)))?;
        let chunks: Vec<Vec<String>> = new.chunks(10).map(<[String]>::to_vec).collect();
        send(&subscribe_frame(&chunks[0]))?;
        let first = sleep(5)?;
        Ok(Some((chunks, first)))
    })();
    async move {
        if let Some((chunks, first)) = prepared? {
            first.await?;
            for args in chunks.into_iter().skip(1) {
                send(&subscribe_frame(&args))?;
                sleep(5)?.await?;
            }
        }
        Ok(())
    }
}
pub fn fetch_fees<E, F: Future<Output = Result<DataValue, E>>>(
    category: &str,
    get: impl FnOnce(&str, &str) -> Result<F, E>,
) -> impl Future<Output = Result<DataValue, E>> {
    let request = get(
        crate::types::ENDPOINTS["FEE_RATE"].as_str().unwrap(),
        category,
    );
    async move {
        match request {
            Ok(request) => match request.await {
                Ok(data) => {
                    let list = prop(&prop(&data, "result"), "list");
                    Ok(if truthy(&list) { list } else { empty() })
                }
                Err(_) => Ok(empty()),
            },
            Err(_) => Ok(empty()),
        }
    }
}
fn spread_into<E>(
    rows: &mut Vec<DataValue>,
    value: DataValue,
    type_error: &impl Fn(&str) -> E,
) -> Result<(), E> {
    match value {
        DataValue::Array(items) => rows.extend(items.borrow().iter().cloned()),
        DataValue::String(text) => rows.extend(crate::ticker_readers::string_items(&text)),
        _ => {
            return Err(type_error(
                "Spread syntax requires ...iterable[Symbol.iterator] to be a function",
            ));
        }
    }
    Ok(())
}
/// Linear markets; inverse markets pushed onto that same array when enabled; option
/// markets appended into a fresh array when enabled. A branded request failure with
/// cached markets resolves to the cache.
#[allow(clippy::too_many_arguments)]
pub fn fetch_markets<
    E,
    F: Future<Output = Result<DataValue, E>>,
    O: Future<Output = Result<DataValue, E>>,
>(
    mut helper: impl FnMut(&str) -> Result<F, E>,
    inverse_enabled: impl FnOnce() -> Result<bool, E>,
    options_enabled: impl FnOnce() -> Result<bool, E>,
    option_markets: impl FnOnce() -> Result<O, E>,
    is_request_error: impl FnOnce(&E) -> bool,
    mut stored: impl FnMut() -> Result<DataValue, E>,
    log: impl FnOnce(&str, &str) -> Result<(), E>,
    type_error: impl Fn(&str) -> E,
) -> impl Future<Output = Result<DataValue, E>> {
    let mut is_request_error = Some(is_request_error);
    let mut log = Some(log);
    let mut recover = move |error| {
        if is_request_error.take().unwrap()(&error)
            && matches!(stored()?,DataValue::Array(rows) if !rows.borrow().is_empty())
        {
            log.take().unwrap()(
                "Failed to refresh Bybit markets, using cached markets",
                "warning",
            )?;
            return stored();
        }
        Err(error)
    };
    let (request, prepared) = match helper("linear") {
        Ok(f) => (Some(f), None),
        Err(e) => (None, Some(recover(e))),
    };
    async move {
        let Some(request) = request else {
            return prepared.unwrap();
        };
        let refreshed = async {
            let markets = request.await?;
            if inverse_enabled()? {
                if matches!(markets, DataValue::Null | DataValue::Undefined) {
                    return Err(type_error(&format!(
                        "Cannot read properties of {} (reading 'push')",
                        if matches!(markets, DataValue::Null) {
                            "null"
                        } else {
                            "undefined"
                        }
                    )));
                }
                let inverse = helper("inverse")?.await?;
                let DataValue::Array(rows) = &markets else {
                    return Err(type_error("markets.push is not a function"));
                };
                let mut pushed = vec![];
                spread_into(&mut pushed, inverse, &type_error)?;
                rows.borrow_mut().extend(pushed);
            }
            if !options_enabled()? {
                return Ok(markets);
            }
            let mut all = vec![];
            spread_into(&mut all, markets, &type_error)?;
            spread_into(&mut all, option_markets()?.await?, &type_error)?;
            Ok(DataValue::Array(Rc::new(RefCell::new(all))))
        };
        match refreshed.await {
            Ok(v) => Ok(v),
            Err(e) => recover(e),
        }
    }
}

/// A failure inside fetchOptionMarkets: a host rejection, or an Error the body raised.
enum OptionFailure<E> {
    Host(E),
    Raised(String),
}
impl<E> From<E> for OptionFailure<E> {
    fn from(error: E) -> Self {
        Self::Host(error)
    }
}
fn to_number(value: &DataValue) -> f64 {
    match value {
        DataValue::Undefined => f64::NAN,
        DataValue::Null => 0.,
        DataValue::Bool(v) => f64::from(u8::from(*v)),
        DataValue::Number(v) => *v,
        DataValue::String(v) => safe_cex_num::js_retained_number(Some(
            &safe_cex_types::retained_json::RetainedJsonInput::Text(v.clone()),
        ))
        .expect("String Number coercion"),
        DataValue::Array(items) => match items.borrow().as_slice() {
            [] => 0.,
            [DataValue::Null | DataValue::Undefined] => 0.,
            [DataValue::String(v)] => safe_cex_num::js_retained_number(Some(
                &safe_cex_types::retained_json::RetainedJsonInput::Text(v.clone()),
            ))
            .expect("String Number coercion"),
            [DataValue::Number(v)] => *v,
            _ => f64::NAN,
        },
        DataValue::Object(_) => f64::NAN,
    }
}
// get-key v(obj, key) over a response object, with its snake_case alias.
fn response_key<E>(
    data: &DataValue,
    key: &str,
    snake: &str,
) -> Result<DataValue, OptionFailure<E>> {
    match data {
        DataValue::Null | DataValue::Undefined | DataValue::Array(_) => Ok(DataValue::Undefined),
        DataValue::Object(fields) => {
            let fields = fields.borrow();
            Ok(fields
                .get(&key.encode_utf16().collect::<Vec<_>>())
                .or_else(|| fields.get(&snake.encode_utf16().collect::<Vec<_>>()))
                .cloned()
                .unwrap_or(DataValue::Undefined))
        }
        primitive => Err(OptionFailure::Raised(format!(
            "Cannot use 'in' operator to search for '{key}' in {}",
            primitive_text(primitive)
        ))),
    }
}
fn primitive_text(value: &DataValue) -> String {
    match value {
        DataValue::Undefined => "undefined".into(),
        DataValue::Null => "null".into(),
        DataValue::Bool(v) => v.to_string(),
        DataValue::Number(v) => safe_cex_num::js_number_string(*v),
        DataValue::String(v) => String::from_utf16_lossy(v),
        DataValue::Array(_) | DataValue::Object(_) => "[object Object]".into(),
    }
}
fn ensure_ok<E>(data: &DataValue) -> Result<(), OptionFailure<E>> {
    if to_number(&response_key(data, "retCode", "ret_code")?) == 0. {
        return Ok(());
    }
    // new Error(undefined) has an empty message; anything else is String(value).
    Err(OptionFailure::Raised(
        match response_key(data, "retMsg", "ret_msg")? {
            DataValue::Undefined => String::new(),
            message => primitive_text(&message),
        },
    ))
}
// `data.result?.list ?? []` and `data.result?.nextPageCursor ?? ''`.
fn result_field<E>(data: &DataValue, key: &str) -> Result<DataValue, OptionFailure<E>> {
    let result = prop(data, "result");
    Ok(match prop(&result, key) {
        DataValue::Null | DataValue::Undefined => DataValue::Undefined,
        value => value,
    })
}
async fn option_markets_body<E, G: Future<Output = Result<DataValue, E>>>(
    coins: G,
    get: &mut impl FnMut(&str, Option<(DataValue, DataValue)>) -> Result<G, E>,
    map_instrument: &mut impl FnMut(DataValue) -> Result<DataValue, E>,
) -> Result<DataValue, OptionFailure<E>> {
    let endpoints = &crate::types::ENDPOINTS;
    let coins = coins.await?;
    ensure_ok(&coins)?;
    let list = match result_field(&coins, "list")? {
        DataValue::Undefined => vec![],
        DataValue::Array(items) => items.borrow().clone(),
        _ => {
            return Err(OptionFailure::Raised(
                "(coins.result?.list ?? []).map is not a function".into(),
            ));
        }
    };
    let mut base_coins = vec![];
    for coin in list {
        match coin {
            DataValue::Null | DataValue::Undefined => {
                return Err(OptionFailure::Raised(format!(
                    "Cannot read properties of {} (reading 'baseCoin')",
                    primitive_text(&coin)
                )));
            }
            coin => base_coins.push(prop(&coin, "baseCoin")),
        }
    }
    let mut markets = vec![];
    for base_coin in base_coins {
        let mut rows = vec![];
        let mut cursor = DataValue::String(Vec::new());
        loop {
            let data = get(
                endpoints["MARKETS"].as_str().unwrap(),
                Some((base_coin.clone(), cursor)),
            )?
            .await?;
            ensure_ok(&data)?;
            match result_field(&data, "list")? {
                DataValue::Undefined => {}
                DataValue::Array(items) => rows.extend(items.borrow().iter().cloned()),
                DataValue::String(text) => rows.extend(crate::ticker_readers::string_items(&text)),
                _ => {
                    return Err(OptionFailure::Raised(
                        "Spread syntax requires ...iterable[Symbol.iterator] to be a function"
                            .into(),
                    ));
                }
            }
            cursor = match result_field(&data, "nextPageCursor")? {
                DataValue::Undefined => DataValue::String(Vec::new()),
                next => next,
            };
            if !truthy(&cursor) {
                break;
            }
        }
        for row in rows {
            match map_instrument(row)? {
                DataValue::Null | DataValue::Undefined => {}
                DataValue::Array(items) => markets.extend(items.borrow().iter().cloned()),
                market => markets.push(market),
            }
        }
    }
    Ok(DataValue::Array(Rc::new(RefCell::new(markets))))
}
// ts: BybitExchange.fetchOptionMarkets — best-effort: any failure logs and keeps the
// option markets already stored, so positions keep their market.
pub fn fetch_option_markets<'a, E: 'a, G: Future<Output = Result<DataValue, E>> + 'a>(
    mut get: impl FnMut(&str, Option<(DataValue, DataValue)>) -> Result<G, E> + 'a,
    mut map_instrument: impl FnMut(DataValue) -> Result<DataValue, E> + 'a,
    describe: impl FnOnce(&E) -> String + 'a,
    log: impl FnOnce(&str, &str) -> Result<(), E> + 'a,
    stored: impl FnOnce() -> Result<DataValue, E> + 'a,
    type_error: impl FnOnce(&str) -> E + 'a,
) -> impl Future<Output = Result<DataValue, E>> + 'a {
    let keep_stored = move |failure: OptionFailure<E>| {
        let failure = match failure {
            OptionFailure::Host(error) => describe(&error),
            OptionFailure::Raised(message) => message,
        };
        log(
            &format!("Failed to load Bybit option markets, keeping the last ones: {failure}"),
            "warning",
        )?;
        let DataValue::Array(markets) = stored()? else {
            return Err(type_error("this.store.markets.filter is not a function"));
        };
        let kept = markets
            .borrow()
            .iter()
            .filter(|m| truthy(&prop(m, "option")))
            .cloned()
            .collect();
        Ok(DataValue::Array(Rc::new(RefCell::new(kept))))
    };
    // The base-coin request starts before the first await, as in the source.
    let first = get(
        crate::types::ENDPOINTS["OPTION_BASE_COINS"]
            .as_str()
            .unwrap(),
        None,
    );
    let mut keep_stored = Some(keep_stored);
    let (coins, prepared) = match first {
        Ok(coins) => (Some(coins), None),
        Err(error) => (
            None,
            Some(keep_stored.take().unwrap()(OptionFailure::Host(error))),
        ),
    };
    async move {
        let Some(coins) = coins else {
            return prepared.unwrap();
        };
        match option_markets_body(coins, &mut get, &mut map_instrument).await {
            Ok(markets) => Ok(markets),
            Err(failure) => keep_stored.take().unwrap()(failure),
        }
    }
}

pub fn fetch_trading_markets_response<
    E,
    F: Future<Output = Result<DataValue, E>>,
    S: Future<Output = Result<(), E>>,
>(
    category: &str,
    mut get: impl FnMut(&str, &str, u64) -> Result<F, E>,
    retryable: impl FnOnce(&E) -> Result<bool, E>,
    describe: impl FnOnce(&E) -> Result<String, E>,
    log: impl FnOnce(&str, &str) -> Result<(), E>,
    sleep: impl FnOnce(u64) -> Result<S, E>,
    wrap: impl Fn(E, Option<u32>) -> E,
) -> impl Future<Output = Result<DataValue, E>> {
    let category = category.to_owned();
    let endpoint = crate::types::ENDPOINTS["MARKETS"].as_str().unwrap();
    let wrap = Rc::new(wrap);
    let retry_wrap = wrap.clone();
    let mut retryable = Some(retryable);
    let mut describe = Some(describe);
    let mut log = Some(log);
    let mut sleep = Some(sleep);
    let mut prepare_retry = move |error| {
        if !retryable.take().unwrap()(&error)? {
            return Err(retry_wrap(error, None));
        }
        let reason = describe.take().unwrap()(&error)?;
        log.take().unwrap()(
            &format!("Failed to fetch Bybit Trading markets ({reason}), retrying once in 1000ms"),
            "warning",
        )?;
        sleep.take().unwrap()(1000)
    };
    // A synchronous request throw enters the source catch before its first await.
    let (request, prepared_retry) = match get(endpoint, &category, 1000) {
        Ok(f) => (Some(f), None),
        Err(error) => (None, Some(prepare_retry(error))),
    };
    async move {
        let retry_sleep = if let Some(request) = request {
            match request.await {
                Ok(value) => return Ok(value),
                Err(error) => prepare_retry(error)?,
            }
        } else {
            prepared_retry.unwrap()?
        };
        retry_sleep.await?;
        let retried = match get(endpoint, &category, 1000) {
            Ok(f) => f.await,
            Err(e) => Err(e),
        };
        retried.map_err(|error| wrap(error, Some(1)))
    }
}
