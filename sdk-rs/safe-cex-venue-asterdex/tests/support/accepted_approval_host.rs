use crate::builder_approval::{
    ApprovalFailure, ApprovalFuture, ApprovalHost, ApprovalRequest, ApprovalResponse, BrowserWallet,
};
use safe_cex_transport::TransportRegistry;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..32 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("approval boundary fixture did not settle");
}

fn failure(value: &Value) -> ApprovalFailure {
    ApprovalFailure {
        value: value["reject"].clone(),
        error_message: (value["isError"] == true).then(|| {
            value["reject"]["message"]
                .as_str()
                .expect("authored Error message")
                .into()
        }),
    }
}

struct Wallet {
    answers: RefCell<VecDeque<Value>>,
    calls: Rc<RefCell<Vec<Value>>>,
}

impl BrowserWallet for Wallet {
    fn request(&self, request: Value) -> ApprovalFuture<Value> {
        self.calls.borrow_mut().push(request);
        let answer = self
            .answers
            .borrow_mut()
            .pop_front()
            .expect("planned wallet request");
        Box::pin(async move {
            if answer.get("reject").is_some() {
                Err(failure(&answer))
            } else {
                Ok(answer["value"].clone())
            }
        })
    }
}

pub struct Fixture {
    wallet: Option<Rc<Wallet>>,
    calls: Rc<RefCell<Vec<Value>>>,
    fetch_answers: RefCell<VecDeque<Value>>,
    fetches: RefCell<Vec<ApprovalRequest>>,
    registry: TransportRegistry,
    now: f64,
}

impl Fixture {
    pub fn new(case: &Value) -> Self {
        let calls = Rc::new(RefCell::new(vec![]));
        Self {
            wallet: (case["noWallet"] != true).then(|| {
                Rc::new(Wallet {
                    answers: RefCell::new(
                        case["wallet"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                            .into(),
                    ),
                    calls: calls.clone(),
                })
            }),
            calls,
            fetch_answers: RefCell::new(
                case["fetch"].as_array().cloned().unwrap_or_default().into(),
            ),
            fetches: RefCell::new(vec![]),
            registry: TransportRegistry::default(),
            now: case["now"].as_f64().unwrap_or(1_000_000_000_000.0),
        }
    }

    pub fn observe(&self, result: Value) -> Value {
        assert!(
            self.fetch_answers.borrow().is_empty(),
            "all planned HTTP replies consumed"
        );
        if let Some(wallet) = &self.wallet {
            assert!(
                wallet.answers.borrow().is_empty(),
                "all planned wallet replies consumed"
            );
        }
        json!({
            "result": result,
            "walletMethods": self.calls.borrow().iter().map(|call| {
                call["method"].as_str().expect("actual wallet method").to_owned()
            }).collect::<Vec<_>>(),
            "fetchMethods": self.fetches.borrow().iter().map(|call| call.method.clone()).collect::<Vec<_>>(),
        })
    }
}

impl ApprovalHost for Fixture {
    fn wallet(&self) -> Option<Rc<dyn BrowserWallet>> {
        self.wallet
            .clone()
            .map(|wallet| wallet as Rc<dyn BrowserWallet>)
    }

    fn current_time_ms(&self) -> f64 {
        self.now
    }

    fn registry(&self) -> &TransportRegistry {
        &self.registry
    }

    fn fetch(&self, request: ApprovalRequest) -> ApprovalFuture<ApprovalResponse> {
        self.fetches.borrow_mut().push(request);
        let answer = self
            .fetch_answers
            .borrow_mut()
            .pop_front()
            .expect("planned HTTP request");
        Box::pin(async move {
            if answer.get("reject").is_some() {
                return Err(failure(&answer));
            }
            Ok(ApprovalResponse {
                status: answer["status"].as_u64().unwrap_or(200) as u16,
                ok: answer["ok"].as_bool().unwrap_or(true),
                json: match answer["jsonError"].as_str() {
                    Some(message) => Err(ApprovalFailure::error(message)),
                    None => Ok(answer["payload"].clone()),
                },
            })
        })
    }
}
