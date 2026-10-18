//! Private-stream health updates shared by Coinbase exchange handlers.
use crate::portfolios_balances::{PortfolioFailure, PortfolioThrown};
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use serde_json::{Map, Value};
use std::{cell::Cell, rc::Rc};

const PRIVATE_USER_STREAM_DEGRADED_MESSAGE: &str = "Coinbase private WS is not receiving live order updates. Orders will reconcile through REST; if your Coinbase key restricts IPs, whitelist both Tealstreet proxy IPs and your current browser IP.";
const PRIVATE_USER_STREAM_AUTH_DEGRADED_MESSAGE: &str = "Coinbase private WS authentication failed. REST trading can continue, but live order updates require your current browser IP to be whitelisted or a Coinbase WS proxy route.";

/// Per-exchange state used by the private user-stream health handlers.
#[derive(Debug, Default)]
pub struct CoinbasePrivateUserStreamHealth {
    last_message_at: Cell<f64>,
    has_warned_degraded: Cell<bool>,
}

#[cfg(test)]
#[path = "private_health_tests.rs"]
mod tests;

impl CoinbasePrivateUserStreamHealth {
    #[cfg(test)]
    pub(crate) fn last_message_at(&self) -> f64 {
        self.last_message_at.get()
    }

    #[cfg(test)]
    pub(crate) fn has_warned_degraded(&self) -> bool {
        self.has_warned_degraded.get()
    }

    // ts: CoinbaseExchange.markPrivateUserStreamAuthFailed
    pub fn mark_private_user_stream_auth_failed(
        &self,
        store: &dyn Store,
        message: Option<&str>,
        log: impl FnMut(&str, &str) -> Result<(), PortfolioFailure>,
        emit_warning: impl FnMut(&str) -> Result<(), PortfolioFailure>,
    ) -> Result<(), PortfolioFailure> {
        let detail = message
            .filter(|message| !message.is_empty())
            .map(|message| format!(" Coinbase message: {message}"))
            .unwrap_or_default();
        let reason = format!("{PRIVATE_USER_STREAM_AUTH_DEGRADED_MESSAGE}{detail}");
        self.mark_private_user_stream_degraded(store, Some(&reason), false, log, emit_warning)
    }

    // ts: CoinbaseExchange.markPrivateUserStreamMessageReceived
    pub fn mark_private_user_stream_message_received(
        &self,
        store: &dyn Store,
        now_ms: f64,
    ) -> Result<(), StoreMutationError> {
        self.last_message_at.set(now_ms);
        self.has_warned_degraded.set(false);

        // TypeScript spreads an own property with value `undefined`, which clears
        // the serialized field. JSON Value has no undefined, so compose the same
        // update with the shared Store read and replacement operations.
        let mut socket_health = store
            .socket_health()?
            .and_then(|current| current.as_object().cloned())
            .unwrap_or_default();
        socket_health.shift_remove("privateDegradedReason");
        store.update(
            StoreUpdate {
                socket_health: Some(Value::Object(socket_health)),
                ..StoreUpdate::default()
            },
            false,
        )
    }

    // ts: CoinbaseExchange.markPrivateUserStreamDegraded
    pub fn mark_private_user_stream_degraded(
        &self,
        store: &dyn Store,
        reason: Option<&str>,
        force_toast: bool,
        mut log: impl FnMut(&str, &str) -> Result<(), PortfolioFailure>,
        mut emit_warning: impl FnMut(&str) -> Result<(), PortfolioFailure>,
    ) -> Result<(), PortfolioFailure> {
        let reason = reason.unwrap_or(PRIVATE_USER_STREAM_DEGRADED_MESSAGE);
        let mut update = Map::new();
        update.insert(
            "privateDegradedReason".into(),
            Value::String(reason.to_owned()),
        );
        store
            .update_socket_health(&update)
            .map_err(|error| Rc::new(PortfolioThrown::from(error)))?;
        log(reason, "warning")?;
        if force_toast && !self.has_warned_degraded.get() {
            self.has_warned_degraded.set(true);
            emit_warning(reason)?;
        }
        Ok(())
    }
}
