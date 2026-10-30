//! The host's socket tracker sees a venue's private socket open and take its
//! first frame, which the hosted server reports as `privateInitialized`.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_engine::book::Book;
use safe_cex_engine::{
    Command, Cx, HttpRequest, Response, SocketChannel, SocketId, SocketSpec, TimerId, Venue,
};
use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport, now_ms};
use std::time::Duration;
use tokio::sync::mpsc;

/// Opens every socket at once and delivers one frame on it.
struct Opens;
impl Transport for Opens {
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsOpened(socket));
        let _ = events.send(HostEvent::WsMessage(
            socket,
            r#"{"op":"auth","success":true}"#.into(),
        ));
        mpsc::unbounded_channel().0
    }
    fn request(&self, _: HttpRequest, _: Events) {}
}

/// Connects one private-role socket at start, as account feeds do.
struct PrivateFeed(Book);
impl Venue for PrivateFeed {
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::BASE
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.connect(SocketSpec {
            channel: Some(SocketChannel::Private),
            ..SocketSpec::new("wss://private.example")
        });
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        Some(&self.0)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_private_socket_that_opens_and_takes_a_frame_reports_initialized() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let engine = safe_cex_engine::test_support::seeded_engine(PrivateFeed(Book::default()));
            let (handle, _task) =
                safe_cex_native::spawn_account(Host::new(engine, Opens), ServerConfig::default())
                    .unwrap();
            let mut summary = handle.socket_health().summary(now_ms() as u64);
            for _ in 0..50 {
                if summary.private_initialized {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
                summary = handle.socket_health().summary(now_ms() as u64);
            }
            assert!(
                summary.private_initialized,
                "{summary:?} {:?}",
                handle.socket_health()
            );
            assert!(summary.private_healthy);
            handle.dispose();
        })
        .await;
}
