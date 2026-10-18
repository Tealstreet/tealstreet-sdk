//! Ticker-reader bodies. Host factories start eagerly; completion is awaited.
//! Values retain absent/null distinctions and array/object allocation identity.
use std::{cell::RefCell, collections::BTreeMap, future::Future, rc::Rc};

#[derive(Clone, Debug)]
pub enum TickerValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Rc<RefCell<Vec<TickerValue>>>),
    Object(Rc<RefCell<BTreeMap<Vec<u16>, TickerValue>>>),
}
pub(crate) fn string_items(text: &[u16]) -> Vec<TickerValue> {
    safe_cex_exchange::js_runtime::value::Js::utf16(text)
        .iterable_values()
        .expect("String spread")
        .into_iter()
        .map(|value| {
            TickerValue::String(value.to_js_string().expect("String iterator item").to_vec())
        })
        .collect()
}
impl TickerValue {
    fn property(&self, key: &str) -> Self {
        match self {
            Self::Object(fields) => fields
                .borrow()
                .get(&key.encode_utf16().collect::<Vec<_>>())
                .cloned()
                .unwrap_or(Self::Undefined),
            _ => Self::Undefined,
        }
    }
    fn type_name(&self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "null",
            _ => "value",
        }
    }
    fn primitive_text(&self) -> String {
        match self {
            Self::Undefined => "undefined".into(),
            Self::Null => "null".into(),
            Self::Bool(v) => v.to_string(),
            Self::Number(v) => safe_cex_num::js_number_string(*v),
            Self::String(v) => String::from_utf16_lossy(v),
            _ => "[object Object]".into(),
        }
    }
    fn has_null<E>(&self, type_error: &impl Fn(&str) -> E) -> Result<bool, E> {
        match self {
            Self::Array(rows) => Ok(rows.borrow().iter().any(|row| matches!(row, Self::Null))),
            Self::Undefined | Self::Null => Err(type_error(&format!(
                "Cannot read properties of {} (reading 'some')",
                self.type_name()
            ))),
            _ => Err(type_error("tickers.some is not a function")),
        }
    }
}

/// Both wrappers call fetchRaw immediately, then read disposal only after resolution.
pub fn fetch_tickers_without_parse<E, F: Future<Output = Result<TickerValue, E>>>(
    fetch_raw: impl FnOnce() -> Result<F, E>,
    is_disposed: impl FnOnce() -> Result<bool, E>,
) -> impl Future<Output = Result<TickerValue, E>> {
    let request = fetch_raw();
    async move {
        let raw = request?.await?;
        if is_disposed()? {
            Ok(TickerValue::Array(Rc::new(RefCell::new(vec![]))))
        } else {
            Ok(raw)
        }
    }
}

pub fn fetch_tickers<E, F: Future<Output = Result<TickerValue, E>>>(
    fetch_raw: impl FnOnce() -> Result<F, E>,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    stored_tickers: impl FnOnce() -> Result<TickerValue, E>,
    parse: impl FnOnce(TickerValue) -> Result<TickerValue, E>,
) -> impl Future<Output = Result<TickerValue, E>> {
    let request = fetch_raw();
    async move {
        let raw = request?.await?;
        if is_disposed()? {
            stored_tickers()
        } else {
            parse(raw)
        }
    }
}

// Array spread of an awaited value: arrays copy rows, strings split into code points.
fn spread_into<E>(
    tickers: &mut Vec<TickerValue>,
    set: TickerValue,
    not_iterable: &str,
    type_error: &impl Fn(&str) -> E,
) -> Result<(), E> {
    match set {
        TickerValue::Array(rows) => tickers.extend(rows.borrow().iter().cloned()),
        TickerValue::String(text) => tickers.extend(string_items(&text)),
        _ => return Err(type_error(not_iterable)),
    }
    Ok(())
}

/// Linear rows, then inverse rows when enabled; the inverse request starts only
/// after the linear one resolves and spreads, and the result is always a fresh array.
pub fn fetch_tickers_raw<E, F: Future<Output = Result<TickerValue, E>>>(
    mut helper: impl FnMut(&str) -> Result<F, E>,
    inverse_enabled: impl FnOnce() -> Result<bool, E>,
    emit: impl FnOnce(TickerValue) -> Result<(), E>,
    type_error: impl Fn(&str) -> E,
) -> impl Future<Output = Result<TickerValue, E>> {
    let request = helper("linear");
    async move {
        let mut tickers = Vec::new();
        let linear = request?.await?;
        spread_into(
            &mut tickers,
            linear,
            "(intermediate value) is not iterable",
            &type_error,
        )?;
        if inverse_enabled()? {
            let inverse = helper("inverse")?.await?;
            spread_into(
                &mut tickers,
                inverse,
                "(intermediate value)(intermediate value)(intermediate value) is not iterable",
                &type_error,
            )?;
        }
        if tickers.iter().any(|row| matches!(row, TickerValue::Null)) {
            emit(TickerValue::String(
                "Failed to fetch tickers".encode_utf16().collect(),
            ))?;
            Ok(TickerValue::Null)
        } else {
            Ok(TickerValue::Array(Rc::new(RefCell::new(tickers))))
        }
    }
}

