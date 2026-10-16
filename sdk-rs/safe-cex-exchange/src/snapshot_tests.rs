use super::*;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};

fn market(id: &str, symbol: &str) -> Market {
    serde_json::from_value(serde_json::json!({
        "id":id, "symbol":symbol, "base":"BTC", "quote":"USDT", "active":true,
        "precision":{"amount":0.001, "price":0.1},
        "limits":{"amount":{"min":0.001, "max":100}, "leverage":{"min":1, "max":100}}
    }))
    .unwrap()
}

#[test]
fn refresh_comparison_is_ids_only_and_preserves_map_order_and_last_duplicate() {
    let old = vec![
        market("a", "old-a"),
        market("b", "old-b"),
        market("a", "last-a"),
    ];
    let changes = market_changes(&old, &[market("a", "changed-fields"), market("c", "new-c")]);
    assert_eq!(
        changes
            .added
            .iter()
            .map(|market| market.id.as_str())
            .collect::<Vec<_>>(),
        ["c"]
    );
    assert_eq!(
        changes
            .removed
            .iter()
            .map(|market| market.id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    let changes = market_changes(&old, &[]);
    assert_eq!(
        changes
            .removed
            .iter()
            .map(|market| market.symbol.as_str())
            .collect::<Vec<_>>(),
        ["last-a", "old-b"]
    );
    let changes = market_changes(&[market("a", "old")], &[market("a", "changed")]);
    assert!(changes.added.is_empty() && changes.removed.is_empty());
}

struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn shared_fetch_wakes_each_waiter_and_retains_rejected_result() {
    let fetch = PositionFetch {
        origin_disposed: Cell::new(false),
        origin_error: RefCell::new(None),
        result: RefCell::new(None),
        wakers: crate::waiters::Waiters::default(),
        task: RefCell::new(None),
    };
    let first = Arc::new(Counter(AtomicUsize::new(0)));
    let second = Arc::new(Counter(AtomicUsize::new(0)));
    let first_waker = Waker::from(first.clone());
    let second_waker = Waker::from(second.clone());
    let mut first_context = Context::from_waker(&first_waker);
    let mut second_context = Context::from_waker(&second_waker);
    let mut first_wait = Box::pin(fetch.wait());
    let mut second_wait = Box::pin(fetch.wait());
    assert!(first_wait.as_mut().poll(&mut first_context).is_pending());
    assert!(first_wait.as_mut().poll(&mut first_context).is_pending());
    assert!(second_wait.as_mut().poll(&mut second_context).is_pending());
    fetch.finish(Err(ExchangeError::new("venue rejected")));
    assert_eq!(first.0.load(Ordering::SeqCst), 1);
    assert_eq!(second.0.load(Ordering::SeqCst), 1);
    for result in [
        first_wait.as_mut().poll(&mut first_context),
        second_wait.as_mut().poll(&mut second_context),
    ] {
        assert!(matches!(result, Poll::Ready(Err(error)) if error.message == "venue rejected"));
    }
    let mut later_wait = Box::pin(fetch.wait());
    assert!(
        matches!(later_wait.as_mut().poll(&mut first_context), Poll::Ready(Err(error)) if error.message == "venue rejected")
    );
}

#[test]
fn dropped_position_waiter_releases_waker_while_shared_fetch_stays_pending() {
    let fetch = PositionFetch {
        origin_disposed: Cell::new(false),
        origin_error: RefCell::new(None),
        result: RefCell::new(None),
        wakers: crate::waiters::Waiters::default(),
        task: RefCell::new(None),
    };
    let sentinel = Arc::new(Counter(AtomicUsize::new(0)));
    let weak = Arc::downgrade(&sentinel);
    let waker = Waker::from(sentinel);
    let mut waiter = fetch.wait();
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(waker);
    drop(waiter);
    assert!(
        weak.upgrade().is_none(),
        "position waiter drop must release its waker before task settlement"
    );
    assert!(
        fetch.result.borrow().is_none(),
        "caller cancellation must not settle shared position work"
    );
    let survivor = Arc::new(Counter(AtomicUsize::new(0)));
    let waker = Waker::from(survivor.clone());
    let mut waiter = fetch.wait();
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    fetch.finish(Ok(vec![]));
    assert_eq!(survivor.0.load(Ordering::SeqCst), 1);
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_ready()
    );
}
