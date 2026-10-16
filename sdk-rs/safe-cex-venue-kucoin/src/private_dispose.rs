use std::cell::RefCell;

/// Handle cloning must preserve primitive value or opaque reference identity.
pub fn dispose_futures_private<H: Clone, E>(
    timer: &RefCell<Option<H>>,
    is_truthy: impl FnOnce(&H) -> bool,
    clear_timeout: impl FnOnce(H) -> Result<(), E>,
    super_dispose: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    let handle = timer.borrow().clone();
    if let Some(handle) = handle
        && is_truthy(&handle)
    {
        clear_timeout(handle)?;
        *timer.borrow_mut() = None;
    }
    super_dispose()?;
    Ok(())
}

/// Handle cloning must preserve primitive value or opaque reference identity.
pub fn dispose_spot_private<H: Clone, E>(
    orders_timer: &RefCell<Option<H>>,
    balance_timer: &RefCell<Option<H>>,
    is_truthy: impl Fn(&H) -> bool,
    mut clear_timeout: impl FnMut(H) -> Result<(), E>,
    super_dispose: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    let orders_handle = orders_timer.borrow().clone();
    if let Some(handle) = orders_handle
        && is_truthy(&handle)
    {
        clear_timeout(handle)?;
        *orders_timer.borrow_mut() = None;
    }
    let balance_handle = balance_timer.borrow().clone();
    if let Some(handle) = balance_handle
        && is_truthy(&handle)
    {
        clear_timeout(handle)?;
        *balance_timer.borrow_mut() = None;
    }
    super_dispose()?;
    Ok(())
}
