//! Journal-only correlation for the remote signing boundary.
use safe_cex_remote_signer::{Outbox, SignerLink};
use std::sync::Arc;

pub(crate) fn link(socket: u64, outbox: Outbox) -> Arc<SignerLink> {
    SignerLink::new(socket, outbox)
}
