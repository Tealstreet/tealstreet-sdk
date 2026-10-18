use super::*;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_venue_binance::public_classes::{PublicKind, PublicSocket};

type SendHook = Rc<dyn Fn() -> Result<(), ExchangeError>>;
struct SubscriptionSocket {
    events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
    sent: RefCell<Vec<String>>,
    hook: RefCell<Option<SendHook>>,
}
impl WsSocket for SubscriptionSocket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |e| Poll::Ready(Ok(e)))
        }))
    }
    fn send(&self, text: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(text.into());
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook()?;
        }
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
struct NoSubscriptionHttp;
impl safe_cex_transport::RestAdapter for NoSubscriptionHttp {
    fn execute(
        &self,
        _: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        panic!("subscription encoding must not request HTTP")
    }
}

#[test]
fn production_native_subscription_frames_order_counts_reentry_and_send_failure() {
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::FuturesPublic,
        PublicKind::PMMarket,
        PublicKind::PMPublic,
        PublicKind::SpotPublic,
    ] {
        let runtime = Rc::new(Runtime::default());
        let socket = Rc::new(SubscriptionSocket {
            events: Rc::new(RefCell::new(std::collections::VecDeque::from([
                WsEvent::Open,
            ]))),
            sent: RefCell::default(),
            hook: RefCell::default(),
        });
        *runtime.dial_socket.borrow_mut() = Some(socket.clone());
        let venue = match kind {
            PublicKind::FuturesMarket | PublicKind::FuturesPublic => Venue::Futures,
            PublicKind::PMMarket | PublicKind::PMPublic => Venue::PM,
            PublicKind::SpotPublic => Venue::Spot,
        };
        let name = match venue {
            Venue::Futures => "binance",
            Venue::PM => "binancepm",
            Venue::Spot => "binancespot",
        };
        let deps = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(),
            std::sync::Arc::new(NoSubscriptionHttp),
            Rc::new(|_, _| panic!("unexpected external data")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let owner = ExchangeOwner::new(
            venue,
            serde_json::from_value(json!(name)).unwrap(),
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            deps,
        )
        .unwrap();
        let public = owner.public_socket();
        let target: Rc<PublicSocket> =
            if matches!(kind, PublicKind::FuturesMarket | PublicKind::PMMarket) {
                public.market_websocket.get().unwrap().clone()
            } else {
                public
            };
        assert_eq!(target.kind, kind);
        target.base().connect_and_subscribe(false).unwrap();
        for _ in 0..12 {
            runtime.pump();
        }
        assert!(
            !socket.sent.borrow().is_empty(),
            "actual onOpen reached the recording host {kind:?}"
        );
        socket.sent.borrow_mut().clear();
        *target.listeners.topics.borrow_mut() = [
            ("10", 1.0),
            ("2", 0.5),
            ("zero", 0.0),
            ("negative", -1.0),
            ("nan", f64::NAN),
            ("positiveInfinity", f64::INFINITY),
            ("negativeInfinity", f64::NEG_INFINITY),
            ("later", 0.0),
            ("last", 2.0),
            ("quote\"\n雪", 1.0),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        let weak = Rc::downgrade(&target);
        *socket.hook.borrow_mut() = Some(Rc::new(move || {
            let target = weak.upgrade().unwrap();
            let mut topics = target.listeners.topics.borrow_mut();
            topics.insert("later".into(), 1.0);
            topics.remove("last");
            topics.insert("appended".into(), 1.0);
            Ok(())
        }));
        target.subscribe().unwrap();
        let expected = ["2", "10", "later", "quote\"\n雪"]
            .into_iter()
            .enumerate()
            .map(|(i, topic)| {
                format!(
                    "{{\"method\":\"SUBSCRIBE\",\"params\":[{}],\"id\":{}}}",
                    serde_json::Value::String(topic.into()),
                    i + 10
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            *socket.sent.borrow(),
            expected,
            "snapshot keys, fresh finite counts and exact outgoing bytes {kind:?}"
        );
        socket.sent.borrow_mut().clear();
        *socket.hook.borrow_mut() = Some(Rc::new(|| {
            let mut e = ExchangeError::new("subscription host send failure");
            e.name = "SubscriptionSendError".into();
            Err(e)
        }));
        let error = target.subscribe().unwrap_err();
        assert_eq!(error.name, "SubscriptionSendError");
        assert_eq!(error.message, "subscription host send failure");
        assert_eq!(
            *socket.sent.borrow(),
            vec![expected[0].clone()],
            "send failure aborts remaining keys {kind:?}"
        );
        socket.sent.borrow_mut().clear();
        *target.listeners.topics.borrow_mut() = Default::default();
        target.subscribe().unwrap();
        assert!(
            socket.sent.borrow().is_empty(),
            "empty snapshot sends no frame {kind:?}"
        );
    }
}
