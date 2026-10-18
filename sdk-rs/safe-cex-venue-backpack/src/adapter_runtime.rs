//! Stateful exchange runtime frame; source bodies own sequencing and disposal guards.
use crate::{
    adapter::{BackpackExchange, Frame},
    public_control::JsString,
    startup_recovery as source,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{Balance, Market, Order, Position, StoreDataLoaded, Ticker};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone)]
pub enum Data {
    Markets(Vec<Market>),
    Tickers(Vec<Ticker>),
    Positions(Vec<Position>),
    Balance(Rc<RefCell<Balance>>),
    Orders(Vec<Order>),
    Account,
}
pub fn loaded(base: &BaseExchange) -> Result<Vec<(JsString, bool)>, ExchangeError> {
    let l = base.store.loaded()?;
    Ok([
        ("balance", l.balance),
        ("orders", l.orders),
        ("markets", l.markets),
        ("tickers", l.tickers),
        ("positions", l.positions),
    ]
    .into_iter()
    .map(|(k, v)| (k.encode_utf16().collect(), v))
    .collect())
}
pub fn loaded_patch(v: Vec<(JsString, bool)>) -> Result<StoreDataLoaded, ExchangeError> {
    let get = |k: &str| -> bool {
        v.iter()
            .find(|(s, _)| s == &k.encode_utf16().collect::<Vec<_>>())
            .is_some_and(|(_, b)| *b)
    };
    Ok(StoreDataLoaded {
        balance: get("balance"),
        orders: get("orders"),
        markets: get("markets"),
        tickers: get("tickers"),
        positions: get("positions"),
    })
}
impl BackpackExchange {
    pub fn start(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let future = Box::pin(source::start(self.frame()));
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn tick(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let future = Box::pin(source::tick(self.frame()));
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn subscribe_to_all_markets(
        self: &Rc<Self>,
        markets: Vec<Market>,
    ) -> Result<(), ExchangeError> {
        let owner = Rc::downgrade(self);
        source::RuntimeHost::timeout(
            &mut *self.frame().borrow_mut(),
            Box::new(move || {
                if let Some(owner) = owner.upgrade() {
                    source::subscribe_markets_now(Data::Markets(markets), owner.frame())?;
                }
                Ok(())
            }),
            2000.0,
        )
    }
    pub fn spawn(&self, task: ExchangeFuture<'static, ()>) {
        let runtime = self.runtime.clone();
        let report = runtime.clone();
        self.tasks
            .borrow_mut()
            .push(runtime.spawn_local(Box::pin(async move {
                if let Err(e) = task.await {
                    report.report_exception(
                        &e,
                        &serde_json::json!({"operation":"Backpack exchange"}).into(),
                    );
                }
            })));
    }
}
impl source::Logger for Frame {
    type Error = ExchangeError;
    type LogTarget = ();
    fn log_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn log(
        &mut self,
        _: (),
        s: JsString,
        severity: Option<source::Severity>,
    ) -> Result<(), ExchangeError> {
        self.owner.base.log(
            &String::from_utf16(&s).map_err(|e| ExchangeError::new(e.to_string()))?,
            match severity {
                Some(source::Severity::Error) => "error",
                Some(source::Severity::Warning) => "warning",
                None => "info",
            },
        )
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(e.js_error_string().encode_utf16().collect())
    }
}
impl source::RuntimeHost for Frame {
    type Value = Data;
    type FetchFuture = ExchangeFuture<'static, Data>;
    type UpdateTarget = ();
    type Market = Market;
    type Subscriber = source::StreamKind;
    fn fetch(&mut self, k: source::ReadKind) -> Result<Self::FetchFuture, ExchangeError> {
        let o = self.owner.clone();
        Ok(match k {
            source::ReadKind::Markets => {
                let f = o.fetch_markets();
                Box::pin(async move {
                    Ok(Data::Markets(
                        f.await?
                            .into_iter()
                            .map(crate::model_bridge::market)
                            .collect::<Result<_, _>>()?,
                    ))
                })
            }
            source::ReadKind::Tickers => {
                let f = o.fetch_tickers();
                Box::pin(async move {
                    Ok(Data::Tickers(
                        f.await?
                            .into_iter()
                            .flatten()
                            .map(crate::model_bridge::ticker)
                            .collect::<Result<_, _>>()?,
                    ))
                })
            }
            source::ReadKind::Tick => {
                let f = o.tick();
                Box::pin(async move {
                    f.await?;
                    Ok(Data::Account)
                })
            }
            source::ReadKind::Account => {
                let f = o.fetch_account_info();
                Box::pin(async move {
                    f.await?;
                    Ok(Data::Account)
                })
            }
            source::ReadKind::Positions => {
                let f = o.fetch_positions();
                Box::pin(async move {
                    Ok(Data::Positions(
                        f.await?
                            .into_iter()
                            .map(crate::model_bridge::position)
                            .collect::<Result<_, _>>()?,
                    ))
                })
            }
            source::ReadKind::Balance => {
                let f = o.fetch_balance();
                Box::pin(async move { Ok(Data::Balance(Rc::new(RefCell::new(f.await?)))) })
            }
            source::ReadKind::Orders => {
                let f = o.fetch_orders(None);
                Box::pin(async move { Ok(Data::Orders(f.await?.into_iter().flatten().collect())) })
            }
        })
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_disposed())
    }
    fn public_only(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.options().public_data_only == Some(true) || !self.owner.base.can_sign())
    }
    fn update_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn loaded(&mut self) -> Result<Vec<(JsString, bool)>, ExchangeError> {
        loaded(&self.owner.base)
    }
    fn update(
        &mut self,
        _: (),
        _: Option<source::ReadKind>,
        v: Option<Data>,
        l: Vec<(JsString, bool)>,
    ) -> Result<(), ExchangeError> {
        let mut u = safe_cex_store::StoreUpdate {
            loaded: Some(loaded_patch(l)?),
            ..Default::default()
        };
        if let Some(v) = v {
            match v {
                Data::Markets(m) => u.markets = Some(m),
                Data::Tickers(t) => u.tickers = Some(t),
                Data::Positions(p) => u.positions = Some(p),
                Data::Balance(b) => u.balance = Some(b.borrow().clone()),
                Data::Orders(o) => u.orders = Some(o),
                Data::Account => {}
            }
        }
        self.owner.base.store.update(u, false).map_err(Into::into)
    }
    fn length(&mut self, v: &Data) -> Result<usize, ExchangeError> {
        match v {
            Data::Markets(v) => Ok(v.len()),
            Data::Tickers(v) => Ok(v.len()),
            Data::Positions(v) => Ok(v.len()),
            Data::Orders(v) => Ok(v.len()),
            _ => Err(ExchangeError::new("Value has no array length")),
        }
    }
    fn connect_public(&mut self) -> Result<(), ExchangeError> {
        self.owner
            .public_websocket
            .get()
            .unwrap()
            .connect_and_subscribe()
    }
    fn connect_private(&mut self) -> Result<(), ExchangeError> {
        self.owner
            .private_websocket
            .get()
            .unwrap()
            .connect_and_subscribe()
    }
    fn subscribe_all(&mut self, m: Data) -> Result<(), ExchangeError> {
        let Data::Markets(markets) = m else {
            return Err(ExchangeError::new("Market kind mismatch"));
        };
        self.owner.subscribe_to_all_markets(markets)
    }
    fn positions_upnl(&mut self, p: &Data) -> Result<f64, ExchangeError> {
        let Data::Positions(p) = p else {
            return Err(ExchangeError::new("Position kind mismatch"));
        };
        Ok(p.iter()
            .map(|p| p.unrealized_pnl)
            .reduce(|a, b| a + b)
            .unwrap_or(0.0))
    }
    fn set_balance_upnl(&mut self, b: &Data, n: f64) -> Result<(), ExchangeError> {
        let Data::Balance(b) = b else {
            return Err(ExchangeError::new("Balance kind mismatch"));
        };
        b.borrow_mut().upnl = n;
        Ok(())
    }
    fn error_message(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(e.message.encode_utf16().collect())
    }
    fn schedule_loop(
        &mut self,
        cb: source::Callback<ExchangeError>,
        d: f64,
    ) -> Result<(), ExchangeError> {
        // This source loop callback is exactly repeat_tick; do not let its
        // captured Frame keep the final native Exchange handle alive forever.
        drop(cb);
        let owner = Rc::downgrade(&self.owner);
        self.timeout(
            Box::new(move || {
                if let Some(owner) = owner.upgrade() {
                    source::RuntimeHost::repeat_tick(&mut Frame { owner })?;
                }
                Ok(())
            }),
            d,
        )
    }
    fn repeat_tick(&mut self) -> Result<(), ExchangeError> {
        self.owner.spawn(self.owner.tick());
        Ok(())
    }
    fn timeout(
        &mut self,
        cb: source::Callback<ExchangeError>,
        d: f64,
    ) -> Result<(), ExchangeError> {
        let sleep = self.owner.runtime.sleep(d as u64);
        self.owner.spawn(Box::pin(async move {
            sleep.await?;
            cb()
        }));
        Ok(())
    }
    fn market_at(&mut self, m: &Data, i: usize) -> Result<Market, ExchangeError> {
        let Data::Markets(m) = m else {
            return Err(ExchangeError::new("Market kind mismatch"));
        };
        m.get(i)
            .cloned()
            .ok_or_else(|| ExchangeError::new("Market index out of bounds"))
    }
    fn market_id(&mut self, m: &Market) -> Result<JsString, ExchangeError> {
        Ok(m.id.encode_utf16().collect())
    }
    fn subscriber(&mut self, k: source::StreamKind) -> Result<Self::Subscriber, ExchangeError> {
        Ok(k)
    }
    fn subscribe(&mut self, k: source::StreamKind, s: JsString) -> Result<(), ExchangeError> {
        let p = self.owner.public_websocket.get().unwrap();
        match k {
            source::StreamKind::Ticker => p.subscribe_to_ticker(&s),
            source::StreamKind::Trades => p.subscribe_to_trades(&s),
            source::StreamKind::BookTicker => p.subscribe_to_book_ticker(&s),
        }
    }
}
