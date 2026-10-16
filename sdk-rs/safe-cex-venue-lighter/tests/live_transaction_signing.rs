#[path = "support/signing_native.rs"]
mod native;
use native::*;
use safe_cex_venue_lighter::{
    http_transactions::*, transaction_builders::*, transaction_signing::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/transaction-signing-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(
            v["contentHash"], "54b06c02be882e060499cea951566f08d311ecd1554f76db9f8a66515260c65f",
            "signing live corpus content"
        );
        v
    })
}
struct State {
    mode: Value,
    trace: Vec<Value>,
    counts: BTreeMap<String, usize>,
    token: HostThrownValue,
}
#[derive(Clone)]
enum Outcome {
    Default,
    False,
    Returned,
    Thrown(HostThrownValue),
}
fn event(state: &Rc<RefCell<State>>, method: &str, args: Vec<Vec<u16>>, receiver: &str) -> Outcome {
    let mut s = state.borrow_mut();
    s.trace.push(json!({"method":method,"args":args.iter().map(|v|t(v)).collect::<Vec<_>>(),"receiver":receiver}));
    let c = s.counts.entry(method.into()).or_default();
    *c += 1;
    let count = *c;
    if s.mode["method"].as_str() == Some(method)
        && count == s.mode["occurrence"].as_u64().unwrap_or(1) as usize
    {
        match s.mode["kind"].as_str().unwrap() {
            "false" => Outcome::False,
            "returned" => Outcome::Returned,
            "thrown" => Outcome::Thrown(s.token.clone()),
            _ => unreachable!(),
        }
    } else {
        Outcome::Default
    }
}
fn failure(value: Outcome) -> Option<CryptoFailure> {
    match value {
        Outcome::Returned => Some(CryptoFailure::ReturnedError(
            "failure"
                .encode_utf16()
                .chain([0xd800, 0xd83d, 0xdc69])
                .collect(),
        )),
        Outcome::Thrown(v) => Some(CryptoFailure::Thrown(v)),
        Outcome::Default => None,
        Outcome::False => panic!("false only applies to ready"),
    }
}
#[derive(Debug, PartialEq)]
struct SymbolWitness(&'static str);
struct Recorder {
    state: Rc<RefCell<State>>,
    fp5: Box<Fp5Callback<'static>>,
}
impl Recorder {
    fn new(mode: Value) -> Self {
        let token = match mode["throwType"].as_str() {
            Some("string") => {
                HostThrownValue::new("thrown".encode_utf16().chain([0xd800]).collect::<Vec<_>>())
            }
            Some("null") => HostThrownValue::new(()),
            Some("symbol") => HostThrownValue::new(SymbolWitness("opaque")),
            _ => HostThrownValue::new(
                json!({"nameUnits":[0x540d,0x524d,0xd800],"messageUnits":[116,104,114,111,119,110,0xd800]}),
            ),
        };
        let state = Rc::new(RefCell::new(State {
            mode,
            trace: vec![],
            counts: BTreeMap::new(),
            token,
        }));
        let shared = state.clone();
        let fp5 = Box::new(move |input: &[u16]| {
            if let Some(e) = failure(event(&shared, "fp5", vec![input.to_vec()], "unbound")) {
                return Err(e);
            }
            let count = shared.borrow().counts["fp5"];
            Ok(match count {
                1 => "0xbase".encode_utf16().chain([0xd800]).collect(),
                2 => "0Xattrs".encode_utf16().chain([0xdfff]).collect(),
                _ => "fp5-final👩".encode_utf16().collect(),
            })
        });
        Self { state, fp5 }
    }
}
impl TransactionCrypto for Recorder {
    fn is_ready(&self) -> Result<bool, HostThrownValue> {
        match event(&self.state, "ready", vec![], "bound") {
            Outcome::Default => Ok(true),
            Outcome::False => Ok(false),
            Outcome::Thrown(v) => Err(v),
            Outcome::Returned => panic!("not a source bool"),
        }
    }
    fn poseidon2_hash(&self, input: &[u16]) -> CryptoCallResult {
        if let Some(e) = failure(event(&self.state, "hash", vec![input.to_vec()], "bound")) {
            return Err(e);
        }
        Ok(format!("hash-{}", self.state.borrow().counts["hash"])
            .encode_utf16()
            .chain([0xd800])
            .collect())
    }
    fn poseidon2_two_to_one(&self, left: &[u16], right: &[u16]) -> CryptoCallResult {
        if let Some(e) = failure(event(
            &self.state,
            "aggregate",
            vec![left.to_vec(), right.to_vec()],
            "bound",
        )) {
            return Err(e);
        }
        Ok(
            format!("aggregate-{}", self.state.borrow().counts["aggregate"])
                .encode_utf16()
                .chain([0xd800])
                .collect(),
        )
    }
    fn fp5_callback(&self) -> Result<Option<&Fp5Callback<'_>>, HostThrownValue> {
        match event(&self.state, "getFp5", vec![], "bound") {
            Outcome::Thrown(v) => return Err(v),
            Outcome::Default => {}
            _ => unreachable!(),
        };
        if self.state.borrow().mode["missingFp5"] == true {
            Ok(None)
        } else {
            Ok(Some(self.fp5.as_ref()))
        }
    }
    fn schnorr_sign(&self, input: &[u16], key: &[u16]) -> CryptoCallResult {
        if let Some(e) = failure(event(
            &self.state,
            "sign",
            vec![input.to_vec(), key.to_vec()],
            "bound",
        )) {
            return Err(e);
        }
        Ok("signature".encode_utf16().chain([0xdfff]).collect())
    }
}
#[test]
#[ignore = "actual source Node type oracle"]
fn http_transaction_native_identity() {
    let rows = capture()["typeRecords"].as_array().unwrap();
    assert_eq!(rows.len(), 44, "HTTP native independent inventory");
    let mut counts = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let record = row["record"].as_str().unwrap();
        *counts.entry(record).or_insert(0usize) += 1;
        let input = &row["input"];
        match record {
            "TxInfo" => {
                let v = TxInfo {
                    kind: kind(&input["type"]),
                    header: header(input),
                }
                .clone();
                identity(&header_native(v.kind(), v.header()), input, "TxInfo");
            }
            "LighterGroupedOrderDetails" => identity(
                &grouped_native(&group_detail(input).clone()),
                input,
                "GroupedDetails",
            ),
            _ => {
                let v = transaction(input).clone();
                if let AnyTxInfo::CreateGroupedOrders(g) = &v {
                    assert_eq!(
                        g.grouped_order_details.orders.len(),
                        if index < 22 { 0 } else { 4 },
                        "group DTO independent native array floor"
                    );
                }
                identity(&tx_native(&v), input, record);
            }
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("TxInfo", 14),
            ("LighterGroupedOrderDetails", 2),
            ("L2CreateOrderTxInfo", 2),
            ("L2CreateGroupedOrdersTxInfo", 2),
            ("L2CancelOrderTxInfo", 2),
            ("L2TransferTxInfo", 2),
            ("L2UpdateLeverageTxInfo", 2),
            ("L2ApproveIntegratorTxInfo", 2),
            ("L2WithdrawalTxInfo", 2),
            ("AnyTxInfo", 14)
        ]),
        "HTTP native executed inventory"
    );
}
#[test]
#[ignore = "actual source Node composition oracle"]
fn signing_native_results_and_calls() {
    let v = capture();
    let rows = v["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 52, "signing independent case inventory");
    let private_key = text(&v["privateKey"]);
    let config = &v["config"];
    let account = big(&config["accountIndex"]);
    let mut counts = BTreeMap::new();
    for row in rows {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let args = row["args"].as_array().unwrap();
        let recorder = Recorder::new(row["mode"].clone());
        let calls = Cell::new(0);
        let clock = || {
            let i = calls.get();
            calls.set(i + 1);
            1700000000000.0 + 7.0 * i as f64
        };
        let context = TransactionBuildContext {
            chain_id: num(&config["chainId"]),
            account_index: &account,
            api_key_index: num(&config["apiKeyIndex"]),
            now_millis: &clock,
        };
        let result:SigningResult<Value>=match method {
 "signTransaction"=>sign_transaction(&recorder,&private_key,&text(&args[0])).map(|v|json!({"signature":t(&v.signature),"txHash":t(&v.tx_hash)})),
 "signCreateOrderTransaction"=>{let attrs=args.get(1).filter(|v|!missing(Some(v))).map(text);sign_create_order_transaction(&recorder,&private_key,&text(&args[0]),attrs.as_deref()).map(|v|json!({"signature":t(&v.signature),"txHash":t(&v.tx_hash)}))},
 "buildCreateOrderTransaction"=>build_create_order_transaction(&context,&recorder,&private_key,&order(&args[0]),&big(&args[1])).map(|v|tx_native(&AnyTxInfo::CreateOrder(v))),
 "buildCreateGroupedOrdersData"=>build_create_grouped_orders_data(&context,&recorder,&group(&args[0]),&big(&args[1])).map(|v|{let expected_count=if group_kind(&args[0]["groupingType"])==safe_cex_venue_lighter::transaction_client_types::LighterGroupedOrderType::OneTriggersOco{3}else{2};assert_eq!(v.orders.len(),expected_count,"grouped builder native order count");json!({"messageHex":t(&v.message_hex),"l2AttributesMessageHex":optional(v.l2_attributes_message_hex.as_ref().map(|v|t(v))),"expiredAt":b(&v.expired_at),"orders":v.orders.iter().map(grouped_native).collect::<Vec<_>>(),"integratorAccountIndex":ob(&v.integrator_account_index),"integratorMakerFee":on(v.integrator_maker_fee),"integratorTakerFee":on(v.integrator_taker_fee)})}),
 "getAuthToken"=>{let encoding=match row["mode"]["encoder"].as_str(){Some("unavailable")=>AuthTokenEncoding::Unavailable,Some("buffer")=>AuthTokenEncoding::NodeBuffer,_=>AuthTokenEncoding::TextEncoder};get_auth_token(&context,&recorder,&private_key,&big(&args[0]),encoding).map(|v|t(&v))},
 _=>panic!("unknown source method"),};
        match result {
            Ok(native) => {
                assert!(
                    row["output"].get("ok").is_some(),
                    "{method} native unexpected success"
                );
                identity(&native, &row["output"]["ok"], method);
            }
            Err(TransactionSigningError::Generated(e)) => {
                assert_eq!(
                    e.name,
                    row["output"]["error"]["name"].as_str().unwrap(),
                    "{method} native generated error name"
                );
                assert_eq!(
                    e.message,
                    row["output"]["error"]["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                        .collect::<Vec<_>>(),
                    "{method} native generated UTF16 error"
                );
            }
            Err(TransactionSigningError::Thrown(token)) => {
                assert_eq!(
                    row["output"]["thrown"]["sameIdentity"], true,
                    "source preserves thrown identity"
                );
                assert!(
                    token.ptr_eq(&recorder.state.borrow().token),
                    "{method} native opaque thrown identity"
                );
                match row["output"]["thrown"]["type"].as_str().unwrap() {
                    "string" => assert_eq!(
                        token.value().downcast_ref::<Vec<u16>>().unwrap(),
                        &"thrown".encode_utf16().chain([0xd800]).collect::<Vec<_>>(),
                        "native thrown UTF16 value"
                    ),
                    "null" => assert!(
                        token.value().downcast_ref::<()>().is_some(),
                        "native null host marker"
                    ),
                    "symbol" => assert_eq!(
                        token.value().downcast_ref::<SymbolWitness>().unwrap(),
                        &SymbolWitness("opaque"),
                        "native symbol host marker"
                    ),
                    "error" => assert_eq!(
                        token.value().downcast_ref::<Value>().unwrap(),
                        &json!({"nameUnits":[0x540d,0x524d,0xd800],"messageUnits":[116,104,114,111,119,110,0xd800]}),
                        "native dynamic error payload"
                    ),
                    other => panic!("unknown source throw family {other}"),
                }
            }
        }
        assert_eq!(
            calls.get(),
            row["clockCalls"].as_u64().unwrap() as usize,
            "{method} clock calls"
        );
        assert_eq!(
            json!(recorder.state.borrow().trace),
            row["trace"],
            "{method} native crypto call arguments/order/receiver"
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("signTransaction", 10),
            ("signCreateOrderTransaction", 15),
            ("buildCreateOrderTransaction", 2),
            ("buildCreateGroupedOrdersData", 19),
            ("getAuthToken", 6)
        ]),
        "signing native executed inventory"
    );
}
