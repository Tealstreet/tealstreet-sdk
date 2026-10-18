use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Context, Wake, Waker};

struct DialDrop(Rc<Cell<usize>>);
impl Drop for DialDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn pending_socket(runtime: &Rc<dyn ExchangeRuntime>, drops: Rc<Cell<usize>>) -> Rc<NativeWsSocket> {
    let guard = DialDrop(drops);
    native_ws_socket(
        runtime,
        Box::pin(async move {
            let _guard = guard;
            std::future::pending().await
        }),
    )
}

#[test]
fn native_connecting_owner_drop_cancels_handshake_with_live_event_waiter() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let runtime: Rc<dyn ExchangeRuntime> = native_runtime();
        for polled in [false,true] {
            let drops = Rc::new(Cell::new(0));
            let socket = pending_socket(&runtime,drops.clone());
            assert_eq!(socket.ready_state(),0,"CONNECTING native handle exists during pending dial");
            let mut event = socket.next_event();
            if polled { assert!(event.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending()); }
            drop(socket);
            assert_eq!(drops.get(),1,"CONNECTING final handle drop releases the actual pending dial before a live event waiter drops");
            assert!(event.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending(),"CONNECTING final drop suppresses lifecycle delivery");
        }
        let socket = native_ws_socket(&runtime, Box::pin(async {
            Err(safe_cex_transport::TransportError::Network(safe_cex_transport::NetworkFailure::from_code("failed attempt".into(),Some("ECONNREFUSED".into()))))
        }));
        let mut queued = socket.next_event();
        drop(socket);
        assert!(queued.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending(),
            "CONNECTING final handle drop suppresses queued failure events");
    });
}

#[test]
fn native_connecting_waiter_drop_releases_registration_without_canceling_dial() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let runtime: Rc<dyn ExchangeRuntime> = native_runtime();
        let drops = Rc::new(Cell::new(0));
        let socket = pending_socket(&runtime, drops.clone());
        let count = Arc::new(WakeCount(AtomicUsize::new(0)));
        let weak = Arc::downgrade(&count);
        let waker = Waker::from(count.clone());
        let mut event = socket.next_event();
        assert!(
            event
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        drop(event);
        drop(waker);
        assert_eq!(
            Arc::strong_count(&count),
            1,
            "CONNECTING event waiter drop releases its registered waker immediately"
        );
        drop(count);
        assert!(weak.upgrade().is_none());
        assert_eq!(
            drops.get(),
            0,
            "CONNECTING dropped waiter does not cancel the socket-owned handshake"
        );
        assert_eq!(
            socket.ready_state(),
            0,
            "CONNECTING native handle exists during pending dial"
        );
        let error = socket.send("pending").unwrap_err();
        assert_eq!(
            (error.name.as_str(), error.message.as_str()),
            ("InvalidStateError", "Sent before connected."),
            "CONNECTING send preserves source InvalidStateError"
        );
        let receiver = Arc::new(WakeCount(AtomicUsize::new(0)));
        let wake = Waker::from(receiver.clone());
        let mut waiting = socket.next_event();
        assert!(
            waiting
                .as_mut()
                .poll(&mut Context::from_waker(&wake))
                .is_pending()
        );
        socket.close();
        assert_eq!(
            receiver.0.load(Ordering::SeqCst),
            1,
            "CONNECTING close wakes the registered event receiver"
        );
        assert_eq!(
            drops.get(),
            1,
            "CONNECTING close releases the actual pending dial synchronously"
        );
        assert!(
            matches!(waiting.await.unwrap(),WsEvent::Error(message) if message == "error"),
            "CONNECTING close emits source error before close"
        );
        assert!(matches!(
            socket.next_event().await.unwrap(),
            WsEvent::Close {
                code: Some(1006),
                ..
            }
        ));
    });
}

thread_local! {
    static ON_WAKER_DROP: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
}
struct ReentrantWaker(Arc<AtomicUsize>);
impl Wake for ReentrantWaker {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl Drop for ReentrantWaker {
    fn drop(&mut self) {
        let callback = ON_WAKER_DROP.with(|slot| slot.borrow_mut().take());
        if let Some(callback) = callback {
            callback();
        }
    }
}
#[test]
fn native_connecting_waker_replacement_can_reenter_close() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let runtime: Rc<dyn ExchangeRuntime> = native_runtime();
        let drops = Rc::new(Cell::new(0));
        let socket = pending_socket(&runtime, drops.clone());
        let weak = Rc::downgrade(&socket);
        ON_WAKER_DROP.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || weak.upgrade().unwrap().close()))
        });
        let stale_wakes = Arc::new(AtomicUsize::new(0));
        let old = Waker::from(Arc::new(ReentrantWaker(stale_wakes.clone())));
        let mut event = socket.next_event();
        assert!(
            event
                .as_mut()
                .poll(&mut Context::from_waker(&old))
                .is_pending()
        );
        drop(old);
        // Registration replaces and drops the old waker outside its own borrow.
        // The socket must also release its state borrow before that callback.
        assert!(
            event
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(
            drops.get(),
            1,
            "CONNECTING waker destruction can close the same socket without a live state borrow"
        );
        assert_eq!(
            stale_wakes.load(Ordering::SeqCst),
            0,
            "CONNECTING replacement releases the old waker without waking it"
        );
        assert!(matches!(event.await.unwrap(), WsEvent::Error(_)));
        assert!(matches!(
            socket.next_event().await.unwrap(),
            WsEvent::Close {
                code: Some(1006),
                ..
            }
        ));
    });
}
