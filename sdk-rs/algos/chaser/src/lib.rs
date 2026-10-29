//! Server-side chaser orders, ported from the TS webworker chaser
//! (`packages/safe-cex/src/webworker/chaserOrder`): post-only orders that
//! follow the touch, re-placing on adverse moves until filled or stopped.
#![cfg(not(target_arch = "wasm32"))]

mod book;
mod chase;

use std::cell::RefCell;
use std::rc::Rc;

use safe_cex_algos::{AlgoCx, AlgoFactory};

pub use book::{ChaserBook, KEPT_ORDER_STOP_REASON, Progress, UNCONFIRMED_ORDER_STOP_REASON};
pub use chase::{
    ActiveChaser, ChaserComplete, ChaserOptions, Terminate, TerminateKind, adjust, placement_filled,
};

/// The `chaser:*` ops for one account. The first op binds the account's book;
/// TWAP reaches the same book through [`ChaserFactory::book`].
#[derive(Default)]
pub struct ChaserFactory {
    book: RefCell<Option<Rc<ChaserBook>>>,
}

impl ChaserFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn book(&self, cx: &AlgoCx) -> Rc<ChaserBook> {
        self.book
            .borrow_mut()
            .get_or_insert_with(|| ChaserBook::new(cx.clone()))
            .clone()
    }
}

impl AlgoFactory for ChaserFactory {
    fn ops(&self) -> &'static [&'static str] {
        &["chaser:start", "chaser:terminate", "chaser:terminateAll"]
    }

    fn handle(&self, op: &str, data: &str, cx: &AlgoCx) -> Result<(), String> {
        let book = self.book(cx);
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
