use super::helpers::{
    get_copier_symbol_from_master, get_copier_weight_ratio, get_master_symbol_from_copier,
    symbol_mapping,
};
use super::{
    CrossTradeConfig, CrossTradeCopier, CrossTradeExchange, CrossTradeGetExchangeFn,
    CrossTradeLoop, CrossTradeLoopFn, CrossTradeOrder, CrossTradePlaceOrderFn, CrossTradeRuntime,
    CrossTradeValue, done, first_price, join_all, truthy,
};
use crate::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::TableHit;
use safe_cex_num::sizing::convert_from_base_size_to_native;
use safe_cex_types::PositionSide;
use safe_cex_types::{Market, OrderSide};
use std::cell::Cell;
use std::rc::Rc;
use std::{cell::RefCell, collections::HashMap};
#[derive(Clone)]
pub struct CrossTradeManagerDeps {
    pub get_exchange: CrossTradeGetExchangeFn,
    pub place_order: CrossTradePlaceOrderFn,
    pub runtime: Rc<dyn CrossTradeRuntime>,
}
#[derive(Debug, Clone, Copy, Default)]
pub struct FailureEntry {
    pub failures: f64,
    pub muted_until: f64,
}
pub fn debug(runtime: &dyn CrossTradeRuntime, message: &str) {
    runtime.debug(&format!("[CrossTradeManager] {message}"));
}
pub struct CrossTradeManager {
    current_loop: RefCell<Option<CrossTradeLoop>>,
    failures: RefCell<HashMap<String, Rc<Cell<FailureEntry>>>>,
    deps: CrossTradeManagerDeps,
}
impl CrossTradeManager {
    pub fn new(deps: CrossTradeManagerDeps) -> Rc<Self> {
        Rc::new(Self {
            current_loop: RefCell::new(None),
            failures: RefCell::new(HashMap::new()),
            deps,
        })
    }
    pub fn loop_state(&self) -> Option<CrossTradeLoop> {
        self.current_loop.borrow().clone()
    }
    pub fn get_exchange(
        &self,
        account: &str,
    ) -> Result<Option<Rc<dyn CrossTradeExchange>>, ExchangeError> {
        (self.deps.get_exchange)(account)
    }
    pub fn place_order(
        &self,
        request: CrossTradeOrder,
    ) -> ExchangeFuture<'static, Option<CrossTradeValue>> {
        (self.deps.place_order)(request)
    }
    pub fn failure_key(account: &str, symbol: &str) -> String {
        format!("{account}:{symbol}")
    }
    pub fn failure_entry(&self, account: &str, symbol: &str) -> Option<FailureEntry> {
        self.failures
            .borrow()
            .get(&Self::failure_key(account, symbol))
            .map(|entry| entry.get())
    }
    pub fn is_muted(&self, account: &str, symbol: &str) -> bool {
        self.failure_entry(account, symbol)
            .is_some_and(|entry| entry.muted_until > self.deps.runtime.now_ms())
    }
    pub fn record_failure(&self, account: &str, symbol: &str) {
        let key = Self::failure_key(account, symbol);
        let entry = self
            .failures
            .borrow()
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Rc::new(Cell::new(FailureEntry::default())));
        let mut state = entry.get();
        state.failures += 1.0;
        entry.set(state);
        if state.failures >= 2.0 {
            // Capture the same entry through reentrant clock/log callbacks;
            // an existing entry is already updated before those callbacks.
            let until = self.deps.runtime.now_ms() + 30_000.0;
            state = entry.get();
            state.muted_until = until;
            entry.set(state);
            debug(
                self.deps.runtime.as_ref(),
                &format!("Muting {key} until {until}"),
            );
        }
        self.failures.borrow_mut().insert(key, entry);
    }

    pub fn record_success(&self, account: &str, symbol: &str) {
        self.failures
            .borrow_mut()
            .remove(&Self::failure_key(account, symbol));
    }
    pub fn meets_min_amount(market: &Market, amount: f64) -> bool {
        let minimum = market.limits.amount.min;
        if minimum <= 0.0 {
            true
        } else {
            amount.abs() >= minimum
        }
    }
    pub fn safe_place_order(
        self: &Rc<Self>,
        request: CrossTradeOrder,
    ) -> ExchangeFuture<'static, Option<CrossTradeValue>> {
        if self.is_muted(&request.account, &request.market.symbol) {
            debug(
                self.deps.runtime.as_ref(),
                &format!(
                    "Skipping muted {} {}",
                    request.account, request.market.symbol
                ),
            );
            return done(Ok(None));
        }
        if !request.reduce_only && !Self::meets_min_amount(&request.market, request.amount) {
            debug(
                self.deps.runtime.as_ref(),
                &format!(
                    "Skipping below-min-amount order {} {} {} min {}",
                    request.account,
                    request.market.symbol,
                    request.amount,
                    request.market.limits.amount.min
                ),
            );
            return done(Ok(None));
        }
        let pending = self.place_order(request.clone());
        let manager = self.clone();
        Box::pin(async move {
            match pending.await {
                Ok(value) => {
                    manager.record_success(&request.account, &request.market.symbol);
                    Ok(value)
                }
                Err(_error) => {
                    manager.record_failure(&request.account, &request.market.symbol);
                    Ok(None)
                }
            }
        })
    }
    pub fn audit_copier_positions(
        self: &Rc<Self>,
        config: &CrossTradeConfig,
        copier: &CrossTradeCopier,
        master: Rc<dyn CrossTradeExchange>,
        copy: Rc<dyn CrossTradeExchange>,
    ) -> ExchangeFuture<'static, Vec<Option<CrossTradeValue>>> {
        let Some(account) = copier
            .account
            .as_ref()
            .filter(|account| !account.is_empty())
        else {
            return done(Ok(vec![]));
        };
        let weight = if truthy(config.master_weight) {
            config.master_weight
        } else {
            1.0
        };
        let hedged = copier.weight / weight < 0.0;
        let cs = copy.store();
        let cn = copy.name();
        let ms = master.store();
        let mn = master.name();
        let master_positions: Vec<_> = ms
            .positions
            .iter()
            .filter(|position| truthy(position.contracts))
            .collect();
        let relevant = config
            .master_symbol
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|symbol| get_copier_symbol_from_master(copier, Some(&cn), &mn, symbol));
        let positions: Vec<_> = cs
            .positions
            .iter()
            .filter(|position| {
                truthy(position.contracts)
                    && relevant.as_ref().is_none_or(|symbol| match symbol {
                        TableHit::Mapped(symbol) => symbol.is_empty() || position.symbol == *symbol,
                        TableHit::Inherited(_) => false,
                        TableHit::Missing => true,
                    })
            })
            .collect();
        let mut requests = vec![];
        for position in positions {
            let cm = cs
                .markets
                .iter()
                .find(|market| market.symbol == position.symbol);
            let ct = cs
                .tickers
                .iter()
                .find(|ticker| ticker.symbol == position.symbol);
            let symbol =
                get_master_symbol_from_copier(config, copier, Some(&mn), &cn, &position.symbol);
            let symbol = match symbol {
                TableHit::Mapped(symbol) => symbol,
                TableHit::Inherited(member) => {
                    debug(
                        self.deps.runtime.as_ref(),
                        &format!(
                            "Could not find market for {} {}",
                            position.symbol,
                            member.js_string()
                        ),
                    );
                    requests.push(done(Ok(None)));
                    continue;
                }
                TableHit::Missing => {
                    debug(
                        self.deps.runtime.as_ref(),
                        &format!("Could not find market for {} undefined", position.symbol),
                    );
                    requests.push(done(Ok(None)));
                    continue;
                }
            };
            let mm = ms.markets.iter().find(|market| market.symbol == symbol);
            let mt = ms.tickers.iter().find(|ticker| ticker.symbol == symbol);
            let (Some(cm), Some(mm)) = (cm, mm) else {
                debug(
                    self.deps.runtime.as_ref(),
                    &format!("Could not find market for {} {symbol}", position.symbol),
                );
                requests.push(done(Ok(None)));
                continue;
            };
            let mut order_price = ct.map(|ticker| ticker.last);
            let mut reference = order_price;
            if cm.base == mm.base {
                reference = first_price(
                    first_price(reference, mt.map(|ticker| ticker.last)),
                    Some(position.entry_price),
                );
                order_price = first_price(order_price, mt.map(|ticker| ticker.last));
            }
            if !reference.is_some_and(truthy) {
                requests.push(done(Ok(None)));
                continue;
            }
            let selected = symbol_mapping(&copier.symbol_mappings, &symbol);
            let matches_selected = match selected {
                TableHit::Mapped(value) => value.is_empty() || value == position.symbol,
                TableHit::Inherited(_) => false,
                TableHit::Missing => true,
            };
            let found = if matches_selected {
                master_positions
                    .iter()
                    .find(|p| {
                        p.symbol == symbol
                            && if hedged {
                                p.side.value() != position.side.value()
                            } else {
                                p.side.value() == position.side.value()
                            }
                    })
                    .copied()
            } else {
                None
            };
            let (side, amount, reduce_only) = if let Some(found) = found {
                let ratio =
                    get_copier_weight_ratio(config, copier, &master.store(), &copy.store(), mm, cm);
                let mut size = found.contracts.abs();
                if mm.base != cm.base {
                    if !mt.is_some_and(|t| truthy(t.last)) || !ct.is_some_and(|t| truthy(t.last)) {
                        debug(self.deps.runtime.as_ref(), "No ticker for size conversion");
                        requests.push(done(Ok(None)));
                        continue;
                    }
                    size = mt.unwrap().last * size / ct.unwrap().last;
                }
                let copier_size = position.contracts.abs();
                if !truthy(ratio) {
                    requests.push(done(Ok(None)));
                    continue;
                }
                let difference = size * ratio - copier_size;
                let native = convert_from_base_size_to_native(difference, cm, reference.unwrap());
                // `!(x > 0.05)` is NOT `x <= 0.05` here: `difference /
                // position.contracts` is 0/0 -> NaN for a zero-contract
                // position, and the source's negated comparison takes the
                // true branch on NaN while the rewritten one would not.
                // Keep the negation; it reproduces the TypeScript.
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                if native == 0.0 || !((difference / position.contracts).abs() > 0.05) {
                    requests.push(done(Ok(None)));
                    continue;
                }
                // Source nested comparisons use >0 in both branches; NaN selects Buy for short.
                let side = if position.side == PositionSide::Long {
                    if native > 0.0 {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    }
                } else if native > 0.0 {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                (side, native.abs(), native < 0.0)
            } else {
                let amount = position
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.contracts_native)
                    .filter(|value| truthy(*value))
                    .unwrap_or(position.contracts)
                    .abs();
                (
                    if position.side == PositionSide::Long {
                        OrderSide::Sell
                    } else {
                        OrderSide::Buy
                    },
                    amount,
                    true,
                )
            };
            requests.push(self.safe_place_order(CrossTradeOrder {
                copier: copier.clone(),
                market: cm.clone(),
                reference_price: order_price,
                account: account.clone(),
                side,
                amount,
                reduce_only,
            }));
        }
        Box::pin(join_all(requests))
    }
    pub fn audit_master_positions(
        self: &Rc<Self>,
        config: &CrossTradeConfig,
        copier: &CrossTradeCopier,
        master: Rc<dyn CrossTradeExchange>,
        copy: Rc<dyn CrossTradeExchange>,
    ) -> ExchangeFuture<'static, Vec<Option<CrossTradeValue>>> {
        let Some(account) = copier
            .account
            .as_ref()
            .filter(|account| !account.is_empty())
        else {
            return done(Ok(vec![]));
        };
        let weight = if truthy(config.master_weight) {
            config.master_weight
        } else {
            1.0
        };
        let hedged = copier.weight / weight < 0.0;
        let cs = copy.store();
        let cn = copy.name();
        let mn = master.name();
        let ms = master.store();
        let positions: Vec<_> = ms
            .positions
            .iter()
            .filter(|position| {
                truthy(position.contracts)
                    && config
                        .master_symbol
                        .as_ref()
                        .is_none_or(|symbol| symbol.is_empty() || position.symbol == *symbol)
            })
            .collect();
        let copier_positions: Vec<_> = cs
            .positions
            .iter()
            .filter(|position| truthy(position.contracts))
            .collect();
        let mut requests = vec![];
        for position in positions {
            let mm = ms
                .markets
                .iter()
                .find(|market| market.symbol == position.symbol);
            let mt = ms
                .tickers
                .iter()
                .find(|ticker| ticker.symbol == position.symbol);
            let symbol = get_copier_symbol_from_master(copier, Some(&cn), &mn, &position.symbol);
            let symbol = match symbol {
                TableHit::Mapped(symbol) => symbol,
                TableHit::Inherited(member) => {
                    debug(
                        self.deps.runtime.as_ref(),
                        &format!(
                            "Could not find market for {} {}",
                            position.symbol,
                            member.js_string()
                        ),
                    );
                    requests.push(done(Ok(None)));
                    continue;
                }
                TableHit::Missing => {
                    debug(
                        self.deps.runtime.as_ref(),
                        &format!("Could not find market for {} undefined", position.symbol),
                    );
                    requests.push(done(Ok(None)));
                    continue;
                }
            };
            let cm = cs.markets.iter().find(|market| market.symbol == symbol);
            let ct = cs.tickers.iter().find(|ticker| ticker.symbol == symbol);
            let (Some(cm), Some(mm)) = (cm, mm) else {
                debug(
                    self.deps.runtime.as_ref(),
                    &format!("Could not find market for {} {symbol}", position.symbol),
                );
                requests.push(done(Ok(None)));
                continue;
            };
            let mut reference = ct.map(|ticker| ticker.last);
            let mut order_price = reference;
            if cm.base == mm.base {
                reference = first_price(
                    first_price(reference, mt.map(|ticker| ticker.last)),
                    Some(position.entry_price),
                );
                order_price = first_price(order_price, mt.map(|ticker| ticker.last));
            }
            if !reference.is_some_and(truthy) {
                requests.push(done(Ok(None)));
                continue;
            }
            let found = copier_positions.iter().find(|p| {
                p.symbol == symbol
                    && if hedged {
                        p.side.value() != position.side.value()
                    } else {
                        p.side.value() == position.side.value()
                    }
            });
            if found.is_some() {
                requests.push(done(Ok(None)));
                continue;
            }
            let mut size = position.contracts.abs();
            if mm.base != cm.base {
                if !mt.is_some_and(|t| truthy(t.last)) || !ct.is_some_and(|t| truthy(t.last)) {
                    debug(self.deps.runtime.as_ref(), "No ticker for size conversion");
                    requests.push(done(Ok(None)));
                    continue;
                }
                size = mt.unwrap().last * size / ct.unwrap().last;
            } else {
                reference = first_price(
                    first_price(reference, mt.map(|t| t.last)),
                    Some(position.entry_price),
                );
                order_price = first_price(order_price, mt.map(|t| t.last));
            }
            let ratio =
                get_copier_weight_ratio(config, copier, &master.store(), &copy.store(), mm, cm);
            if !truthy(ratio) || !reference.is_some_and(truthy) {
                requests.push(done(Ok(None)));
                continue;
            }
            let amount = convert_from_base_size_to_native(size * ratio, cm, reference.unwrap());
            let ordinary = if position.side == PositionSide::Long {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };
            let side = if hedged {
                if ordinary == OrderSide::Buy {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                }
            } else {
                ordinary
            };
            requests.push(self.safe_place_order(CrossTradeOrder {
                copier: copier.clone(),
                market: cm.clone(),
                reference_price: order_price,
                account: account.clone(),
                side,
                amount,
                reduce_only: false,
            }));
        }
        Box::pin(join_all(requests))
    }
    fn process_config(
        self: &Rc<Self>,
        config: CrossTradeConfig,
        disposed: Rc<Cell<bool>>,
    ) -> ExchangeFuture<'static, ()> {
        let Some(account) = config.master_account.as_ref().filter(|v| !v.is_empty()) else {
            debug(
                self.deps.runtime.as_ref(),
                "No master account for cross trade config",
            );
            return done(Ok(()));
        };
        let master = match self.get_exchange(account) {
            Ok(Some(exchange)) => exchange,
            Ok(None) => {
                debug(self.deps.runtime.as_ref(), "Master exchange not loaded");
                return done(Ok(()));
            }
            Err(error) => return done(Err(error)),
        };
        if !master.store().positions_loaded {
            debug(self.deps.runtime.as_ref(), "Master exchange not loaded");
            return done(Ok(()));
        }
        let mut requests = vec![];
        for copier in &config.copiers {
            let Some(account) = copier
                .account
                .as_ref()
                .filter(|v| !v.is_empty() && copier.enabled)
            else {
                debug(self.deps.runtime.as_ref(), "Invalid copier");
                requests.push(done(Ok(())));
                continue;
            };
            let copy = match self.get_exchange(account) {
                Ok(Some(exchange)) => exchange,
                Ok(None) => {
                    debug(self.deps.runtime.as_ref(), "No copier exchange");
                    requests.push(done(Ok(())));
                    continue;
                }
                Err(error) => {
                    requests.push(done(Err(error)));
                    continue;
                }
            };
            if !copy.store().positions_loaded {
                debug(self.deps.runtime.as_ref(), "Copier exchange not loaded");
                requests.push(done(Ok(())));
                continue;
            }
            let pending =
                self.audit_copier_positions(&config, copier, master.clone(), copy.clone());
            let config = config.clone();
            let copier = copier.clone();
            let manager = self.clone();
            let master = master.clone();
            let disposed = disposed.clone();
            requests.push(Box::pin(async move {
                pending.await?;
                if manager.current_loop.borrow().is_none() || disposed.get() {
                    return Ok(());
                }
                manager
                    .audit_master_positions(&config, &copier, master, copy)
                    .await?;
                Ok(())
            }));
        }
        Box::pin(async move { join_all(requests).await.map(|_| ()) })
    }
    pub fn run_loop(
        self: &Rc<Self>,
        configs: Rc<Vec<CrossTradeConfig>>,
        disposed: Rc<Cell<bool>>,
    ) -> ExchangeFuture<'static, ()> {
        if self.current_loop.borrow().is_none() || disposed.get() {
            return done(Ok(()));
        }
        // Deliberately no config.enabled guard: the source ignores that flag.
        let requests = configs
            .iter()
            .map(|config| self.process_config(config.clone(), disposed.clone()))
            .collect();
        let manager = self.clone();
        Box::pin(async move {
            if let Err(error) = join_all(requests).await {
                manager
                    .deps
                    .runtime
                    .error("Error in cross trade manager", &error);
            }
            if manager.current_loop.borrow().is_some() && !disposed.get() {
                let next = manager
                    .current_loop
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .callback
                    .clone();
                manager.deps.runtime.schedule_loop(next, 3000);
            }
            Ok(())
        })
    }
    pub fn start(self: &Rc<Self>, configs: Vec<CrossTradeConfig>) {
        if let Some(previous) = self.current_loop.borrow().as_ref() {
            previous.disposed.set(true);
        }
        debug(self.deps.runtime.as_ref(), "Starting cross trade manager");
        let disposed = Rc::new(Cell::new(false));
        let state = disposed.clone();
        let configs = Rc::new(configs);
        let manager = Rc::downgrade(self);
        let callback: CrossTradeLoopFn = Rc::new(move || {
            manager.upgrade().map_or_else(
                || done(Ok(())),
                |manager| manager.run_loop(configs.clone(), state.clone()),
            )
        });
        self.deps.runtime.schedule_loop(callback.clone(), 3000);
        *self.current_loop.borrow_mut() = Some(CrossTradeLoop { disposed, callback });
    }
    pub fn stop(&self) {
        let has_loop = self.current_loop.borrow().is_some();
        if has_loop {
            debug(self.deps.runtime.as_ref(), "Stopping cross trade manager");
            // Source re-reads this._loop after logging; host logging can reenter start.
            if let Some(current) = self.current_loop.borrow().as_ref() {
                current.disposed.set(true);
            }
        }
        self.failures.borrow_mut().clear();
    }
}