/// `all` is the host's Promise.all equivalent: requests already started, results
/// ordered by input, first rejection propagated, sibling requests remain owned by host.
pub fn fetch_inverse_tickers_raw<
    E,
    Task,
    Helper,
    All: Future<Output = Result<Vec<TickerValue>, E>>,
>(
    categories: impl FnOnce() -> Result<Rc<Vec<String>>, E>,
    bind_helper: impl FnOnce() -> Result<Helper, E>,
    all: impl FnOnce(Vec<Task>) -> Result<All, E>,
    emit: impl FnOnce(TickerValue) -> Result<(), E>,
    type_error: impl Fn(&str) -> E,
) -> impl Future<Output = Result<TickerValue, E>>
where
    Helper: FnMut(&str, usize, &Rc<Vec<String>>) -> Result<Task, E>,
{
    let request = (|| {
        let categories = categories()?;
        let mut helper = bind_helper()?;
        let requests = categories
            .iter()
            .enumerate()
            .map(|(i, c)| helper(c, i, &categories))
            .collect::<Result<Vec<_>, _>>()?;
        all(requests)
    })();
    async move {
        let found = request?.await?;
        let mut tickers = Vec::new();
        for set in found {
            match set {
                TickerValue::Array(rows) => tickers.extend(rows.borrow().iter().cloned()),
                TickerValue::String(text) => tickers.extend(string_items(&text)),
                TickerValue::Null => {
                    return Err(type_error(
                        "tickerSet is not iterable (cannot read property null)",
                    ));
                }
                TickerValue::Undefined => {
                    return Err(type_error(
                        "tickerSet is not iterable (cannot read property undefined)",
                    ));
                }
                _ => {
                    return Err(type_error(
                        "Spread syntax requires ...iterable[Symbol.iterator] to be a function",
                    ));
                }
            }
        }
        let tickers = TickerValue::Array(Rc::new(RefCell::new(tickers)));
        if tickers.has_null(&type_error)? {
            emit(TickerValue::String(
                "Failed to fetch tickers".encode_utf16().collect(),
            ))?;
            Ok(TickerValue::Null)
        } else {
            Ok(tickers)
        }
    }
}

pub fn fetch_tickers_raw_helper<E, F: Future<Output = Result<TickerValue, E>>>(
    category: &str,
    get: impl FnOnce(&str, &str) -> Result<F, E>,
    emit: impl FnOnce(TickerValue) -> Result<(), E>,
    type_error: impl Fn(&str) -> E,
) -> impl Future<Output = Result<TickerValue, E>> {
    let request = get(
        crate::types::ENDPOINTS["TICKERS"].as_str().unwrap(),
        category,
    );
    async move {
        let data = request?.await?;
        let message = match &data {
            TickerValue::Object(fields) => {
                let fields = fields.borrow();
                fields
                    .get(&"retMsg".encode_utf16().collect::<Vec<_>>())
                    .or_else(|| fields.get(&"ret_msg".encode_utf16().collect::<Vec<_>>()))
                    .cloned()
                    .unwrap_or(TickerValue::Undefined)
            }
            TickerValue::Null | TickerValue::Undefined | TickerValue::Array(_) => {
                TickerValue::Undefined
            }
            primitive => {
                return Err(type_error(&format!(
                    "Cannot use 'in' operator to search for 'retMsg' in {}",
                    primitive.primitive_text()
                )));
            }
        };
        if !matches!(&message,TickerValue::String(m) if m.iter().copied().eq("OK".encode_utf16())) {
            emit(message)?;
            return Ok(TickerValue::Array(Rc::new(RefCell::new(vec![
                TickerValue::Null,
            ]))));
        }
        let result = data.property("result");
        if matches!(result, TickerValue::Null | TickerValue::Undefined) {
            return Err(type_error(&format!(
                "Cannot read properties of {} (reading 'list')",
                result.type_name()
            )));
        }
        Ok(result.property("list"))
    }
}

pub fn fetch_spot_tickers_raw<E, F: Future<Output = Result<TickerValue, E>>>(
    get: impl FnOnce(&str, &str) -> Result<F, E>,
    emit: impl FnOnce(TickerValue) -> Result<(), E>,
    type_error: impl Fn(&str) -> E,
) -> impl Future<Output = Result<TickerValue, E>> {
    fetch_tickers_raw_helper("spot", get, emit, type_error)
}
