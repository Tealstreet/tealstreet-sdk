//! BingX constructor and startup compose the landed REST and stream bodies.
use crate::{
    amendment::BingxAmendment,
    api::{ApiHost, BingxApi, create_api},
    data_reads::{BingxDataReads, ReadTransport},
    listen_key::ListenKeyHost,
    placement::{BingxPlacement, PlacementHost, create_client_order_id},
    private_dispatch::PrivateMessageHost,
    private_events::PrivateEventHost,
    public_stream::PublicSocketHost,
    read_history::{BingxReadHistory, HistoryHost, dispose_exchange},
    stream_subscriptions::SubscriptionHost,
    websocket::{BingxPrivateWebsocket, BingxPublicWebsocket, WebsocketBindings},
};
use safe_cex_exchange::{
    client_order_id::ClientOrderIdRule,
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{rc::Rc, task::Poll};

pub type ApiFactory<'a> = dyn Fn(ExchangeOptions) -> Result<Rc<BingxApi>, ExchangeError> + 'a;
/// Imported BaseExchange external-data and secondary-key/rate-limit operations.
pub trait ExchangeHost {
    fn signers(&self) -> safe_cex_exchange::runtime::ExchangeSigners {
        Default::default()
    }
    fn secondary(
        &self,
        options: &ExchangeOptions,
        create: &ApiFactory<'_>,
        max_rps: f64,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError>;
    fn init_external_public_data(&self, base: Rc<BaseExchange>) -> ExchangeFuture<'_, bool>;
    fn is_public_data_only(&self, base: &BaseExchange) -> bool;
}
pub struct BingxHosts {
    pub api: Rc<dyn ApiHost>,
    pub exchange: Rc<dyn ExchangeHost>,
    pub history: Rc<dyn HistoryHost>,
    pub placement: Rc<dyn PlacementHost>,
    pub public_socket: Rc<dyn PublicSocketHost>,
    pub subscription: Rc<dyn SubscriptionHost>,
    pub private_keys: Rc<dyn ListenKeyHost>,
    pub private_events: Rc<dyn PrivateEventHost>,
    pub private_message: Rc<dyn PrivateMessageHost>,
    pub bindings: Rc<dyn WebsocketBindings>,
}
pub struct BingxExchange {
    pub base: Rc<BaseExchange>,
    pub xhr: Rc<dyn ReadTransport>,
    pub unlimited_xhr: Rc<BingxApi>,
    pub reads: Rc<BingxDataReads>,
    pub history: Rc<BingxReadHistory>,
    pub placement: Rc<BingxPlacement>,
    pub amendment: Rc<BingxAmendment>,
    pub public_websocket: Rc<BingxPublicWebsocket>,
    pub private_websocket: Rc<BingxPrivateWebsocket>,
    pub host: Rc<dyn ExchangeHost>,
    pub grafana_endpoints_every: [&'static str; 7],
    pub grafana_endpoints_periodic: [&'static str; 4],
}
impl BingxExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        runtime: Rc<dyn ExchangeRuntime>,
        hosts: BingxHosts,
    ) -> Result<Rc<Self>, ExchangeError> {
        let mint = hosts.placement.clone();
        let mut base = BaseExchange::new(
            name,
            options.clone(),
            store,
            runtime,
            ExchangeSettings {
                supports_public_only: true,
                tick_interval: Some(2000.0),
                client_order_id_rule: Some(ClientOrderIdRule {
                    label: "BingX".into(),
                    requirement: "be 1-40 characters with no uppercase letters".into(),
                    is_valid: Rc::new(|id| {
                        let len = id.encode_utf16().count();
                        (1..=40).contains(&len) && id == id.to_lowercase()
                    }),
                    is_honorable_shape: None,
                    mint: Some(Rc::new(move || {
                        create_client_order_id(&mint.uuid().unwrap_or_else(|e| {
                            panic!("BingX client ID mint failed: {}", e.message)
                        }))
                    })),
                }),
                ..Default::default()
            },
        );
        base.signers = hosts.exchange.signers();
        let base = Rc::new(base);
        base.bind_http_endpoint_lists(
            &crate::constants::GRAFANA_ENDPOINTS_EVERY,
            &crate::constants::GRAFANA_ENDPOINTS_PERIODIC,
        );
        let factory = |opts| create_api(opts, &base, hosts.api.clone());
        let xhr = hosts.exchange.secondary(&options, &factory, 3.0)?;
        let unlimited_xhr = create_api(options, &base, hosts.api)?;
        let reads = Rc::new(BingxDataReads::new(base.clone(), xhr.clone()));
        let history = BingxReadHistory::new(reads.clone(), xhr.clone(), hosts.history);
        let placement = BingxPlacement::new(reads.clone(), unlimited_xhr.clone(), hosts.placement);
        let amendment = Rc::new(BingxAmendment {
            reads: reads.clone(),
            history: history.clone(),
            placement: placement.clone(),
            transport: xhr.clone(),
        });
        let public_websocket = BingxPublicWebsocket::new(
            reads.clone(),
            hosts.public_socket,
            hosts.subscription,
            hosts.bindings.clone(),
        )?;
        let private_websocket = BingxPrivateWebsocket::new(
            reads.clone(),
            xhr.clone(),
            hosts.private_keys,
            hosts.private_events,
            hosts.private_message,
            hosts.bindings,
        )?;
        Ok(Rc::new(Self {
            base,
            xhr,
            unlimited_xhr,
            reads,
            history,
            placement,
            amendment,
            public_websocket,
            private_websocket,
            host: hosts.exchange,
            grafana_endpoints_every: crate::constants::GRAFANA_ENDPOINTS_EVERY,
            grafana_endpoints_periodic: crate::constants::GRAFANA_ENDPOINTS_PERIODIC,
        }))
    }
    pub async fn start(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let external = self
            .host
            .init_external_public_data(self.base.clone())
            .await?;
        let public_only = self.host.is_public_data_only(&self.base);
        if !external {
            let markets_owner = self.clone();
            let ticker_owner = self.clone();
            let mut market_count = 0;
            let results = join(
                self.base.clone(),
                vec![
                    Box::pin(async move {
                        let markets = markets_owner.reads.fetch_markets().await?;
                        let mut yielded = false;
                        std::future::poll_fn(|cx| {
                            if yielded {
                                Poll::Ready(())
                            } else {
                                yielded = true;
                                cx.waker().wake_by_ref();
                                Poll::Pending
                            }
                        })
                        .await;
                        if markets_owner.base.is_disposed() {
                            return Ok(StartupValue::Markets(0));
                        }
                        let count = markets.len();
                        let mut loaded = markets_owner.base.store.loaded()?.clone();
                        loaded.markets = true;
                        markets_owner
                            .base
                            .store
                            .update(
                                StoreUpdate {
                                    markets: Some(markets),
                                    loaded: Some(loaded),
                                    ..Default::default()
                                },
                                false,
                            )
                            .map_err(ExchangeError::from)?;
                        markets_owner
                            .public_websocket
                            .stream
                            .connect_and_subscribe()?;
                        Ok(StartupValue::Markets(count))
                    }),
                    Box::pin(async move {
                        Ok(StartupValue::Tickers(
                            ticker_owner.reads.fetch_tickers_raw().await?,
                        ))
                    }),
                ],
            )
            .await?;
            if self.base.is_disposed() {
                return Ok(());
            }
            let mut raw = None;
            for value in results {
                match value {
                    StartupValue::Markets(count) => market_count = count,
                    StartupValue::Tickers(t) => raw = Some(t),
                    StartupValue::Unit => {}
                }
            }
            let tickers = self.reads.format_tickers(
                &raw.ok_or_else(|| ExchangeError::new("BingX startup ticker result missing"))?,
            )?;
            let count = tickers.len().min(market_count);
            let mut loaded = self.base.store.loaded()?.clone();
            loaded.tickers = true;
            self.base
                .store
                .update(
                    StoreUpdate {
                        tickers: Some(tickers),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ExchangeError::from)?;
            self.base
                .log(&format!("Loaded {count} Bingx markets"), "info")?;
        }
        if public_only {
            let mut loaded = self.base.store.loaded()?.clone();
            loaded.balance = true;
            loaded.positions = true;
            loaded.orders = true;
            self.base
                .store
                .update(
                    StoreUpdate {
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ExchangeError::from)?;
            return Ok(());
        }
        let private = self.private_websocket.clone();
        let runtime = self.base.runtime.clone();
        let reporter = runtime.clone();
        runtime.spawn_local(Box::pin(async move {
            if let Err(error) = private.keys.connect_and_subscribe().await {
                reporter.report_exception(
                    &error,
                    &serde_json::json!({"operation":"BingX startup private connect"}).into(),
                );
            }
        }));
        self.reads.get_or_fetch_position_mode(true).await?;
        let orders = self.clone();
        let positions = self.clone();
        let balance = self.clone();
        join(
            self.base.clone(),
            vec![
                Box::pin(async move {
                    let fetched = orders.history.fetch_orders().await?;
                    if !orders.base.is_disposed() {
                        let mut loaded = orders.base.store.loaded()?.clone();
                        loaded.orders = true;
                        orders
                            .base
                            .store
                            .update(
                                StoreUpdate {
                                    orders: Some(fetched),
                                    loaded: Some(loaded),
                                    ..Default::default()
                                },
                                false,
                            )
                            .map_err(ExchangeError::from)?;
                    }
                    Ok(StartupValue::Unit)
                }),
                Box::pin(async move {
                    let fetched = positions.reads.fetch_positions(None).await?;
                    if !positions.base.is_disposed() {
                        positions
                            .base
                            .store
                            .add_or_update_positions_full(fetched)
                            .map_err(ExchangeError::from)?;
                        let mut loaded = positions.base.store.loaded()?.clone();
                        loaded.positions = true;
                        positions
                            .base
                            .store
                            .update(
                                StoreUpdate {
                                    loaded: Some(loaded),
                                    ..Default::default()
                                },
                                false,
                            )
                            .map_err(ExchangeError::from)?;
                    }
                    Ok(StartupValue::Unit)
                }),
                Box::pin(async move {
                    let fetched = balance.reads.fetch_balance().await?;
                    if !balance.base.is_disposed() {
                        let mut loaded = balance.base.store.loaded()?.clone();
                        loaded.balance = true;
                        safe_cex_exchange::native_store::update_balance_and_loaded(
                            balance.base.store.as_ref(),
                            &fetched,
                            &loaded,
                        )
                        .map_err(ExchangeError::from)?;
                    }
                    Ok(StartupValue::Unit)
                }),
            ],
        )
        .await?;
        if self.base.is_disposed() {
            return Ok(());
        }
        self.history.tick(0.0).await?;
        self.base.log("Ready to trade on Bingx", "info")
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        dispose_exchange(
            || self.base.dispose(),
            || self.public_websocket.dispose(),
            || self.private_websocket.keys.dispose(),
        )
    }
}
enum StartupValue {
    Markets(usize),
    Tickers(crate::data_reads::RawTickers),
    Unit,
}
async fn join(
    base: Rc<BaseExchange>,
    futures: Vec<ExchangeFuture<'static, StartupValue>>,
) -> Result<Vec<StartupValue>, ExchangeError> {
    let mut futures = futures.into_iter().map(Some).collect::<Vec<_>>();
    let mut results = (0..futures.len()).map(|_| None).collect::<Vec<_>>();
    let outcome = std::future::poll_fn(|cx| {
        let mut pending = false;
        let mut error = None;
        for i in 0..futures.len() {
            if let Some(f) = &mut futures[i] {
                match f.as_mut().poll(cx) {
                    Poll::Pending => pending = true,
                    Poll::Ready(r) => {
                        futures[i] = None;
                        match r {
                            Ok(v) => results[i] = Some(v),
                            Err(e) => {
                                if error.is_none() {
                                    error = Some(e)
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(e) = error {
            Poll::Ready(Err(e))
        } else if pending {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    })
    .await;
    if let Err(error) = outcome {
        for mut future in futures.into_iter().flatten() {
            let reporter = base.runtime.clone();
            base.runtime.spawn_local(Box::pin(async move {
                if let Err(error) = future.as_mut().await {
                    reporter.report_exception(
                        &error,
                        &serde_json::json!({"operation":"BingX startup sibling"}).into(),
                    );
                }
            }));
        }
        return Err(error);
    }
    Ok(results.into_iter().flatten().collect())
}
