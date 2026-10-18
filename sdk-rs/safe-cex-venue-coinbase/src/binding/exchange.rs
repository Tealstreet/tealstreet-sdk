//! Production Coinbase Advanced Trade/Spot composition. Starbase stays inert.
use super::{error::exchange_error, lifecycle::frame, rt::Host, value::to_js};
use crate::{
    cdp_signing::CdpSigningCache,
    classes, exchange_lifecycle,
    exchange_ws_auth::{self, ExchangeWsAuthAsync, ExchangeWsAuthHost, ExchangeWsAuthReceiver},
    order_edit::{self, EditUpdate, OrderEditReceiver},
    portfolios_balances::PortfolioFailure,
    public_data::PublicData,
    public_market_reads::{self, PublicOHLCVOptions, PublicOHLCVOptionsRoot},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange, ValidatedOrder},
    js_runtime::{
        base_exchange::ExternalPublicDataFactory,
        client::JsHttpHost,
        value::{Js, to_json, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::{cell::RefCell, rc::Rc};

pub struct CoinbaseDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    /// Create once per SDK runtime; source coinbase.utils.ts caches imported keys at module scope.
    pub signing_cache: Rc<CdpSigningCache>,
    /// Host local timezone/DST conversion for native Date parsing.
    pub local_wall_time_to_utc: Rc<dyn Fn(f64) -> f64>,
}
pub struct CoinbaseExchange {
    host: Host,
    class: Rc<classes::CoinbaseExchange<Host>>,
    capabilities: ExchangeCapabilities,
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../tests/support/generic_alias_native.rs"]
mod generic_alias_native;
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../tests/support/portfolio_alias_native.rs"]
mod portfolio_alias_native;

fn result<'a, T: Clone + 'static>(
    promise: Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>,
) -> ExchangeFuture<'a, T> {
    Box::pin(async move {
        promise
            .map_err(|error| exchange_error(&error))?
            .await
            .map_err(|error| exchange_error(&error))
    })
}
fn unsupported<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}
fn options_root(options: OHLCVOptions) -> PublicOHLCVOptionsRoot {
    Rc::new(RefCell::new(PublicOHLCVOptions {
        symbol: options.symbol,
        interval: serde_json::to_value(options.interval)
            .expect("timeframe")
            .as_str()
            .expect("timeframe literal")
            .into(),
        limit: options.limit.map(serde_json::Value::from),
        from: options.from.map(serde_json::Value::from),
        to: options.to.map(serde_json::Value::from),
    }))
}
impl CoinbaseExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: CoinbaseDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if !matches!(name, ExchangeName::Coinbase | ExchangeName::Coinbasespot) {
            return Err(ExchangeError::new(
                "CoinbaseExchange requires coinbase or coinbasespot",
            ));
        }
        let host = Host::new(
            name.clone(),
            options.clone(),
            store,
            deps.runtime,
            deps.http,
            deps.external_public_data,
            deps.signing_cache,
            deps.local_wall_time_to_utc,
        );
        let class_options = frame(&options).map_err(|error| exchange_error(&error))?;
        let class_store = Frame::Object(Rc::default());
        let name_text = serde_json::to_value(&name)
            .expect("name")
            .as_str()
            .expect("name literal")
            .to_owned();
        let class = if name == ExchangeName::Coinbasespot {
            classes::CoinbaseSpotExchange::new(
                Rc::new(host.clone()),
                name_text,
                class_options,
                class_store,
                host.shared.extra.spawn.clone(),
            )
            .map(|spot| spot.exchange)
        } else {
            classes::CoinbaseExchange::new(
                Rc::new(host.clone()),
                name_text,
                class_options,
                class_store,
                host.shared.extra.spawn.clone(),
            )
        }
        .map_err(|error| exchange_error(&error))?;
        let venue = Rc::new(Self {
            host,
            class,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|venue, ()| venue.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                ..Default::default()
            },
        });
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *venue.host.shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        Ok(venue)
    }
    pub fn fetch_balance(&self) -> ExchangeFuture<'_, Balance> {
        result(self.host.balance_value())
    }
}
impl Exchange for CoinbaseExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.host.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn is_public_data_only(&self) -> bool {
        self.class.is_public_data_only()
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        exchange_lifecycle::dispose(&self.class.lifecycle_receiver(self.host.clone()))
            .map_err(|error| exchange_error(&error))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        if let Err(error) = self.base().resolve_caller_client_order_id(order.options()) {
            return Box::pin(async move { Err(error) });
        }
        result(Ok(self.host.place(order.options().clone())))
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        result(Ok(exchange_lifecycle::start(
            self.class.lifecycle_receiver(self.host.clone()),
        )))
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        let result = self.class.get_account();
        Box::pin(async move {
            let account = result.await.map_err(|error| exchange_error(&error))?;
            Ok(ExchangeAccount {
                user_id: account.borrow().user_id.clone(),
                sub_id: None,
                affiliate_id: None,
            })
        })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        let validation =
            exchange_lifecycle::validate_account(self.class.lifecycle_receiver(self.host.clone()));
        Box::pin(async move {
            let validation = validation.await.map_err(|error| exchange_error(&error))?;
            let balance = validation
                .balance
                .as_ref()
                .map(|balance| to_typed(&to_js(balance)).map_err(|error| error.to_exchange_error()))
                .transpose()?;
            Ok(ValidateAccountResult {
                error: AccountValidationError::from_value(validation.error),
                balance,
            })
        })
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        result(self.host.position_values())
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let orders = crate::order_sync::fetch_orders(self.host.order_snapshot_receiver());
        Box::pin(async move {
            Ok(Host::native_orders(
                &orders.await.map_err(|error| exchange_error(&error))?,
            ))
        })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        result(Ok(self.host.market_values()))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        result(self.host.ticker_values(false))
    }
    // ts: base.ts:799,808,832,836. Coinbase declares no leverage/mode overrides.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        unsupported()
    }
    fn set_leverage(
        &self,
        _symbol: String,
        _leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        unsupported()
    }
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        unsupported()
    }
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        unsupported()
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let update = match options.update {
            UpdateOrderOptsUpdate::Variant0(value) => EditUpdate {
                amount: Some(value.amount),
                price: value.price,
                has_amount: true,
                has_price: value.price.is_some(),
            },
            UpdateOrderOptsUpdate::Variant1(value) => EditUpdate {
                amount: value.amount,
                price: Some(value.price),
                has_amount: value.amount.is_some(),
                has_price: true,
            },
        };
        result(Ok(order_edit::update_order(
            Rc::new(OrderEditReceiver {
                host: self.host.clone(),
                spawn: self.host.shared.extra.spawn.clone(),
            }),
            Host::synced_order(options.order),
            Rc::new(RefCell::new(update)),
        )))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        result(Ok(self.host.cancel(orders)))
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let candles = public_market_reads::fetch_ohlcv(
            self.host.public_read_receiver(),
            options_root(options),
        );
        Box::pin(async move {
            Ok(candles
                .await
                .map_err(|error| exchange_error(&error))?
                .borrow()
                .iter()
                .map(|candle| candle.borrow().clone())
                .collect())
        })
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let socket = self
            .class
            .public_websocket
            .borrow()
            .clone()
            .expect("assembled public socket");
        let cleanup = socket
            .listen_ohlcv(
                options_root(options),
                Rc::new(move |candle| {
                    let candle = match candle {
                        PublicData::Candle(candle) => candle.borrow().clone(),
                        other => {
                            let value = Js::object(vec![]);
                            for (key, field) in other.fields() {
                                value
                                    .define_key(
                                        &key,
                                        field.as_ref().map(to_js).unwrap_or(Js::Undefined),
                                    )
                                    .map_err(|error| {
                                        Rc::new(
                                            crate::public_stream_dispatch::PublicStreamFailure {
                                                name: error.to_exchange_error().name,
                                                message: error.to_exchange_error().message,
                                                native: None,
                                            },
                                        )
                                    })?;
                            }
                            to_typed(&value).map_err(|error| {
                                Rc::new(crate::public_stream_dispatch::PublicStreamFailure {
                                    name: error.to_exchange_error().name,
                                    message: error.to_exchange_error().message,
                                    native: None,
                                })
                            })?
                        }
                    };
                    callback(candle);
                    Ok(())
                }),
            )
            .map_err(|error| ExchangeError::new(error.message.clone()))?;
        Ok(Box::new(move || {
            cleanup().map_err(|error| ExchangeError::new(error.message.clone()))
        }))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let socket = self
            .class
            .public_websocket
            .borrow()
            .clone()
            .expect("assembled public socket");
        let cleanup = socket
            .listen_order_book(
                symbol,
                Rc::new(move |book| {
                    callback(super::sockets::native_book(&book));
                    Ok(())
                }),
            )
            .map_err(|error| ExchangeError::new(error.message.clone()))?;
        Ok(Box::new(move || {
            cleanup().map_err(|error| ExchangeError::new(error.message.clone()))
        }))
    }
    fn sign_ws_auth<'a>(
        &'a self,
        request: &'a safe_cex_crypto::WsAuthRequest,
    ) -> ExchangeFuture<'a, safe_cex_crypto::WsAuthResponse> {
        let request = frame(request);
        let signed = request.map(|request| {
            exchange_ws_auth::sign_ws_auth(
                Rc::new(ExchangeWsAuthReceiver {
                    host: self.host.clone(),
                    spawn: self.host.shared.extra.spawn.clone(),
                }),
                Some(request),
            )
        });
        Box::pin(async move {
            let signed = signed
                .map_err(|error| exchange_error(&error))?
                .await
                .map_err(|error| exchange_error(&error))?;
            Ok(safe_cex_crypto::WsAuthResponse {
                id: signed
                    .id
                    .as_ref()
                    .map(|id| to_json(&to_js(id)).map_err(|error| error.to_exchange_error()))
                    .transpose()?,
                error: signed.error.as_ref().map(|error| to_js(error).to_display()),
                ws_auth_signature: signed
                    .signature
                    .as_ref()
                    .map(|token| to_js(token).to_display()),
            })
        })
    }
    fn class_name(&self) -> &'static str {
        if *self.name() == ExchangeName::Coinbasespot {
            "CoinbaseSpotExchange"
        } else {
            "CoinbaseExchange"
        }
    }
}
impl ExchangeWsAuthHost for Host {
    fn options(&self) -> Frame {
        self.options()
    }
    fn sign_local(&self, key: Option<Frame>, secret: Option<Frame>) -> ExchangeWsAuthAsync {
        use crate::cdp_signing::{self, CdpSigningReceiver};
        let text = |value| {
            crate::public_data::string(value)
                .map(|value| String::from_utf16_lossy(&value))
                .map_err(|error| Host::json_error(error.name, error.message))
        };
        let result = cdp_signing::sign(
            Rc::new(CdpSigningReceiver {
                host: super::crypto::CryptoHost {
                    runtime: self.runtime(),
                },
                cache: self.shared.extra.signing_cache.clone(),
                spawn: self.shared.extra.spawn.clone(),
            }),
            text(key)?,
            text(secret)?,
            None,
        );
        Ok(self.promise(async move { result.await.map(|token| Some(super::value::text(&token))) }))
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../tests/support/retained_merge_native.rs"]
mod retained_merge_native;
