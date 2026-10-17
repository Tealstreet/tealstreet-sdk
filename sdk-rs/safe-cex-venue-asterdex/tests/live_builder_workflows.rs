use safe_cex_transport::TransportRegistry;
use safe_cex_venue_asterdex::{
    builder::{AsterBuilderApprovalCheckResult, AsterBuilderApprovalResult},
    builder_approval::*,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut f = Box::pin(future);
    let w = Waker::noop().clone();
    for _ in 0..32 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(&w)) {
            return v;
        }
    }
    panic!("32-poll host floor exceeded");
}
#[derive(Default)]
struct Wallet {
    queue: Rc<RefCell<VecDeque<Value>>>,
    calls: Rc<RefCell<Vec<Value>>>,
    provider_calls: Rc<RefCell<Vec<usize>>>,
    active: Rc<Cell<usize>>,
    id: usize,
}
fn failure(q: &Value) -> ApprovalFailure {
    ApprovalFailure {
        value: q["reject"].clone(),
        error_message: if q["isError"] == true {
            Some(q["reject"]["message"].as_str().unwrap_or("").into())
        } else {
            None
        },
    }
}
impl BrowserWallet for Wallet {
    fn request(&self, request: Value) -> ApprovalFuture<Value> {
        self.calls.borrow_mut().push(request);
        self.provider_calls.borrow_mut().push(self.id);
        let q = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("independent planned wallet call");
        if q["replaceProvider"] == true {
            self.active.set(1);
        }
        let answer = if q.get("reject").is_some() {
            Err(failure(&q))
        } else {
            Ok(q["value"].clone())
        };
        Box::pin(async move { answer })
    }
}
struct Host {
    wallet: Option<Rc<Wallet>>,
    alternate: Rc<Wallet>,
    queue: RefCell<VecDeque<Value>>,
    requests: RefCell<Vec<ApprovalRequest>>,
    registry: TransportRegistry,
    now: f64,
}
impl ApprovalHost for Host {
    fn wallet(&self) -> Option<Rc<dyn BrowserWallet>> {
        self.wallet.clone().map(|w| {
            if w.active.get() == 1 {
                self.alternate.clone() as Rc<dyn BrowserWallet>
            } else {
                w as Rc<dyn BrowserWallet>
            }
        })
    }
    fn current_time_ms(&self) -> f64 {
        self.now
    }
    fn registry(&self) -> &TransportRegistry {
        &self.registry
    }
    fn fetch(&self, r: ApprovalRequest) -> ApprovalFuture<ApprovalResponse> {
        self.requests.borrow_mut().push(r);
        let q = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("independent planned fetch call");
        let answer = if q.get("reject").is_some() {
            Err(failure(&q))
        } else {
            Ok(ApprovalResponse {
                status: q["status"].as_u64().unwrap_or(200) as u16,
                ok: q["ok"].as_bool().unwrap_or(true),
                json: match q["jsonError"].as_str() {
                    Some(s) => Err(ApprovalFailure::error(s)),
                    None => Ok(q["payload"].clone()),
                },
            })
        };
        Box::pin(async move { answer })
    }
}
const NAMES: [&str; 36] = [
    "switch-ok",
    "switch-add",
    "switch-rejected",
    "switch-unsupported",
    "switch-string-code",
    "switch-error",
    "switch-no-wallet",
    "switch-add-fails",
    "check-approved",
    "check-insufficient",
    "check-missing",
    "check-null-rate",
    "check-nonfinite",
    "check-first-match",
    "check-null-row-before-match",
    "check-invalid-address",
    "check-zero-address",
    "check-invalid-key",
    "check-http-fails",
    "check-fetch-rejects",
    "check-json-rejects",
    "check-zero-key-rejects-outside-catch",
    "approve-success",
    "approve-testnet-custom-base",
    "approve-no-wallet",
    "approve-invalid-address",
    "approve-network-rejected",
    "approve-no-account",
    "approve-wrong-wallet",
    "approve-invalid-signature",
    "approve-string-response-code",
    "approve-http-error",
    "approve-json-error",
    "approve-wallet-error",
    "switch-provider-replaced",
    "approve-provider-replaced",
];
fn oracle() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/builder-workflow-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source boundary: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 36, "independent builder scenario floor");
    for (c, name) in cases.iter().zip(NAMES) {
        assert_eq!(c["name"], name, "independent builder scenario identity");
    }
    cases
}
fn component(s: &str, form: bool) -> String {
    let bytes = s.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            output.push(
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap(), 16).unwrap(),
            );
            i += 3;
        } else {
            output.push(if form && bytes[i] == b'+' {
                b' '
            } else {
                bytes[i]
            });
            i += 1;
        }
    }
    String::from_utf8(output).unwrap()
}
fn fields(s: &str, form: bool) -> Vec<(String, String)> {
    s.split('&')
        .filter(|s| !s.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (component(k, form), component(v, form))
        })
        .collect()
}
fn compare(kind: &str) {
    let mut wire = Vec::new();
    for c in oracle().into_iter().filter(|c| c["kind"] == kind) {
        let name = c["name"].as_str().unwrap();
        let wallet = Rc::new(Wallet::default());
        wallet
            .queue
            .replace(c["wallet"].as_array().cloned().unwrap_or_default().into());
        let alternate = Rc::new(Wallet {
            queue: wallet.queue.clone(),
            calls: wallet.calls.clone(),
            provider_calls: wallet.provider_calls.clone(),
            active: wallet.active.clone(),
            id: 1,
        });
        let host = Host {
            alternate,
            wallet: if c["noWallet"] == true {
                None
            } else {
                Some(wallet.clone())
            },
            queue: RefCell::new(c["fetch"].as_array().cloned().unwrap_or_default().into()),
            requests: RefCell::new(Vec::new()),
            registry: TransportRegistry::default(),
            now: 1000000.0 + c["index"].as_f64().unwrap() * 2000.0,
        };
        match kind {
            "switch" => {
                let actual = run(switch_to_aster_signature_chain(&host));
                let expected: AsterBuilderApprovalResult =
                    serde_json::from_value(c["result"].clone()).unwrap();
                assert_eq!(actual, expected, "{name} native chain-switch result");
            }
            "approve" => {
                let actual = run(approve_aster_builder_fee_with_browser_wallet(
                    &host,
                    &serde_json::from_value(c["params"].clone()).unwrap(),
                ));
                let expected: AsterBuilderApprovalResult =
                    serde_json::from_value(c["result"].clone()).unwrap();
                assert_eq!(
                    actual.unwrap(),
                    expected,
                    "{name} native wallet approval result"
                );
            }
            "check" => {
                let actual = run(check_aster_builder_approved(
                    &host,
                    &serde_json::from_value(c["params"].clone()).unwrap(),
                ));
                if c.get("rejection").is_some() {
                    assert!(
                        actual.is_err(),
                        "{name} native signing rejection outside fetch catch"
                    );
                } else {
                    let expected: AsterBuilderApprovalCheckResult =
                        serde_json::from_value(c["result"].clone()).unwrap();
                    assert_eq!(
                        actual.unwrap(),
                        expected,
                        "{name} native builder check result"
                    );
                }
            }
            _ => unreachable!(),
        }
        assert_eq!(
            *wallet.provider_calls.borrow(),
            serde_json::from_value::<Vec<usize>>(c["providerCalls"].clone()).unwrap(),
            "{name} native provider selection after await"
        );
        let expected_wallet = c["walletCalls"].as_array().unwrap();
        assert_eq!(
            wallet.calls.borrow().len(),
            expected_wallet.len(),
            "{name} native wallet call count"
        );
        for (actual, expected) in wallet.calls.borrow().iter().zip(expected_wallet) {
            assert_eq!(
                actual["method"], expected["method"],
                "{name} native wallet method"
            );
            if actual["method"] == "eth_signTypedData_v4" {
                assert_eq!(
                    actual["params"][0], expected["params"][0],
                    "{name} native signer address"
                );
                let data: Value =
                    serde_json::from_str(actual["params"][1].as_str().unwrap()).unwrap();
                let expected_data: Value =
                    serde_json::from_str(expected["params"][1].as_str().unwrap()).unwrap();
                assert_eq!(
                    data, expected_data,
                    "{name} decoded native approval typed data"
                );
                wire.push((
                    actual["params"][1].as_str().unwrap().to_owned(),
                    expected["params"][1].as_str().unwrap().to_owned(),
                    format!("{name} typed-data wire"),
                ));
            } else {
                assert_eq!(actual, expected, "{name} native wallet request fields");
            }
        }
        let expected_requests = c["fetchCalls"].as_array().unwrap();
        assert_eq!(
            host.requests.borrow().len(),
            expected_requests.len(),
            "{name} native fetch count"
        );
        for (actual, expected) in host.requests.borrow().iter().zip(expected_requests) {
            assert_eq!(
                actual.method,
                expected["method"].as_str().unwrap(),
                "{name} native HTTP method"
            );
            let expected_headers: std::collections::BTreeMap<String, String> =
                serde_json::from_value(expected["headers"].clone()).unwrap();
            assert_eq!(
                actual.headers, expected_headers,
                "{name} native HTTP headers"
            );
            assert_eq!(
                actual.body.is_some(),
                !expected["body"].is_null(),
                "{name} native body presence"
            );
            if let Some(body) = &actual.body {
                assert_eq!(
                    fields(body, true),
                    fields(expected["body"].as_str().unwrap(), true),
                    "{name} decoded native approval form values"
                );
                wire.push((
                    body.clone(),
                    expected["body"].as_str().unwrap().into(),
                    format!("{name} form wire"),
                ));
            }
            let (path, query) = actual.url.split_once('?').unwrap_or((&actual.url, ""));
            let (ep, eq) = expected["url"]
                .as_str()
                .unwrap()
                .split_once('?')
                .unwrap_or((expected["url"].as_str().unwrap(), ""));
            assert_eq!(path, ep, "{name} native endpoint path");
            assert_eq!(
                fields(query, false),
                fields(eq, false),
                "{name} decoded native signed query values"
            );
            wire.push((
                actual.url.clone(),
                expected["url"].as_str().unwrap().into(),
                format!("{name} URL wire"),
            ));
        }
    }
    // All decoded result/request assertions across the family precede wire checks.
    for (a, e, label) in wire {
        assert_eq!(a, e, "{label}");
    }
}
#[test]
#[ignore = "live unmodified TypeScript wallet workflow"]
fn chain_switch_matches_source() {
    compare("switch");
}
#[test]
#[ignore = "live unmodified TypeScript wallet workflow"]
fn builder_check_matches_source() {
    compare("check");
}
#[test]
#[ignore = "live unmodified TypeScript wallet workflow"]
fn wallet_approval_matches_source() {
    compare("approve");
}
