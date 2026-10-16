//! Prepared subscription bodies; the host binds JSON.stringify/send and handler maps.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_table_lookup};

pub type Topic = Vec<u16>;

#[derive(Debug, Clone, PartialEq)]
pub enum PublicSubscription {
    Futures {
        event: &'static str,
        feed: &'static str,
        product_ids: Vec<Topic>,
    },
    SpotTicker {
        method: &'static str,
        channel: &'static str,
        symbols: Vec<Topic>,
        event_trigger: &'static str,
    },
    SpotBook {
        method: &'static str,
        channel: &'static str,
        symbols: Vec<Topic>,
        depth: f64,
    },
    SpotOhlc {
        method: &'static str,
        channel: &'static str,
        symbols: Vec<Topic>,
        interval: TableHit<f64>,
    },
}

pub trait PublicSubscriptionHost {
    fn is_disposed(&self) -> bool;
    fn ticker_product_ids(&self) -> Result<Vec<Topic>, ExchangeError>;
    fn market_symbols(&self) -> Result<Vec<Topic>, ExchangeError>;
    fn book_keys(&self) -> Result<Vec<Topic>, ExchangeError>;
    fn book_length(&self, key: &[u16]) -> Result<Option<usize>, ExchangeError>;
    fn ohlcv_keys(&self) -> Result<Vec<Topic>, ExchangeError>;
    fn ohlcv_length(&self, key: &[u16]) -> Result<Option<usize>, ExchangeError>;
    fn stringify_and_send(&self, frame: PublicSubscription) -> Result<(), ExchangeError>;
    fn book_resubscribe_error(
        &self,
        message: &'static str,
        symbol: &[u16],
        error: ExchangeError,
    ) -> Result<(), ExchangeError>;
}

pub fn futures_on_open<H: PublicSubscriptionHost + ?Sized>(host: &H) -> Result<(), ExchangeError> {
    if !host.is_disposed() {
        let products = host.ticker_product_ids()?;
        for batch in products.chunks(50) {
            host.stringify_and_send(PublicSubscription::Futures {
                event: "subscribe",
                feed: "ticker",
                product_ids: batch.to_vec(),
            })?;
        }
        for symbol in host.book_keys()? {
            if host.book_length(&symbol)?.is_some_and(|length| length > 0) {
                let frame = PublicSubscription::Futures {
                    event: "subscribe",
                    feed: "book",
                    product_ids: vec![symbol.clone()],
                };
                if let Err(error) = host.stringify_and_send(frame) {
                    host.book_resubscribe_error(
                        "[Kraken WS] Failed to resubscribe to book for",
                        &symbol,
                        error,
                    )?;
                }
            }
        }
        for key in host.ohlcv_keys()? {
            if host.ohlcv_length(&key)?.is_some_and(|length| length > 0) {
                // Source reads active handlers but sends no Futures OHLCV frame.
            }
        }
    }
    Ok(())
}

fn split_topic(key: &[u16]) -> (&[u16], &[u16]) {
    let Some(first) = key.windows(2).position(|pair| pair == [58, 58]) else {
        return (key, &[]);
    };
    let rest = &key[first + 2..];
    let end = rest
        .windows(2)
        .position(|pair| pair == [58, 58])
        .unwrap_or(rest.len());
    (&key[..first], &rest[..end])
}

fn spot_interval(timeframe: &[u16]) -> TableHit<f64> {
    match String::from_utf16(timeframe)
        .ok()
        .map(|key| js_table_lookup(crate::spot_types::INTERVAL, &key))
    {
        Some(TableHit::Mapped(value)) => TableHit::Mapped(*value),
        Some(TableHit::Inherited(member)) => TableHit::Inherited(member),
        _ => TableHit::Mapped(1.0),
    }
}

pub fn spot_on_open<H: PublicSubscriptionHost + ?Sized>(host: &H) -> Result<(), ExchangeError> {
    if !host.is_disposed() {
        let symbols = host.market_symbols()?;
        for batch in symbols.chunks(50) {
            host.stringify_and_send(PublicSubscription::SpotTicker {
                method: "subscribe",
                channel: "ticker",
                symbols: batch.to_vec(),
                event_trigger: "trades",
            })?;
        }
        for symbol in host.book_keys()? {
            if host.book_length(&symbol)?.is_some_and(|length| length > 0) {
                host.stringify_and_send(PublicSubscription::SpotBook {
                    method: "subscribe",
                    channel: "book",
                    symbols: vec![symbol],
                    depth: 100.0,
                })?;
            }
        }
        for key in host.ohlcv_keys()? {
            if host.ohlcv_length(&key)?.is_some_and(|length| length > 0) {
                let (symbol, timeframe) = split_topic(&key);
                host.stringify_and_send(PublicSubscription::SpotOhlc {
                    method: "subscribe",
                    channel: "ohlc",
                    symbols: vec![symbol.to_vec()],
                    interval: spot_interval(timeframe),
                })?;
            }
        }
    }
    Ok(())
}
