//! Bitrue's per-symbol position metadata cache and source fetch boundary.
use crate::account_helpers::{AccountPositionSnapshot, get_account_position_snapshots};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_number, numeric_ingress::js_value_as_f64};
use safe_cex_types::{MarginType, PositionMode, PositionSide};
use serde_json::Value;
use std::{cell::RefCell, future::Future, rc::Rc};

#[derive(Debug, Clone, PartialEq)]
pub struct PositionMetadata {
    pub symbol: String,
    pub side: PositionSide,
    pub leverage: f64,
    pub position_mode: PositionMode,
    pub margin_type: MarginType,
}

pub type PositionMetadataHandle = Rc<RefCell<PositionMetadata>>;

/// Entries retain Map insertion order; replacing a symbol retains its slot.
/// The caller supplies the actual account-fetch hook and post-await lifecycle
/// view. No borrow of the cache is held over that hook's await.
pub fn fetch_position_metadata<'a, F, Fut, D>(
    cache: &'a RefCell<Vec<(String, PositionMetadataHandle)>>,
    symbols: &'a [String],
    supplied: Option<&[AccountPositionSnapshot<'_>]>,
    fetch_account: F,
    is_disposed: D,
) -> impl Future<Output = Result<Vec<PositionMetadataHandle>, ExchangeError>> + 'a
where
    F: FnOnce() -> Result<Fut, ExchangeError> + 'a,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>> + 'a,
    D: Fn() -> bool + 'a,
{
    let missing = supplied.is_none()
        && symbols
            .iter()
            .any(|symbol| !cache.borrow().iter().any(|(key, _)| key == symbol));
    // Without an await, source cache mutation and disposal/read happen at entry.
    // Keep errors in the returned future, as source async arrows reject a Promise.
    let entry = if missing {
        Err(fetch_account())
    } else {
        Ok(finish_metadata(
            cache,
            symbols,
            supplied.map(<[_]>::to_vec),
            &is_disposed,
        ))
    };
    async move {
        match entry {
            Ok(result) => result,
            Err(request) => {
                let fetched = request?.await?;
                let current =
                    get_account_position_snapshots(fetched.as_ref()).map_err(coercion_error)?;
                finish_metadata(cache, symbols, current, &is_disposed)
            }
        }
    }
}

fn finish_metadata(
    cache: &RefCell<Vec<(String, PositionMetadataHandle)>>,
    symbols: &[String],
    current: Option<Vec<AccountPositionSnapshot<'_>>>,
    is_disposed: &impl Fn() -> bool,
) -> Result<Vec<PositionMetadataHandle>, ExchangeError> {
    if let Some(current) = current {
        let mut next: Vec<(String, PositionMetadataHandle)> = Vec::new();
        for position in current {
            let leverage = js_number(Some(position.leverage_level)).map_err(coercion_error)?;
            if position.symbol.is_empty() || !leverage.is_finite() || leverage <= 0.0 {
                continue;
            }
            let metadata = PositionMetadata {
                symbol: position.symbol.to_owned(),
                side: position.side,
                leverage,
                position_mode: PositionMode::OneWay,
                margin_type: if position.position_type.is_number()
                    && js_value_as_f64(position.position_type) == Some(2.0)
                {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                },
            };
            if let Some((_, existing)) = next.iter_mut().find(|(key, _)| key == &metadata.symbol) {
                *existing = Rc::new(RefCell::new(metadata));
            } else {
                next.push((metadata.symbol.clone(), Rc::new(RefCell::new(metadata))));
            }
        }
        *cache.borrow_mut() = next;
    }
    if is_disposed() {
        return Ok(Vec::new());
    }
    Ok(symbols
        .iter()
        .filter_map(|symbol| {
            cache
                .borrow()
                .iter()
                .find(|(key, _)| key == symbol)
                .map(|(_, entry)| entry.clone())
        })
        .collect())
}

fn coercion_error(error: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut output = ExchangeError::new(error.message);
    output.name = error.name.into();
    output
}
