//! Diagnostic ingress throughput, with no venue decoding, network or signing.
//! Compare interleaved release binaries from the same master base; replay is
//! still required for venue parity and this is not a Rust/TS CPU certificate.
use safe_cex_engine::book::Book;
use safe_cex_engine::entropy::Entropy;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, Control, ControlPolicy, FeedId, FeedPolicy, Invalidation, SocketError,
    TrafficPolicy,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Response, SocketChannel, SocketId, Store, TimerId, Venue,
};
use std::{hint::black_box, time::Instant};

struct Probe {
    controls: ControlPolicy,
    received: usize,
}
impl Venue for Probe {
    type Pending = ();
    fn managed_sockets() -> bool {
        true
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let feed = cx
            .register_socket_feed(FeedPolicy {
                role: SocketChannel::Public,
                supervision: Config::default(),
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: self.controls,
                invalidation: Invalidation::None,
                subscription_interval_ms: 0.,
                close_floors: &[],
                data_silence: None,
            })
            .unwrap();
        cx.connect_socket(feed, "wss://profile.invalid").unwrap();
    }
    fn socket_control(&self, _: &Store, _: FeedId, _: bool, _: &str) -> Control {
        Control::Other
    }
    fn on_socket_event(
        &mut self,
        _: &mut Cx<'_, ()>,
        _: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), SocketError> {
        Ok(())
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {
        self.received += 1;
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn main() {
    let native = std::env::args().any(|arg| arg == "--native");
    let mut engine = Engine::new(
        Probe {
            controls: if native {
                ControlPolicy::Native
            } else {
                ControlPolicy::None
            },
            received: 0,
        },
        Some(Entropy::new(
            Box::new(|bytes| {
                bytes.fill(17);
                Ok(())
            }),
            Box::new(|_| Err("the diagnostic never signs".into())),
        )),
    )
    .unwrap();
    engine.start(0.);
    let socket = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    engine.handle(0., Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    let warmup = 100_000;
    let frames = std::env::args()
        .find_map(|arg| {
            arg.strip_prefix("--frames=")
                .map(|value| value.parse::<usize>().expect("positive frame count"))
        })
        .unwrap_or(2_000_000);
    assert!(frames > 0);
    let mut run = |count| {
        for _ in 0..count {
            black_box(&mut engine).handle(
                black_box(1.),
                Event::WsMessage {
                    socket,
                    text: black_box("tick"),
                },
            );
            engine.drain_actions().for_each(drop);
        }
    };
    run(warmup);
    let start = Instant::now();
    run(frames);
    let elapsed = start.elapsed().as_nanos();
    assert_eq!(engine.venue().received, warmup + frames);
    println!(
        "{}",
        serde_json::json!({
            "native": native, "frames": frames, "received": engine.venue().received,
            "elapsedNs": elapsed, "nsPerFrame": elapsed as f64 / frames as f64,
        })
    );
}
