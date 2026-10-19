//! Constructed MEXC sockets on NativeRuntime scheduling; only terminal I/O is scripted.
use super::*;
use safe_cex_exchange::native::{NativeRuntime, native_runtime};
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner as RuntimeSigner, RuntimeTaskKind};
use safe_cex_transport::signing::{RemoteSigner, RemoteSignerRequest, RemoteSignerResponse};
use safe_cex_transport::{TransportError, TransportFuture};
use std::time::Duration;

struct Observed {
    native: Rc<NativeRuntime>,
    next: Cell<usize>,
    completed: Rc<RefCell<Vec<(usize, usize)>>>,
    reads: Rc<Cell<usize>>,
    requests: Rc<RefCell<Vec<String>>>,
    sent: Rc<RefCell<Vec<String>>>,
}
impl Observed {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            native: native_runtime(),
            next: Cell::new(0),
            completed: Rc::default(),
            reads: Rc::default(),
            requests: Rc::default(),
            sent: Rc::default(),
        })
    }
    fn observe(&self, work: Work) -> Work {
        let id = self.next.get();
        self.next.set(id + 1);
        let completed = self.completed.clone();
        let reads = self.reads.clone();
        Box::pin(async move {
            work.await;
            completed.borrow_mut().push((id, reads.get()));
        })
    }
}
impl ExchangeRuntime for Observed {
    fn now_ms(&self) -> f64 {
        self.native.now_ms()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.native.monotonic_now_ms()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        self.native.sleep(ms)
    }
    fn spawn_local(&self, work: Work) -> Box<dyn ExchangeTask> {
        self.native.spawn_local(self.observe(work))
    }
    fn spawn_classified(&self, kind: RuntimeTaskKind, work: Work) -> Box<dyn ExchangeTask> {
        self.native.spawn_classified(kind, self.observe(work))
    }
    fn task_work_started(&self) {
        self.native.task_work_started()
    }
    fn console_values(&self, level: &str, values: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.native.console_values(level, values)
    }
    fn unique_id(&self) -> String {
        self.native.unique_id()
    }
    fn is_worker(&self) -> bool {
        self.native.is_worker()
    }
    fn random(&self) -> f64 {
        self.native.random()
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.native.report_exception(error, context)
    }
    fn report_raw_exception(
        &self,
        error: &safe_cex_exchange::js_runtime::value::Js,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.native.report_raw_exception(error, context)
    }
    fn execute_worker_request(&self, request: Value) -> ExchangeFuture<'static, Value> {
        self.native.execute_worker_request(request)
    }
}
impl WsRuntime for Observed {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        self.native.transport()
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        self.native.tunnel_host()
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket: Rc<dyn WsSocket> = Rc::new(Socket {
            events: RefCell::new(VecDeque::from([WsEvent::Open])),
            sent: self.sent.clone(),
            closed: Cell::new(false),
            send_hook: Rc::default(),
        });
        Box::pin(async move { Ok(socket) })
    }
    fn headers_capable(&self) -> bool {
        self.native.headers_capable()
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        self.native.min_reconnection_delay_ms()
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        self.native.library_min_reconnection_delay_ms()
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Assets(Rc<Cell<usize>>, Rc<RefCell<Vec<String>>>);
impl RestAdapter for Assets {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.1.borrow_mut().push(request.url.clone());
        let bytes = if request.url.contains("/account/assets") {
            self.0.set(self.0.get() + 1);
            br#"{"success":true,"data":[{"currency":"USDT","availableBalance":17,"equity":23,"frozenBalance":5,"unrealized":3}]}"#.to_vec()
        } else {
            br#"{"success":true,"data":[]}"#.to_vec()
        };
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(bytes),
            })
        })
    }
}
fn constructed(runtime: Rc<Observed>, signer: Option<Rc<dyn RuntimeSigner>>) -> Rc<MexcExchange> {
    let store = Rc::new(DefaultStore::new());
    let mut base = BaseExchange::new(
        ExchangeName::Mexc,
        serde_json::from_value(json!({"key":" fixture ","secret":"secret"})).unwrap(),
        store,
        runtime.clone(),
        ExchangeSettings::default(),
    );
    base.signers.remote_signer = signer;
    let clock = runtime.clone();
    MexcExchange::new(
        base,
        Rc::new(Assets(runtime.reads.clone(), runtime.requests.clone())),
        Rc::new(|n| n),
        MexcSocketDependencies {
            runtime,
            current_time_ms: Rc::new(move || clock.now_ms()),
            remote_account_name: Some("account-A".into()),
            emit_fill: Rc::new(|_| Ok(())),
        },
        Rc::new(|_, _| panic!("no external provider")),
    )
    .unwrap()
}
#[test]
fn private_balance_timer_starts_http_before_its_callback_returns() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor,async {
        let runtime=Observed::new();let venue=constructed(runtime.clone(),None);
        let private=venue.base().get_socket(SocketSlot::Private).unwrap();
        private.on_message(&SocketFrame::Text(r#"{"channel":"push.personal.asset"}"#.into())).unwrap();
        // trackLastReceived may register health work first; the final task
        // installed by this synchronous asset frame is its balance timeout.
        let task=runtime.next.get()-1;
        assert_eq!(runtime.reads.get(),0,"balance waits for the 1000ms debounce");
        tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                if let Some((_,reads))=runtime.completed.borrow().iter().find(|(id,_)|*id==task) {
                    assert_eq!(*reads,1,"MEXC BALANCE ENTRY starts HTTP before returning from the actual NativeRuntime timer callback");break;
                }
                tokio::task::yield_now().await;
            }
            while venue.base().store.balance().unwrap().total != 23.0 {tokio::task::yield_now().await;}
        }).await.unwrap_or_else(|error| panic!("native timer did not settle: {error}; next={}, completed={:?}, reads={}, balance={:?}", runtime.next.get(),runtime.completed.borrow(),runtime.reads.get(),venue.base().store.balance()));
        assert_eq!(runtime.reads.get(),1);
        let balance=venue.base().store.balance().unwrap();
        assert_eq!(balance.free.to_bits(),17_f64.to_bits());
        assert_eq!(balance.total.to_bits(),23_f64.to_bits());
        venue.dispose().unwrap();
    });
}
struct Signer {
    calls: RefCell<Vec<Value>>,
}
impl RemoteSigner for Signer {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        match request {
            RemoteSignerRequest::WsAuth {
                exchange_name,
                account_name,
                request,
            } => {
                self.calls.borrow_mut().push(
                    json!({"exchange":exchange_name,"account":account_name,"request":request}),
                );
                Box::pin(std::future::pending())
            }
            _ => Box::pin(async {
                Err(TransportError::Host(
                    "HTTP signer unused in login prefix witness".into(),
                ))
            }),
        }
    }
}
impl RuntimeSigner for Signer {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
#[test]
fn private_remote_signer_prefix_precedes_reconnected_on_native_runtime() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = Observed::new();
        let signer = Rc::new(Signer {
            calls: RefCell::new(vec![]),
        });
        let venue = constructed(runtime.clone(), Some(signer.clone()));
        let seen = Rc::new(Cell::new(0));
        let count = seen.clone();
        let capture = signer.clone();
        venue.base().on(
            "ws-reconnected",
            Rc::new(move |_, _| {
                assert_eq!(
                    capture.calls.borrow().len(),
                    1,
                    "MEXC SIGNER ENTRY precedes source reconnected prefix"
                );
                count.set(count.get() + 1);
                Ok(())
            }),
        );
        venue
            .base()
            .get_socket(SocketSlot::Private)
            .unwrap()
            .connect_and_subscribe(false)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while seen.get() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let calls = signer.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["exchange"], "mexc");
        assert_eq!(calls[0]["account"], "account-A");
        let ws = &calls[0]["request"]["wsAuth"];
        assert_eq!(ws["outputFormat"], "hex");
        assert_eq!(
            ws["toSign"],
            format!("fixture{}", ws["extraData"]["reqTime"].as_str().unwrap())
        );
        assert!(
            runtime
                .sent
                .borrow()
                .iter()
                .all(|text| !text.contains("\"method\":\"login\"")),
            "held signer cannot send login"
        );
        drop(calls);
        venue.dispose().unwrap();
    });
}

