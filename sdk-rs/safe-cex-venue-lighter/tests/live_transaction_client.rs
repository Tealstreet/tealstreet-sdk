#[allow(dead_code)] // Shared source-native carrier also serves the broader signing type oracle.
#[path = "support/signing_native.rs"]
mod native;
use native::*;
use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    http_transactions::*, transaction_client::*, transaction_client_types::*,
    transaction_promise::*, transaction_signing::*,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::{Rc, Weak},
};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/transaction-client-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            v["contentHash"], "794a306617545328da0c72bd9fb122663849f4526ed3aac6a0e3355f9751ee68",
            "client independent corpus digest"
        );
        v
    })
}
struct State {
    mode: Value,
    scenario: String,
    trace: Vec<Value>,
    counts: BTreeMap<String, usize>,
    token: HostThrownValue,
    client: Weak<TransactionClient>,
    clocks: usize,
    encoder: AuthTokenEncoding,
    encoder_reads: usize,
    held: Option<PromiseResolver<BigInt>>,
    batch_hashes: Shared<Vec<Vec<u16>>>,
}
fn event(state: &Rc<RefCell<State>>, method: &str, args: Vec<Value>) -> usize {
    let mut s = state.borrow_mut();
    s.trace.push(json!({"method":method,"args":args}));
    let c = s.counts.entry(method.into()).or_default();
    *c += 1;
    *c
}
struct Runtime {
    state: Rc<RefCell<State>>,
    tasks: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl Runtime {
    fn drain(&self) {
        for _ in 0..500 {
            let task = self.tasks.borrow_mut().pop_front();
            match task {
                Some(task) => task(),
                None => return,
            }
        }
        panic!("native microtask floor exhausted at500");
    }
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, task: Box<dyn FnOnce()>) {
        self.tasks.borrow_mut().push_back(task);
    }
    fn now_millis(&self) -> f64 {
        let cached = self.state.borrow().client.upgrade().unwrap().cached_nonce();
        event(&self.state, "clock", vec![ob(&cached)]);
        let mut s = self.state.borrow_mut();
        let i = s.clocks;
        s.clocks += 1;
        1700000000000.0 + 7.0 * i as f64
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        let mut s = self.state.borrow_mut();
        s.encoder_reads += 1;
        s.encoder
    }
}
struct Crypto {
    state: Rc<RefCell<State>>,
    fp5: Box<Fp5Callback<'static>>,
}
impl TransactionCrypto for Crypto {
    fn is_ready(&self) -> Result<bool, HostThrownValue> {
        event(&self.state, "ready", vec![]);
        let mut s = self.state.borrow_mut();
        if s.mode["kind"] == "ready-disable" {
            s.encoder = AuthTokenEncoding::Unavailable;
        }
        Ok(s.mode["kind"] != "ready-false")
    }
    fn poseidon2_hash(&self, input: &[u16]) -> CryptoCallResult {
        let i = event(&self.state, "hash", vec![t(input)]);
        Ok(format!("hash-{i}").encode_utf16().chain([0xd800]).collect())
    }
    fn poseidon2_two_to_one(&self, left: &[u16], right: &[u16]) -> CryptoCallResult {
        let i = event(&self.state, "aggregate", vec![t(left), t(right)]);
        Ok(format!("aggregate-{i}")
            .encode_utf16()
            .chain([0xd800])
            .collect())
    }
    fn fp5_callback(&self) -> Result<Option<&Fp5Callback<'_>>, HostThrownValue> {
        event(&self.state, "getFp5", vec![]);
        Ok(Some(self.fp5.as_ref()))
    }
    fn schnorr_sign(&self, input: &[u16], key: &[u16]) -> CryptoCallResult {
        event(&self.state, "sign", vec![t(input), t(key)]);
        Ok("signature".encode_utf16().chain([0xdfff]).collect())
    }
}
fn tx_reference(v: &TransactionRef) -> Value {
    tx_native(&match v {
        TransactionRef::CreateOrder(v) => AnyTxInfo::CreateOrder(v.borrow().clone()),
        TransactionRef::CreateGroupedOrders(v) => {
            AnyTxInfo::CreateGroupedOrders(v.borrow().clone())
        }
        TransactionRef::CancelOrder(v) => AnyTxInfo::CancelOrder(v.borrow().clone()),
        TransactionRef::Transfer(v) => AnyTxInfo::Transfer(v.borrow().clone()),
        TransactionRef::UpdateLeverage(v) => AnyTxInfo::UpdateLeverage(v.borrow().clone()),
        TransactionRef::ApproveIntegrator(v) => AnyTxInfo::ApproveIntegrator(v.borrow().clone()),
    })
}
struct Api {
    state: Rc<RefCell<State>>,
    runtime: Rc<Runtime>,
}
impl TransactionApi for Api {
    fn get_next_nonce(&self, account: &BigInt, key: f64) -> NativePromise<BigInt> {
        let i = event(&self.state, "nonce", vec![b(account), n(key)]);
        let mut s = self.state.borrow_mut();
        if s.mode["kind"] == "nonce-error"
            || (i == 1 && (s.scenario == "fetch-reject" || s.scenario == "refresh-reject"))
        {
            return NativePromise::settled(self.runtime.clone(), Err(s.token.clone().into()));
        }
        if ["concurrent-fetch", "reset-inflight", "drop-observer"].contains(&s.scenario.as_str()) {
            let (p, r) = NativePromise::pending(self.runtime.clone());
            s.held = Some(r);
            p
        } else {
            NativePromise::settled(self.runtime.clone(), Ok(71.into()))
        }
    }
    fn send_raw_tx(&self, tx: TransactionRef) -> NativePromise<Vec<u16>> {
        event(&self.state, "send", vec![tx_reference(&tx)]);
        let s = self.state.borrow();
        NativePromise::settled(
            self.runtime.clone(),
            if s.mode["kind"] == "send-error" {
                Err(s.token.clone().into())
            } else {
                Ok("sent"
                    .encode_utf16()
                    .chain([0xd800, 0xd83d, 0xdc69])
                    .collect())
            },
        )
    }
    fn send_raw_tx_batch(
        &self,
        txs: Shared<Vec<Shared<L2CreateOrderTxInfo>>>,
    ) -> ObjectPromise<SendTxBatchResponse> {
        let transactions = txs.borrow();
        let len = transactions.len();
        assert!(
            [0, 2].contains(&len),
            "native sent batch independent size floor"
        );
        event(
            &self.state,
            "batch",
            vec![Value::Array(
                transactions
                    .iter()
                    .map(|v| tx_native(&AnyTxInfo::CreateOrder(v.borrow().clone())))
                    .collect(),
            )],
        );
        let s = self.state.borrow();
        ObjectPromise::settled(
            self.runtime.clone(),
            if s.mode["kind"] == "batch-error" {
                Err(s.token.clone().into())
            } else {
                Ok(RefCell::new(SendTxBatchResponse {
                    tx_hashes: s.batch_hashes.clone(),
                    predicted_execution_time_ms: Some(17.25),
                    volume_quota_remaining: Some(VolumeQuotaRemaining::String(
                        "quota".encode_utf16().chain([0xdfff]).collect(),
                    )),
                }))
            },
        )
    }
}
struct Harness {
    client: Rc<TransactionClient>,
    runtime: Rc<Runtime>,
    state: Rc<RefCell<State>>,
}
impl Harness {
    fn new(mode: Value, scenario: &str) -> Self {
        let state = Rc::new(RefCell::new(State {
            encoder: match mode["encoder"].as_str() {
                Some("buffer") => AuthTokenEncoding::NodeBuffer,
                Some("unavailable") => AuthTokenEncoding::Unavailable,
                _ => AuthTokenEncoding::TextEncoder,
            },
            mode,
            scenario: scenario.into(),
            trace: vec![],
            counts: BTreeMap::new(),
            token: HostThrownValue::new("host".encode_utf16().chain([0xd800]).collect::<Vec<_>>()),
            client: Weak::new(),
            clocks: 0,
            encoder_reads: 0,
            held: None,
            batch_hashes: shared(vec![
                "hash-A".encode_utf16().chain([0xd800]).collect(),
                "hash-B👩".encode_utf16().collect(),
            ]),
        }));
        let runtime = Rc::new(Runtime {
            state: state.clone(),
            tasks: RefCell::new(VecDeque::new()),
        });
        let fpstate = state.clone();
        let crypto = Rc::new(Crypto {
            state: state.clone(),
            fp5: Box::new(move |input| {
                let i = event(&fpstate, "fp5", vec![t(input)]);
                Ok(match i {
                    1 => "0xbase".encode_utf16().chain([0xd800]).collect(),
                    2 => "0Xattrs".encode_utf16().chain([0xdfff]).collect(),
                    _ => "fp5-final👩".encode_utf16().collect(),
                })
            }),
        });
        let client = TransactionClient::new(TransactionClientConfig {
            http_client: Rc::new(Api {
                state: state.clone(),
                runtime: runtime.clone(),
            }),
            crypto,
            runtime: runtime.clone(),
            chain_id: 304.0,
            private_key: "key".encode_utf16().chain([0xd800, 45, 65]).collect(),
            account_index: 9007199254740993u64.into(),
            api_key_index: 13.0,
        });
        state.borrow_mut().client = Rc::downgrade(&client);
        Self {
            client,
            runtime,
            state,
        }
    }
    fn prime(&self, n: BigInt) {
        // Prime through the real allocator; do not write the native field under test.
        self.state.borrow_mut().scenario = "concurrent-fetch".into();
        drop(self.client.get_next_nonce(false));
        self.runtime.drain();
        self.state.borrow_mut().held.take().unwrap().settle(Ok(n));
        self.runtime.drain();
        let mut s = self.state.borrow_mut();
        s.scenario.clear();
        s.trace.clear();
        s.counts.clear();
    }
}
fn error_native(e: TransactionSigningError, token: &HostThrownValue) -> Value {
    match e {
        TransactionSigningError::Generated(e) => json!({"error":{"name":e.name,"units":e.message}}),
        TransactionSigningError::Thrown(e) => {
            assert!(e.ptr_eq(token), "native host rejection identity");
            assert_eq!(
                e.value().downcast_ref::<Vec<u16>>().unwrap(),
                &"host".encode_utf16().chain([0xd800]).collect::<Vec<_>>(),
                "native host rejection payload"
            );
            json!({"thrown":true})
        }
    }
}
fn observe<T: 'static>(
    p: NativePromise<T>,
    out: Rc<RefCell<Vec<Value>>>,
    token: HostThrownValue,
    project: impl FnOnce(&T) -> Value + 'static,
) {
    p.subscribe(move |r| {
        out.borrow_mut().push(match r {
            Ok(v) => json!({"ok":project(&v)}),
            Err(e) => error_native(e, &token),
        })
    });
}
fn cancel(v: &Value) -> CancelOrderParams {
    CancelOrderParams {
        order_id: big(&v["orderId"]),
        market_id: num(&v["marketId"]),
    }
}
fn transfer(v: &Value) -> TransferParams {
    TransferParams {
        to_account_index: big(&v["toAccountIndex"]),
        amount: big(&v["amount"]),
        token_id: opt_num(v, "tokenId"),
    }
}
fn leverage(v: &Value) -> UpdateLeverageParams {
    UpdateLeverageParams {
        market_id: num(&v["marketId"]),
        leverage: num(&v["leverage"]),
        margin_mode: MarginMode::new(num(&v["marginMode"])).unwrap(),
    }
}
fn approval(v: &Value) -> ApproveIntegratorParams {
    ApproveIntegratorParams {
        integrator_account_index: big(&v["integratorAccountIndex"]),
        max_perps_taker_fee: num(&v["maxPerpsTakerFee"]),
        max_perps_maker_fee: num(&v["maxPerpsMakerFee"]),
        max_spot_taker_fee: num(&v["maxSpotTakerFee"]),
        max_spot_maker_fee: num(&v["maxSpotMakerFee"]),
        approval_expiry: big(&v["approvalExpiry"]),
        nonce: opt_big(v, "nonce"),
        l1_signature: v.get("l1Signature").filter(|v| !missing(Some(v))).map(text),
    }
}
fn invoke(h: &Harness, method: &str, args: &[Value], out: Rc<RefCell<Vec<Value>>>) {
    let c = &h.client;
    let token = h.state.borrow().token.clone();
    let a = &args[0];
    match method {
        "getCreateOrderTransaction" => {
            observe(c.get_create_order_transaction(order(a)), out, token, |v| {
                tx_native(&AnyTxInfo::CreateOrder(v.borrow().clone()))
            })
        }
        "getCreateGroupedOrdersTransaction" => observe(
            c.get_create_grouped_orders_transaction(group(a)),
            out,
            token,
            |v| tx_native(&AnyTxInfo::CreateGroupedOrders(v.borrow().clone())),
        ),
        "getCancelOrderTransaction" => {
            observe(c.get_cancel_order_transaction(cancel(a)), out, token, |v| {
                tx_native(&AnyTxInfo::CancelOrder(v.borrow().clone()))
            })
        }
        "getTransferTransaction" => {
            observe(c.get_transfer_transaction(transfer(a)), out, token, |v| {
                tx_native(&AnyTxInfo::Transfer(v.borrow().clone()))
            })
        }
        "getUpdateLeverageTransaction" => observe(
            c.get_update_leverage_transaction(leverage(a)),
            out,
            token,
            |v| tx_native(&AnyTxInfo::UpdateLeverage(v.borrow().clone())),
        ),
        "getApproveIntegratorTransaction" => observe(
            c.get_approve_integrator_transaction(approval(a)),
            out,
            token,
            |v| tx_native(&AnyTxInfo::ApproveIntegrator(v.borrow().clone())),
        ),
        "getCreateOrderTransactions" => {
            let params = a.as_array().unwrap().iter().map(order).collect::<Vec<_>>();
            let expected = if params.is_empty() { 0 } else { 2 };
            observe(
                c.get_create_order_transactions(params),
                out,
                token,
                move |v| {
                    let list = v.borrow();
                    assert_eq!(
                        list.len(),
                        expected,
                        "native result batch independent count"
                    );
                    Value::Array(
                        list.iter()
                            .map(|v| tx_native(&AnyTxInfo::CreateOrder(v.borrow().clone())))
                            .collect(),
                    )
                },
            );
        }
        "getAuthToken" => observe(c.get_auth_token(big(a)), out, token, |v| {
            assert_eq!(
                v,
                &"3601:9007199254740993:13:signature"
                    .encode_utf16()
                    .chain([0xdfff])
                    .collect::<Vec<_>>(),
                "native auth decoded UTF16 value"
            );
            t(v)
        }),
        "submitCreateOrder" => observe(c.submit_create_order(order(a)), out, token, |v| t(v)),
        "submitCreateGroupedOrders" => {
            observe(c.submit_create_grouped_orders(group(a)), out, token, |v| {
                t(v)
            })
        }
        "submitCancelOrder" => observe(c.submit_cancel_order(cancel(a)), out, token, |v| t(v)),
        "submitTransfer" => observe(c.submit_transfer(transfer(a)), out, token, |v| t(v)),
        "submitUpdateLeverage" => {
            observe(c.submit_update_leverage(leverage(a)), out, token, |v| t(v))
        }
        "submitApproveIntegrator" => {
            observe(c.submit_approve_integrator(approval(a)), out, token, |v| {
                t(v)
            })
        }
        "submitCreateOrders" => {
            let hashes = h.state.borrow().batch_hashes.clone();
            observe(
                c.submit_create_orders(a.as_array().unwrap().iter().map(order).collect()),
                out,
                token,
                move |v| {
                    assert!(
                        std::ptr::eq(v, hashes.as_ref()),
                        "native submit batch preserves returned hash array"
                    );
                    assert_eq!(v.borrow().len(), 2, "native batch hashes independent count");
                    Value::Array(v.borrow().iter().map(|s| t(s)).collect())
                },
            );
        }
        other => panic!("unexpected method {other}"),
    }
}
#[test]
#[ignore = "actual source Node orchestration oracle"]
fn public_transaction_results_and_calls() {
    let rows = capture()["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 41, "public independent floor");
    let mut counts = BTreeMap::new();
    for row in rows {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0) += 1;
        let h = Harness::new(row["mode"].clone(), "");
        let out = Rc::new(RefCell::new(vec![]));
        invoke(&h, method, row["args"].as_array().unwrap(), out.clone());
        if method == "getAuthToken"
            || (method == "getApproveIntegratorTransaction"
                && !missing(row["args"][0].get("nonce")))
        {
            assert!(
                out.borrow().is_empty(),
                "settled promise observer must defer"
            );
        }
        h.runtime.drain();
        assert_eq!(out.borrow().len(), 1, "one native result");
        assert_eq!(
            h.client.cached_nonce(),
            opt_big(row, "cachedNonce"),
            "{method} native cache"
        );
        assert_eq!(
            h.state.borrow().clocks,
            row["clockCalls"].as_u64().unwrap() as usize,
            "{method} native clock count"
        );
        if method == "getAuthToken" {
            assert_eq!(
                h.state.borrow().encoder_reads,
                if row["mode"]["kind"] == "ready-false" {
                    0
                } else {
                    1
                },
                "lazy encoder branch after ready"
            );
        }
        identity(&out.borrow()[0], &row["output"], method);
        identity(
            &json!(h.state.borrow().trace),
            &row["trace"],
            &format!("{method} native callback arguments/order"),
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("getCreateOrderTransaction", 2),
            ("submitCreateOrder", 2),
            ("getCreateOrderTransactions", 4),
            ("submitCreateOrders", 4),
            ("getCreateGroupedOrdersTransaction", 3),
            ("submitCreateGroupedOrders", 2),
            ("getCancelOrderTransaction", 3),
            ("submitCancelOrder", 2),
            ("getTransferTransaction", 2),
            ("submitTransfer", 3),
            ("getUpdateLeverageTransaction", 2),
            ("submitUpdateLeverage", 2),
            ("getApproveIntegratorTransaction", 3),
            ("submitApproveIntegrator", 2),
            ("getAuthToken", 5)
        ]),
        "public executed independent floor"
    );
}
fn nonce_observe(
    p: NativePromise<BigInt>,
    label: &'static str,
    out: Rc<RefCell<Vec<Value>>>,
    token: HostThrownValue,
) {
    p.subscribe(move |r| {
        out.borrow_mut().push(match r {
            Ok(v) => json!({"label":label,"ok":b(&v)}),
            Err(e) => {
                assert!(
                    matches!(e,TransactionSigningError::Thrown(ref x)if x.ptr_eq(&token)),
                    "nonce native rejection identity"
                );
                json!({"label":label,"thrown":true})
            }
        })
    });
}
#[test]
#[ignore = "actual source Node nonce oracle"]
fn nonce_fifo_release_reset_and_detach() {
    let rows = capture()["nonceCases"].as_array().unwrap();
    assert_eq!(rows.len(), 7, "nonce independent scenario floor");
    for row in rows {
        let scenario = row["scenario"].as_str().unwrap();
        let h = Harness::new(json!({}), scenario);
        let out = Rc::new(RefCell::new(vec![]));
        let token = h.state.borrow().token.clone();
        if scenario == "fifo-cache" {
            h.prime(39.into());
            let a = order(&capture()["cases"][0]["args"][0]);
            let mut c = a.clone();
            c.order_id = 303.into();
            for (label, p) in [
                ("a", h.client.get_create_order_transaction(a)),
                ("b", h.client.get_create_order_transaction(c)),
            ] {
                let target = out.clone();
                p.subscribe(move |r|target.borrow_mut().push(json!({"label":label,"ok":tx_native(&AnyTxInfo::CreateOrder(r.unwrap().borrow().clone()))})));
            }
        } else {
            if scenario == "refresh-reject" || scenario == "reset-queued" {
                h.prime(19.into());
                h.state.borrow_mut().scenario = scenario.into();
            }
            let p = h.client.get_next_nonce(scenario == "refresh-reject");
            if scenario != "drop-observer" {
                nonce_observe(p, "a", out.clone(), token.clone());
            }
            if scenario != "reset-inflight" {
                nonce_observe(
                    h.client.get_next_nonce(false),
                    "b",
                    out.clone(),
                    token.clone(),
                );
            }
            if scenario == "concurrent-fetch" {
                nonce_observe(h.client.get_next_nonce(false), "c", out.clone(), token);
            }
            if scenario == "reset-queued" {
                h.client.reset_nonce();
            }
            h.runtime.drain();
            let held = h.state.borrow_mut().held.take();
            if let Some(held) = held {
                if scenario == "reset-inflight" {
                    h.client.reset_nonce();
                }
                held.settle(Ok(9.into()));
            }
        }
        h.runtime.drain();
        assert_eq!(
            h.client.cached_nonce(),
            opt_big(row, "cachedNonce"),
            "{scenario} nonce FIFO cached native value"
        );
        assert_eq!(
            out.borrow().len(),
            match scenario {
                "concurrent-fetch" => 3,
                "reset-inflight" | "drop-observer" => 1,
                _ => 2,
            },
            "nonce independent result count"
        );
        identity(&json!(*out.borrow()), &row["outputs"], scenario);
        identity(
            &json!(h.state.borrow().trace),
            &row["trace"],
            &format!("{scenario} native FIFO callback order"),
        );
    }
}
#[test]
#[ignore = "actual source Node batch DTO and identity oracle"]
fn native_batch_type_identity_and_owned_input_finding() {
    let rows = capture()["batchTypeRecords"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "batch DTO independent floor");
    for (i, row) in rows.iter().enumerate() {
        let v = SendTxBatchResponse {
            tx_hashes: shared(
                row["txHashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(text)
                    .collect(),
            ),
            predicted_execution_time_ms: opt_num(row, "predictedExecutionTimeMs"),
            volume_quota_remaining: row.get("volumeQuotaRemaining").map(|v| {
                if v.get("number").is_some() {
                    VolumeQuotaRemaining::Number(num(v))
                } else {
                    VolumeQuotaRemaining::String(text(v))
                }
            }),
        };
        match i {
            0 => {
                assert_eq!(v.predicted_execution_time_ms, None);
                assert_eq!(v.volume_quota_remaining, None);
                assert!(v.tx_hashes.borrow().is_empty());
            }
            1 => {
                assert_eq!(
                    v.predicted_execution_time_ms.unwrap().to_bits(),
                    (-0.0f64).to_bits(),
                    "decoded batch time signed zero"
                );
                match &v.volume_quota_remaining {
                    Some(VolumeQuotaRemaining::Number(q)) => assert_eq!(
                        q.to_bits(),
                        f64::INFINITY.to_bits(),
                        "decoded numeric quota"
                    ),
                    _ => panic!("numeric quota native variant"),
                }
                let a = "A".encode_utf16().chain([0xd800]).collect::<Vec<_>>();
                let b = "B👩".encode_utf16().collect::<Vec<_>>();
                assert_eq!(
                    *v.tx_hashes.borrow(),
                    vec![a.clone(), b, a],
                    "decoded batch hash order/duplicates/UTF16"
                );
            }
            2 => {
                assert_eq!(
                    v.predicted_execution_time_ms.unwrap().to_bits(),
                    0x7ff8000000000000,
                    "decoded batch time NaN bits"
                );
                assert_eq!(
                    v.volume_quota_remaining,
                    Some(VolumeQuotaRemaining::String(
                        "quota".encode_utf16().chain([0xdfff]).collect()
                    )),
                    "decoded string quota"
                );
                assert_eq!(
                    *v.tx_hashes.borrow(),
                    vec![vec![67u16], vec![68u16]],
                    "decoded batch hashes"
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(
            v.tx_hashes.borrow().len(),
            [0, 3, 2][i],
            "batch DTO independent native length"
        );
        identity(
            &json!({"txHashes":v.tx_hashes.borrow().iter().map(|v|t(v)).collect::<Vec<_>>(),"predictedExecutionTimeMs":on(v.predicted_execution_time_ms),"volumeQuotaRemaining":optional(v.volume_quota_remaining.as_ref().map(|v|match v{VolumeQuotaRemaining::Number(v)=>n(*v),VolumeQuotaRemaining::String(v)=>t(v)}))}),
            row,
            "batch DTO",
        );
    }
    let h = Harness::new(json!({}), "");
    assert_eq!(
        h.client.get_account_index(),
        BigInt::from(9007199254740993u64)
    );
    assert_eq!(h.client.get_api_key_index().to_bits(), 13f64.to_bits());
    assert_eq!(h.client.get_chain_id().to_bits(), 304f64.to_bits());
    let p = h.client.get_cancel_order_transaction(CancelOrderParams {
        order_id: 401.into(),
        market_id: 23.0,
    });
    let first = Rc::new(RefCell::new(None));
    let alias = first.clone();
    p.subscribe(move |v| {
        let v = v.unwrap();
        v.borrow_mut().cancel_details.order_id = 777.into();
        *alias.borrow_mut() = Some(v);
    });
    let check = first.clone();
    p.subscribe(move |v| {
        let v = v.unwrap();
        assert!(
            Rc::ptr_eq(&v, check.borrow().as_ref().unwrap()),
            "native fulfilled DTO same identity"
        );
        assert_eq!(
            v.borrow().cancel_details.order_id,
            BigInt::from(777),
            "native later observer sees DTO mutation"
        );
    });
    h.runtime.drain();
    assert!(first.borrow().is_some());
    assert_eq!(capture()["identity"][0]["sameObject"], true);
    let mut changed = order(&capture()["cases"][0]["args"][0]);
    let out = Rc::new(RefCell::new(None));
    let p = h.client.get_create_order_transaction(changed.clone());
    changed.price = 99.into();
    let target = out.clone();
    p.subscribe(move |v| *target.borrow_mut() = Some(v.unwrap()));
    h.runtime.drain();
    assert_eq!(
        out.borrow().as_ref().unwrap().borrow().order_details.price,
        BigInt::from(2401),
        "CHOSEN native owned input snapshot"
    );
    assert_eq!(changed.price, BigInt::from(99));
    assert_eq!(
        big(&capture()["ownershipFinding"]["sourcePrice"]),
        BigInt::from(99),
        "source caller mutation differs from native snapshot"
    );
    assert_eq!(
        big(&capture()["ownershipFinding"]["nativeSnapshotPrice"]),
        BigInt::from(2401)
    );
}
