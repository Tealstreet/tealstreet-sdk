//! Actual exchange constructor, private state and Spot Auth; terminal IO only.
use super::*;
use safe_cex_exchange::websocket::WsEvent;

#[test]
fn native_private_owner_keeps_family_receiver_defaults_and_store_route() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let state: Rc<PrivateState> = std::ops::Deref::deref(owner.private.as_ref()).clone();
        let expected_parent: Rc<dyn PrivateParent> = owner.state.clone();
        assert!(
            Rc::ptr_eq(&state.parent, &expected_parent),
            "canonical private state retains the real exchange receiver"
        );
        assert_eq!(
            state.venue, venue,
            "actual constructor selects the source private family"
        );
        assert_eq!(
            state.position.throttle_ms.get().to_bits(),
            if venue == Venue::PM {
                500.0f64
            } else {
                10000.0f64
            }
            .to_bits()
        );
        assert!(!state.connect.connecting.get());
        assert_eq!(
            state.connect.retry_delay.get().to_bits(),
            5000.0f64.to_bits()
        );
        assert!(!state.has_opened.get());
        assert!(matches!(
            *state.refresh_timer.borrow(),
            EventValue::Undefined
        ));
        assert!(matches!(
            *state.orders_timer.borrow(),
            EventValue::Undefined
        ));
        owner
            .private
            .on_message(string(native_order_frame(venue, false).to_string()))
            .unwrap();
        let rows = owner.base.store.orders();
        assert_eq!(
            rows.len(),
            1,
            "canonical state routes the actual native frame to Store"
        );
        assert_eq!(
            rows[0].id,
            if venue == Venue::Futures {
                "42"
            } else {
                "native-client"
            }
        );
        assert_eq!(rows[0].amount.to_bits(), 3.0f64.to_bits());
        assert_eq!(rows[0].price.to_number().to_bits(), 91.2f64.to_bits());
        drop(rows);
        owner.dispose().unwrap();
        assert!(
            owner.private.base.disposed(),
            "live imported Base disposal remains visible through retained state"
        );
    }
}

struct AuthSocket {
    events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
    sent: RefCell<Vec<String>>,
}
impl WsSocket for AuthSocket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |event| Poll::Ready(Ok(event)))
        }))
    }
    fn send(&self, text: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(text.into());
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
struct NoAuthHttp;
impl safe_cex_transport::RestAdapter for NoAuthHttp {
    fn execute(
        &self,
        _: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        panic!("Spot private authentication must not fetch a listen key")
    }
}
#[test]
fn native_spot_auth_uses_actual_parent_clock_secret_and_socket() {
    for (key, secret) in [
        ("native-key-A", "native-secret-A"),
        ("native-key-B", "native-secret-B"),
    ] {
        let runtime = Rc::new(Runtime::default());
        let socket = Rc::new(AuthSocket {
            events: Rc::new(RefCell::new(std::collections::VecDeque::from([
                WsEvent::Open,
            ]))),
            sent: RefCell::default(),
        });
        *runtime.dial_socket.borrow_mut() = Some(socket.clone());
        let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(),
            std::sync::Arc::new(NoAuthHttp),
            Rc::new(|_, _| panic!("no external provider")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let owner = ExchangeOwner::new(
            Venue::Spot,
            safe_cex_types::ExchangeName::Binancespot,
            serde_json::from_value(json!({"key":key,"secret":secret})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            dependencies,
        )
        .unwrap();
        run(&runtime, owner.private.connect_and_subscribe()).unwrap();
        for _ in 0..16 {
            runtime.pump();
        }
        let sent = socket.sent.borrow();
        let frames: Vec<Value> = sent
            .iter()
            .map(|text| serde_json::from_str(text).unwrap())
            .collect();
        let auth: Vec<&Value> = frames
            .iter()
            .filter(|frame| frame["method"] == "userDataStream.subscribe.signature")
            .collect();
        assert_eq!(auth.len(), 1, "actual private onOpen executes Auth once");
        let frame = auth[0];
        assert_eq!(frame["id"], "tealstreet-user-data");
        let params = &frame["params"];
        assert_eq!(
            params["apiKey"], key,
            "Auth observes the original production parent's key"
        );
        assert_eq!(params["recvWindow"], 5000);
        let timestamp = params["timestamp"]
            .as_f64()
            .expect("native virtual clock number");
        assert!(
            timestamp.is_finite() && timestamp > 1_000_000_000_000.0,
            "Auth forwards the actual epoch clock"
        );
        // The primitive HMAC is independently source-tested by api_auth/spot_protocol.
        // This checks the adapter's actual parent/clock/payload wiring at its socket output.
        let payload = format!(
            "apiKey={key}&recvWindow=5000&timestamp={}",
            safe_cex_num::js_number_string(timestamp)
        );
        let expected = safe_cex_crypto::hmac_sha256(
            secret.as_bytes(),
            payload.as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        );
        assert_eq!(
            params["signature"], expected,
            "Auth forwards the real secret and source payload before native socket send"
        );
        drop(sent);
        assert!(runtime.reported.borrow().is_empty());
        owner.dispose().unwrap();
    }
}
