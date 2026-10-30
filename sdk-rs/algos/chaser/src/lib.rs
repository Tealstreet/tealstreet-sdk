//! Server-side chaser orders, ported from the TS webworker chaser
//! (`packages/safe-cex/src/webworker/chaserOrder`): post-only orders that
//! follow the touch, re-placing on adverse moves until filled or stopped.

mod book;
mod chase;
mod client_id;

use std::rc::Rc;

use safe_cex_algos::{AlgoCx, AlgoFactory};

pub use book::{
    ChaserBook, KEPT_ORDER_STOP_REASON, MAX_CHASES_PER_ACCOUNT, Progress,
    UNCONFIRMED_ORDER_STOP_REASON,
};
pub use chase::{
    ActiveChaser, ChaserComplete, ChaserOptions, Terminate, TerminateKind, adjust, placement_filled,
};

/// The `chaser:*` ops for one account.
///
/// Wiring contract: build it with the ACCOUNT's context, never a socket's. Every
/// chase runs in a child of that scope, so disposing a socket scope would abort
/// chases other sockets and TWAP started and leave their orders live. On socket
/// close dispatch `chaser:terminateAll`; on account dispose await
/// [`ChaserBook::shutdown`] and only then dispose the account scope.
pub struct ChaserFactory {
    book: Rc<ChaserBook>,
}

impl ChaserFactory {
    pub fn new(account: AlgoCx) -> Self {
        Self {
            book: ChaserBook::new(account),
        }
    }

    /// The account's chases, shared with TWAP.
    pub fn book(&self) -> Rc<ChaserBook> {
        self.book.clone()
    }
}

impl AlgoFactory for ChaserFactory {
    fn ops(&self) -> &'static [&'static str] {
        &["chaser:start", "chaser:terminate", "chaser:terminateAll"]
    }

    fn handle(&self, op: &str, data: &str, _cx: &AlgoCx) -> Result<(), String> {
        let book = &self.book;
        match op {
            "chaser:start" => {
                let options: ChaserOptions = serde_json::from_str(data)
                    .map_err(|error| format!("Chase was not started: {error}"))?;
                book.start(options).map(|_| ())
            }
            "chaser:terminate" => {
                let id: String = serde_json::from_str(data).map_err(|error| error.to_string())?;
                book.remove(&id);
                Ok(())
            }
            "chaser:terminateAll" => {
                book.terminate_all("Client disconnected while chase was active");
                Ok(())
            }
            _ => Err(format!("unknown chaser op {op}")),
        }
    }
}
