#![cfg(all(not(target_arch = "wasm32"), feature = "address-nonce"))]
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::*;
use safe_cex_engine::*;
use safe_cex_native::{Events, Host, Transport};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

struct Probe;
impl Venue for Probe {
    type Pending = ();
    fn public_address_slots(&self) -> AddressSetupSlots {
        AddressSetupSlots::Primary
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.store.set_hedged(true);
        cx.notice(Level::Info, "held_setup", "native observer control");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
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
struct NoTransport;
impl Transport for NoTransport {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("setup must not open a venue socket")
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("setup must not send HTTP")
    }
}
#[derive(Default)]
struct State {
    started: AtomicUsize,
    cancelled: AtomicUsize,
    polls: AtomicUsize,
    dropped: AtomicUsize,
    joined: AtomicBool,
}
struct Provider {
    state: Arc<State>,
    cleanup: Mutex<Option<oneshot::Receiver<()>>>,
}
struct Execution {
    state: Arc<State>,
    cleanup: oneshot::Receiver<()>,
}
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("setup must use the owned factory")
    }
    fn owned_public_address_capability(&self) -> Capability {
        Capability::Supported
    }
    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        assert_eq!(
            request.operations,
            vec![Operation::PublicAddress {
                key: KeyRef("signing".into())
            }]
        );
        Some(Box::new(Execution {
            state: self.state.clone(),
            cleanup: self.cleanup.lock().unwrap().take()?,
        }))
    }
}
impl OwnedSetupExecution for Execution {
    fn start(&mut self) -> Result<(), SignError> {
        self.state.started.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.state.cancelled.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn poll_result(&mut self, _: &mut Context<'_>) -> Poll<Result<SignResult, SignError>> {
        Poll::Pending
    }
    fn poll_joined(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SignError>> {
        self.state.polls.fetch_add(1, Ordering::SeqCst);
        match Pin::new(&mut self.cleanup).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                self.state.joined.store(true, Ordering::SeqCst);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Err(SignError::Unavailable)),
        }
    }
}
impl Drop for Execution {
    fn drop(&mut self) {
        self.state.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
fn fixture() -> (Host<Probe, NoTransport>, Arc<State>, oneshot::Sender<()>) {
    let state = Arc::new(State::default());
    let (release, cleanup) = oneshot::channel();
    let provider = Provider {
        state: state.clone(),
        cleanup: Mutex::new(Some(cleanup)),
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe);
    engine
        .set_identities(Identities {
            primary: Some(Identity::new(
                KeyRef("credential".into()),
                Some(KeyRef("signing".into())),
            )),
            secondary: None,
        })
        .unwrap();
    engine.set_signer(Some(Arc::new(provider))).unwrap();
    assert_eq!(
        engine.initialize_address_nonce(100.).unwrap(),
        AddressSetupProgress::Pending
    );
    assert_eq!(state.started.load(Ordering::SeqCst), 1);
    (Host::new(engine, NoTransport), state, release)
}
async fn wait_for_retirement(state: &State) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.cancelled.load(Ordering::SeqCst) == 0 || state.polls.load(Ordering::SeqCst) < 2
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual cleanup must be retained and polled");
    assert_eq!(state.dropped.load(Ordering::SeqCst), 0);
    assert!(!state.joined.load(Ordering::SeqCst));
}
#[tokio::test]
async fn throwing_notice_and_frame_observers_propagate_only_after_actual_setup_join() {
    for frame in [false, true] {
        let (host, state, release) = fixture();
        let (_commands, inbox) = mpsc::unbounded_channel();
        let run = tokio::spawn(host.run(
            inbox,
            Duration::from_millis(1),
            move |_| {
                if frame {
                    panic!("frame observer")
                }
            },
            move |_| {
                if !frame {
                    panic!("notice observer")
                }
            },
        ));
        wait_for_retirement(&state).await;
        assert!(
            !run.is_finished(),
            "panic must wait for the held actual cleanup"
        );
        release.send(()).unwrap();
        match run.await {
            Err(error) => assert!(error.is_panic()),
            Ok(_) => panic!("observer panic was swallowed"),
        }
        assert!(state.joined.load(Ordering::SeqCst));
        assert_eq!(state.dropped.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn cancelling_a_polled_run_retains_actual_setup_until_its_cleanup_finishes() {
    let (host, state, release) = fixture();
    let (_commands, inbox) = mpsc::unbounded_channel();
    let run = tokio::spawn(host.run(inbox, Duration::from_millis(1), |_| {}, |_| {}));
    tokio::task::yield_now().await;
    run.abort();
    assert!(run.await.err().unwrap().is_cancelled());
    wait_for_retirement(&state).await;
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.dropped.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(state.joined.load(Ordering::SeqCst));
}
#[tokio::test]
async fn dropping_an_unpolled_run_retains_the_already_started_setup_owner() {
    let (host, state, release) = fixture();
    let (_commands, inbox) = mpsc::unbounded_channel();
    let run = host.run(inbox, Duration::from_millis(1), |_| {}, |_| {});
    drop(run);
    wait_for_retirement(&state).await;
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.dropped.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(state.joined.load(Ordering::SeqCst));
}