#[test]
fn private_snapshot_http_prefix_precedes_reconnected_on_native_runtime() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = Observed::new();
        let venue = constructed(runtime.clone(), None);
        let seen = Rc::new(RefCell::new(None));
        let mark = seen.clone();
        let requests = runtime.requests.clone();
        let store = venue.base().store.clone();
        venue.base().on(
            "ws-reconnected",
            Rc::new(move |_, _| {
                *mark.borrow_mut() = Some((requests.borrow().clone(), store.loaded()?.orders));
                Ok(())
            }),
        );
        venue
            .base()
            .get_socket(SocketSlot::Private)
            .unwrap()
            .connect_and_subscribe(false)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while seen.borrow().is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let (requests, loaded) = seen.borrow_mut().take().unwrap();
        assert!(
            requests
                .iter()
                .any(|url| url.contains("/order/list/open_orders")),
            "MEXC SNAPSHOT ENTRY starts actual orders HTTP before reconnected"
        );
        assert!(
            !loaded,
            "MEXC SNAPSHOT ENTRY keeps ready HTTP Store work after the source await"
        );
        assert_eq!(
            requests.len(),
            1,
            "MEXC snapshot starts exactly its first request before reconnect"
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while !venue.base().store.loaded().unwrap().orders {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            runtime.requests.borrow().len(),
            3,
            "MEXC snapshot completes regular/plan/stop request sequence"
        );
        venue.dispose().unwrap();
    });
}
