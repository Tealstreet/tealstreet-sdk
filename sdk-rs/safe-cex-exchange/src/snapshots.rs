use crate::runtime::{ExchangeError, ExchangeTask};
use safe_cex_types::{Market, Position};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    task::Poll,
};

pub type AccountMarketOverlay = Rc<dyn Fn(Vec<Market>) -> Result<Vec<Market>, ExchangeError>>;

pub struct MarketChanges {
    pub added: Vec<Market>,
    pub removed: Vec<Market>,
}

pub(crate) struct PositionFetch {
    pub(crate) origin_disposed: Cell<bool>,
    // Other callers await only the shared fetch, not the originator's Store write.
    pub(crate) origin_error: RefCell<Option<ExchangeError>>,
    pub(crate) result: RefCell<Option<Result<Vec<Position>, ExchangeError>>>,
    pub(crate) wakers: crate::waiters::Waiters,
    pub(crate) task: RefCell<Option<Box<dyn ExchangeTask>>>,
}
impl PositionFetch {
    pub(crate) fn finish(&self, result: Result<Vec<Position>, ExchangeError>) {
        if self.result.borrow().is_some() {
            return;
        }
        *self.result.borrow_mut() = Some(result);
        self.wakers.wake_all();
    }
    pub(crate) fn wait(&self) -> crate::runtime::ExchangeFuture<'_, Vec<Position>> {
        let registration = self.wakers.register();
        Box::pin(std::future::poll_fn(move |context| {
            if let Some(result) = self.result.borrow().as_ref() {
                return Poll::Ready(result.clone());
            }
            registration.update(context.waker());
            Poll::Pending
        }))
    }
}

// Construct before spawn: even an unpolled host-discarded task must settle waiters.
pub(crate) struct PositionFetchGuard(pub(crate) Rc<PositionFetch>);
impl Drop for PositionFetchGuard {
    fn drop(&mut self) {
        self.0
            .finish(Err(ExchangeError::new("Position fetch task cancelled")));
    }
}

pub(crate) fn market_changes(old: &[Market], new: &[Market]) -> MarketChanges {
    let mut old_indexes = HashMap::new();
    let mut old_unique: Vec<Market> = Vec::new();
    for market in old {
        if let Some(index) = old_indexes.get(&market.id).copied() {
            old_unique[index] = market.clone();
        } else {
            old_indexes.insert(market.id.clone(), old_unique.len());
            old_unique.push(market.clone());
        }
    }
    let new_ids: HashSet<_> = new.iter().map(|market| &market.id).collect();
    MarketChanges {
        added: new
            .iter()
            .filter(|market| !old_indexes.contains_key(&market.id))
            .cloned()
            .collect(),
        removed: old_unique
            .into_iter()
            .filter(|market| !new_ids.contains(&market.id))
            .collect(),
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
