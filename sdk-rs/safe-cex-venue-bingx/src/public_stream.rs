//! Public-stream bodies over imported socket/Blob capabilities and the shared Store.
use crate::data_reads::BingxDataReads;
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough};
use safe_cex_num::{
    JsJsonValue as J, js_json_stringify, js_number, js_string, js_truthy, parse_float,
};
use safe_cex_store::model_patch::ModelPatch;
use serde::Serialize;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

/// The source Number intrinsic is imported; its value/error identity stays with the caller.
pub fn parse_positive_finite_number<E>(
    coerce: impl FnOnce() -> Result<f64, E>,
) -> Result<Option<f64>, E> {
    let number = coerce()?;
    Ok(if number.is_finite() && number > 0.0 {
        Some(number)
    } else {
        None
    })
}
pub type MessageCallback = Rc<dyn Fn(&[Value]) -> Result<(), ExchangeError>>;
pub type HandlerList = Rc<RefCell<Vec<MessageCallback>>>;
pub enum PublicFrame {
    Text(String),
    Blob(u64),
}
pub enum SocketListener {
    Open(Rc<dyn Fn() -> Result<(), ExchangeError>>),
    Message(Rc<dyn Fn(PublicFrame) -> ExchangeFuture<'static, ()>>),
    Close,
}
/// Imported BaseWebSocket and host intrinsics; no pending venue body is delegated.
pub trait PublicSocketHost {
    fn is_disposed(&self) -> bool;
    fn nanoid(&self) -> String;
    fn monotonic_now_ms(&self) -> f64;
    fn iso_time(&self) -> Result<String, ExchangeError>;
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError>;
    fn add_listener(&self, listener: SocketListener) -> Result<(), ExchangeError>;
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError>;
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError>;
    fn track_last_received(&self) -> Result<(), ExchangeError>;
    fn handle_pong(&self) -> Result<(), ExchangeError>;
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError>;
    fn diagnostic_receive(
        &self,
        raw: &str,
        allow: &dyn Fn() -> bool,
        format: &dyn Fn() -> Result<Option<String>, ExchangeError>,
    ) -> Result<(), ExchangeError>;
    fn read_blob(&self, blob: u64) -> ExchangeFuture<'_, String>;
    fn report_exception(&self, error: &ExchangeError);
}
pub struct BingxPublicStream {
    pub parent: Rc<BingxDataReads>,
    pub host: Rc<dyn PublicSocketHost>,
    pub message_handlers: RefCell<Vec<(String, HandlerList)>>,
    pub topics: RefCell<Vec<(String, f64)>>,
}
#[derive(Serialize)]
#[serde(transparent)]
struct Parsed {
    value: Value,
}
impl JsNumberOutput for Parsed {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct {} passthrough {value});
    }
}
fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
fn text(value: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(value).map_err(|e| type_error(e.message))
}
fn alias<'a>(value: &'a Value, key: &str) -> Result<Option<&'a Value>, ExchangeError> {
    use safe_cex_num::get_key::GetKeyUnknownValue;
    match safe_cex_num::get_key::v_unknown(Some(value), key)
        .map_err(|e| type_error(e.to_string()))?
    {
        GetKeyUnknownValue::Json(value) => Ok(Some(value)),
        GetKeyUnknownValue::Undefined => Ok(None),
        _ => Err(type_error(
            "BingX stream field refuses an inherited callable",
        )),
    }
}
fn float(value: Option<&Value>) -> Result<f64, ExchangeError> {
    Ok(parse_float(&text(value)?))
}
fn stringify(value: J) -> Result<String, ExchangeError> {
    js_json_stringify(&value)
        .map_err(|e| type_error(e.message))?
        .ok_or_else(|| type_error("stream payload is undefined"))
}
fn spread(value: Option<&Value>) -> Vec<(String, J)> {
    match value {
        Some(Value::Object(fields)) => fields
            .iter()
            .map(|(k, v)| (k.clone(), J::from(v.clone())))
            .collect(),
        Some(Value::Array(rows)) => rows
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), J::from(v.clone())))
            .collect(),
        Some(Value::String(text)) => text
            .chars()
            .enumerate()
            .map(|(i, c)| (i.to_string(), J::String(c.to_string())))
            .collect(),
        _ => Vec::new(),
    }
}
fn parse(raw: &str) -> Result<Value, ExchangeError> {
    match serde_json::from_str::<JsNumberInput>(raw) {
        Ok(input) => input
            .decode_with(|decoder| {
                js_deserialize_passthrough(decoder).map(|value| Parsed { value })
            })
            .map(|v| v.value)
            .map_err(|e| ExchangeError::new(e.to_string())),
        Err(_) => {
            if serde_json::from_str::<Box<serde_json::value::RawValue>>(raw).is_ok() {
                Err(type_error(
                    "BingX stream UTF-8 boundary refuses a lone surrogate",
                ))
            } else {
                Ok(Value::Null)
            }
        }
    }
}
impl BingxPublicStream {
    pub fn new(parent: Rc<BingxDataReads>, host: Rc<dyn PublicSocketHost>) -> Rc<Self> {
        Rc::new(Self {
            parent,
            host,
            message_handlers: RefCell::new(Vec::new()),
            topics: RefCell::new(Vec::new()),
        })
    }
    pub fn get_diagnostic_symbols(&self) -> Vec<String> {
        let handlers = self.message_handlers.borrow();
        let keys = safe_cex_num::js_own_keys(handlers.iter().map(|(key, _)| key.as_str()));
        let mut symbols = Vec::new();
        for topic in keys {
            let suffix = topic
                .strip_prefix("market.kline.")
                .or_else(|| topic.strip_prefix("market.depth."));
            if let Some((symbol, _)) = suffix.and_then(|s| s.split_once('.'))
                && !symbol.is_empty()
                && !symbols.iter().any(|s| s == symbol)
            {
                symbols.push(symbol.to_owned());
            }
        }
        symbols
    }
    pub fn format_market_contracts_diagnostic(
        &self,
        json: &Value,
        symbols: &[String],
    ) -> Result<String, ExchangeError> {
        let data = json.get("data");
        let contracts = data
            .and_then(|d| d.get("contracts"))
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |rows| {
                rows.iter()
                    .filter(|contract| {
                        contract
                            .get("symbol")
                            .and_then(Value::as_str)
                            .is_some_and(|symbol| symbols.iter().any(|s| s == symbol))
                    })
                    .cloned()
                    .map(J::from)
                    .collect()
            });
        let mut fields = spread(Some(json));
        let mut data = spread(data);
        data.push(("contracts".into(), J::Array(contracts)));
        data.push((
            "filteredForSymbols".into(),
            J::Array(symbols.iter().cloned().map(J::String).collect()),
        ));
        fields.push(("data".into(), J::Object(data)));
        Ok(format!("[bingx public] {}", stringify(J::Object(fields))?))
    }
    pub fn connect_and_subscribe(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        let environment = usize::from(self.parent.base.options().testnet == Some(true));
        self.host
            .create_socket(crate::constants::BASE_WS_URL[0].1[environment].1)?;
        let weak = Rc::downgrade(self);
        self.host
            .add_listener(SocketListener::Open(Rc::new(move || {
                weak.upgrade()
                    .ok_or_else(|| ExchangeError::new("BingX public stream owner released"))?
                    .on_open()
            })))?;
        let weak = Rc::downgrade(self);
        self.host
            .add_listener(SocketListener::Message(Rc::new(move |frame| {
                let owner = weak.upgrade();
                Box::pin(async move {
                    owner
                        .ok_or_else(|| ExchangeError::new("BingX public stream owner released"))?
                        .on_message(frame)
                        .await
                })
            })))?;
        self.host.add_listener(SocketListener::Close)
    }
    pub fn on_open(&self) -> Result<(), ExchangeError> {
        if !self.host.is_disposed() {
            let frame = stringify(J::Object(vec![
                ("id".into(), J::String(self.host.nanoid())),
                ("reqType".into(), J::String("sub".into())),
                ("dataType".into(), J::String("market.contracts".into())),
            ]))?;
            self.host.send_with_log(&frame)?;
            self.subscribe()?;
            self.ping()?;
        }
        Ok(())
    }
    pub fn ping(&self) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        let result = (|| {
            self.host.set_ping_at(self.host.monotonic_now_ms())?;
            self.host.send_with_log("Ping")?;
            let id = self.host.nanoid();
            let time = self.host.iso_time()?;
            self.host
                .send_with_log(&format!("{{\"ping\":\"{id}\",\"time\":\"{time}\"}}"))
        })();
        if let Err(error) = result {
            self.host.report_exception(&error);
        }
        Ok(())
    }
    pub fn subscribe(&self) -> Result<(), ExchangeError> {
        let keys = {
            let topics = self.topics.borrow();
            safe_cex_num::js_own_keys(topics.iter().map(|(key, _)| key.as_str()))
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        for topic in keys {
            self.host.send_with_log(&stringify(J::Object(vec![
                ("reqType".into(), J::String("sub".into())),
                ("dataType".into(), J::String(topic)),
                ("id".into(), J::String(self.host.nanoid())),
            ]))?)?;
        }
        Ok(())
    }
    pub async fn on_message(&self, frame: PublicFrame) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        self.host.track_last_received()?;
        let raw = match frame {
            PublicFrame::Text(text) => text,
            PublicFrame::Blob(blob) => {
                let text = self.host.read_blob(blob).await?;
                if self.host.is_disposed() {
                    return Ok(());
                }
                text
            }
        };
        let json = parse(&raw)?;
        let topic = json.get("dataType").and_then(Value::as_str);
        let symbols = if topic == Some("market.contracts") {
            Some(self.get_diagnostic_symbols())
        } else {
            None
        };
        self.host.diagnostic_receive(
            &raw,
            &|| {
                topic == Some("market.contracts") && symbols.as_ref().is_some_and(|s| !s.is_empty())
            },
            &|| {
                if let Some(symbols) = &symbols {
                    self.format_market_contracts_diagnostic(&json, symbols)
                        .map(Some)
                } else {
                    Ok(None)
                }
            },
        )?;
        if raw == "Ping" || js_truthy(json.get("ping")) {
            return self.host.send_with_log("Pong");
        }
        if raw == "Pong" || js_truthy(json.get("pong")) {
            return self.host.handle_pong();
        }
        if topic == Some("market.contracts") {
            self.host.record_stats("ticker")?;
            let contracts = json.get("data").and_then(|d| d.get("contracts"));
            let rows = if !js_truthy(contracts) {
                &[][..]
            } else {
                contracts
                    .and_then(Value::as_array)
                    .ok_or_else(|| type_error("contracts is not iterable"))?
            };
            for contract in rows {
                let key = text(contract.get("symbol"))?;
                let ticker = self.parent.base.store.tickers_by_id()?.get(&key).cloned();
                let inherited = ticker.is_none()
                    && matches!(
                        safe_cex_num::js_table_lookup::<&str, ()>(&[], &key),
                        safe_cex_num::TableHit::Inherited(_)
                    );
                if ticker.is_none() && !inherited {
                    continue;
                }
                let last = parse_positive_finite_number(|| {
                    js_number(alias(contract, "tradePrice")?).map_err(|e| type_error(e.message))
                })?;
                let market = match ticker.as_ref() {
                    Some(ticker) => self
                        .parent
                        .base
                        .store
                        .markets_by_symbol()?
                        .get(&ticker.symbol)
                        .cloned(),
                    None => None,
                };
                let touches = last.zip(market.as_ref()).map(|(last, market)| {
                    crate::ticker::derive_ticker_bid_ask(Some(&market.precision), last)
                });
                let mut patch = ModelPatch::default();
                for (field, key) in [
                    ("percentage", "changePercentage"),
                    ("volume", "volumeF"),
                    ("quoteVolume", "valueF"),
                    ("mark", "fairPrice"),
                    ("index", "indexPrice"),
                ] {
                    patch.set_number(field, float(alias(contract, key)?)?);
                }
                patch.set_number(
                    "fundingRate",
                    float(alias(contract, "fundingRate")?)? / 100.0,
                );
                patch.set_number("fundingTime", 0.0);
                let Some(ticker) = ticker else {
                    return Err(type_error(
                        "BingX typed Ticker refuses an inherited callable target",
                    ));
                };
                patch.set_number("last", last.unwrap_or(ticker.last));
                patch.set_number("bid", touches.map_or(ticker.bid, |v| v.0));
                patch.set_number("ask", touches.map_or(ticker.ask, |v| v.1));
                self.parent
                    .base
                    .store
                    .update_ticker(&ticker.id, &patch)
                    .map_err(ExchangeError::from)?;
            }
        } else {
            let key = text(json.get("dataType"))?;
            let callbacks = self
                .message_handlers
                .borrow()
                .iter()
                .find(|(topic, _)| topic == &key)
                .map(|(_, handlers)| handlers.clone());
            let inherited = callbacks.is_none()
                && matches!(safe_cex_num::js_table_lookup::<&str,()>(&[],&key),safe_cex_num::TableHit::Inherited(member) if member.length().unwrap_or(0)>0);
            if let Some(callbacks) = callbacks
                && !callbacks.borrow().is_empty()
            {
                self.host.record_stats(&key)?;
                let payload = if let Value::Array(rows) = json {
                    rows
                } else {
                    vec![json]
                };
                let mut index = 0;
                loop {
                    let callback = callbacks.borrow().get(index).cloned();
                    let Some(callback) = callback else {
                        break;
                    };
                    callback(&payload)?;
                    index += 1;
                }
            } else if inherited {
                self.host.record_stats(&key)?;
                return Err(type_error("callbacks is not iterable"));
            }
        }
        Ok(())
    }
}
