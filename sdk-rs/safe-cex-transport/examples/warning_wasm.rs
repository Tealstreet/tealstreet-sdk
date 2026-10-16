#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn probe_tunnel_endpoints() -> String {
    use safe_cex_transport::{routing::HowEgress, tunnel_endpoints::tunnel_egress_url};
    serde_json::to_string(&[
        tunnel_egress_url(HowEgress::Jp),
        tunnel_egress_url(HowEgress::Sg),
    ])
    .unwrap()
}

use safe_cex_transport::{DirectTransport, RoutingOptions, TransportProvider};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn probe_warning(exchange: &str, enabled: bool) -> String {
    let exchange = serde_json::from_value(serde_json::json!(exchange)).unwrap();
    let route = DirectTransport.ws_routing(
        &exchange,
        &RoutingOptions {
            tealstreet_ws_route: enabled,
            ..Default::default()
        },
    );
    serde_json::to_string(&route).unwrap()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn probe_relay_overlay() -> String {
    use safe_cex_transport::{
        TransportError, TransportFuture,
        relay::{RelayWebSocket, UpgradeHeaders, WebSocketConnectionOptions},
        socket::{
            SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
            TunnelSocket, TunnelTask,
        },
    };
    use std::{
        cell::RefCell,
        future::Future,
        pin::{Pin, pin},
        rc::Rc,
        task::{Context, Waker},
    };
    struct Socket;
    impl TunnelSocket for Socket {
        fn send(&self, _: SocketFrame) -> Result<(), TransportError> {
            Ok(())
        }
        fn receive(&self) -> TransportFuture<'static, SocketFrame> {
            Box::pin(std::future::pending())
        }
        fn close(&self, _: Option<u16>, _: &str) -> Result<(), TransportError> {
            Ok(())
        }
    }
    #[derive(Default)]
    struct Host(RefCell<Vec<TunnelDialOptions>>);
    impl TunnelHost for Host {
        fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
            unreachable!()
        }
        fn capabilities(&self) -> SocketCapabilities {
            SocketCapabilities {
                runtime: SocketRuntime::Native,
                upgrade_headers: true,
                alpn: true,
            }
        }
        fn now_ms(&self) -> u64 {
            0
        }
        fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
            Box::pin(std::future::pending())
        }
        fn connect(
            &self,
            _: String,
            options: TunnelDialOptions,
        ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
            self.0.borrow_mut().push(options);
            Box::pin(async { Ok(Rc::new(Socket) as Rc<dyn TunnelSocket>) })
        }
        fn proxy_token(&self) -> TransportFuture<'_, String> {
            Box::pin(async { Ok("token".into()) })
        }
        fn is_token_expired(&self, _: &str) -> bool {
            false
        }
        fn sync_server_time(&self, _: u64) {}
    }
    let host = Rc::new(Host::default());
    let url = "wss://tunnel.example/tunnel?ts-relay=wss%3A%2F%2Fvenue.example";
    let mut options = WebSocketConnectionOptions {
        headers: Some(UpgradeHeaders::Fresh(Rc::new(|| {
            panic!("wasm must not evaluate node auth provider")
        }))),
        headers_required: true,
        protocols: vec!["private-v1".into()],
        ..Default::default()
    };
    let required_rejected =
        RelayWebSocket::with_connection_options(host.clone(), url, options.clone()).is_err();
    assert!(host.0.borrow().is_empty());
    options.headers_required = false;
    let relay = RelayWebSocket::with_connection_options(host.clone(), url, options).unwrap();
    let mut future = pin!(relay.on_inner_open());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    let dials = host.0.borrow();
    assert_eq!(dials.len(), 1);
    assert!(!dials[0].headers().contains_key("Authorization"));
    assert!(dials[0].protocols().is_empty());
    serde_json::json!({"requiredRejected": required_rejected, "dials":dials.len(), "protocols":dials[0].protocols()}).to_string()
}
