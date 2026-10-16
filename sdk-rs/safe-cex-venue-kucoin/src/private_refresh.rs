use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
};

pub type RefreshFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + 'a>>;

// Handle clones retain primitive value or opaque reference identity.
// Truthiness is pure; snapshot invocation retains the ignored-promise boundary.
pub fn refresh_snapshot_soon<'a, H: Clone + 'a, E: 'a>(
    timer: &'a RefCell<Option<H>>,
    disposed: &'a Cell<bool>,
    is_truthy: impl FnOnce(&H) -> bool,
    clear_timeout: impl FnOnce(H) -> Result<(), E>,
    set_timeout: impl FnOnce(u32, Box<dyn FnOnce() -> Result<(), E> + 'a>) -> Result<H, E>,
    invoke_snapshot: impl FnOnce() -> Result<(), E> + 'a,
) -> Result<(), E> {
    let previous = timer.borrow().clone();
    if let Some(previous) = previous.filter(is_truthy) {
        clear_timeout(previous)?;
    }
    let callback = Box::new(move || {
        *timer.borrow_mut() = None;
        if !disposed.get() {
            invoke_snapshot()?;
        }
        Ok(())
    });
    let handle = set_timeout(500, callback)?;
    *timer.borrow_mut() = Some(handle);
    Ok(())
}

// Fetch capabilities invoke now and convert sync throws into rejected futures.
pub fn refresh_orders_soon<'a, H: Clone + 'a, E: 'a>(
    timer: &'a RefCell<Option<H>>,
    disposed: &'a Cell<bool>,
    is_truthy: impl FnOnce(&H) -> bool,
    set_timeout: impl FnOnce(u32, Box<dyn FnOnce() -> RefreshFuture<'a, (), E> + 'a>) -> Result<H, E>,
    fetch_orders: impl FnOnce() -> RefreshFuture<'a, (), E> + 'a,
) -> Result<(), E> {
    if timer.borrow().as_ref().is_some_and(is_truthy) || disposed.get() {
        return Ok(());
    }
    let callback = Box::new(move || -> RefreshFuture<'a, (), E> {
        *timer.borrow_mut() = None;
        if disposed.get() {
            return Box::pin(async { Ok(()) });
        }
        let pending = fetch_orders();
        Box::pin(async move {
            pending.await?;
            Ok(())
        })
    });
    let handle = set_timeout(250, callback)?;
    *timer.borrow_mut() = Some(handle);
    Ok(())
}

// Update capability owns the complete store.update/current-loaded expression.
pub fn refresh_balance_soon<'a, H: Clone + 'a, B: 'a, E: 'a>(
    timer: &'a RefCell<Option<H>>,
    disposed: &'a Cell<bool>,
    is_truthy: impl FnOnce(&H) -> bool,
    set_timeout: impl FnOnce(u32, Box<dyn FnOnce() -> RefreshFuture<'a, (), E> + 'a>) -> Result<H, E>,
    fetch_balance: impl FnOnce() -> RefreshFuture<'a, B, E> + 'a,
    update_balance_and_current_loaded: impl FnOnce(B) -> Result<(), E> + 'a,
) -> Result<(), E> {
    if timer.borrow().as_ref().is_some_and(is_truthy) || disposed.get() {
        return Ok(());
    }
    let callback = Box::new(move || -> RefreshFuture<'a, (), E> {
        *timer.borrow_mut() = None;
        if disposed.get() {
            return Box::pin(async { Ok(()) });
        }
        let pending = fetch_balance();
        Box::pin(async move {
            let balance = pending.await?;
            if disposed.get() {
                return Ok(());
            }
            update_balance_and_current_loaded(balance)?;
            Ok(())
        })
    });
    let handle = set_timeout(250, callback)?;
    *timer.borrow_mut() = Some(handle);
    Ok(())
}
