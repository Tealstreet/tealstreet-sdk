use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    http_models::*, http_number_helpers::*, sdk_protocol::*, sdk_validation::*,
    transaction_signing::HostThrownValue,
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, process::Command, rc::Rc};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/sdk-primitives-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(
            v["contentHash"], "db94b17767b5fe83f5fbce0e53d0aff1d716a4cb5e3164544ed72977d58da3d5",
            "independent SDK primitive corpus"
        );
        v
    })
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn bigint(v: &Value) -> BigInt {
    v["decimal"].as_str().unwrap().parse().unwrap()
}
fn text(v: &Value) -> Vec<u16> {
    units(&v["units"])
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn absent(v: Option<&Value>) -> bool {
    v.is_none_or(|v| v.get("undefined") == Some(&json!(true)))
}
fn optnum(v: &Value, k: &str) -> Option<f64> {
    v.get(k).filter(|v| !absent(Some(v))).map(number)
}
fn opttext(v: &Value, k: &str) -> Option<Vec<u16>> {
    v.get(k).filter(|v| !absent(Some(v))).map(text)
}
fn value(v: &Value) -> ValidationValue {
    match v["kind"].as_str().unwrap() {
        "undefined" => ValidationValue::Undefined,
        "null" => ValidationValue::Null,
        "boolean" => ValidationValue::Boolean(v["value"].as_bool().unwrap()),
        "number" => ValidationValue::Number(number(v)),
        "bigint" => ValidationValue::BigInt(bigint(v)),
        "string" => ValidationValue::String(text(v)),
        "symbol" => ValidationValue::Symbol,
        "function" => ValidationValue::Function,
        "object" => ValidationValue::Object,
        _ => panic!("unknown primitive kind"),
    }
}
fn token() -> HostThrownValue {
    HostThrownValue::new("host".encode_utf16().chain([0xd800]).collect::<Vec<_>>())
}
fn assert_result(result: ValidationResult, expected: &Value, token: &HostThrownValue, label: &str) {
    match result {
        Ok(()) => assert_eq!(
            expected["kind"], "ok",
            "{label} native validation result kind"
        ),
        Err(ValidationFailure::Validation(e)) => {
            assert_eq!(
                expected["kind"], "validation",
                "{label} native validation result kind"
            );
            assert_eq!(
                e.name,
                units(&expected["name"]),
                "{label} native ValidationError name"
            );
            assert_eq!(
                e.message,
                units(&expected["message"]),
                "{label} native ValidationError UTF16 message"
            );
        }
        Err(ValidationFailure::Builtin(e)) => {
            assert_eq!(
                expected["kind"], "builtin",
                "{label} native validation result kind"
            );
            assert_eq!(
                u(e.kind.name()),
                units(&expected["name"]),
                "{label} native builtin name"
            );
            assert_eq!(
                e.message_utf16,
                units(&expected["message"]),
                "{label} native builtin UTF16 message"
            );
        }
        Err(ValidationFailure::Thrown(e)) => {
            assert_eq!(
                expected["kind"], "thrown",
                "{label} native validation result kind"
            );
            assert!(e.ptr_eq(token), "{label} native host throw identity");
            assert_eq!(
                e.value().downcast_ref::<Vec<u16>>().unwrap(),
                &"host".encode_utf16().chain([0xd800]).collect::<Vec<_>>(),
                "native host throw payload"
            );
        }
    }
}
#[test]
#[ignore = "actual TypeScript SDK scalar validator oracle"]
fn scalar_validator_native_results() {
    let v = capture();
    assert_eq!(
        MAX_ACCOUNT_INDEX.to_bits(),
        number(&v["constants"]["MaxAccountIndex"]).to_bits()
    );
    assert_eq!(MAX_API_KEY_INDEX.to_bits(), 255.0f64.to_bits());
    assert_eq!(MAX_NONCE.to_bits(), 4294967295.0f64.to_bits());
    assert_eq!(*MIN_ORDER_SIZE, BigInt::from(1));
    assert_eq!(
        *MAX_ORDER_SIZE,
        "18446744073709551615".parse::<BigInt>().unwrap()
    );
    let rows = v["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 310, "independent scalar inventory");
    let mut counts = BTreeMap::new();
    for row in rows {
        let name = row["method"].as_str().unwrap();
        *counts.entry(name).or_insert(0usize) += 1;
        let args = row["args"].as_array().unwrap();
        let input = value(&args[0]);
        let thrown = token();
        let calls = RefCell::new(0usize);
        let clock = || {
            *calls.borrow_mut() += 1;
            if row["clockThrows"] == true {
                Err(thrown.clone())
            } else {
                Ok(row.get("clock").map(number).unwrap_or(1700000000987.0))
            }
        };
        let result = match name {
            "validateAccountIndex" => validate_account_index(&input),
            "validateApiKeyIndex" => validate_api_key_index(&input),
            "validateNonce" => validate_nonce(&input),
            "validateOrderId" => validate_order_id(&input),
            "validateMarketId" => validate_market_id(&input),
            "validatePrice" => validate_price(&input),
            "validateAmount" => validate_amount(&input),
            "validatePrivateKey" => validate_private_key(&input),
            "validatePublicKey" => validate_public_key(&input),
            "validateAddress" => validate_address(&input),
            "validateChainId" => validate_chain_id(&input),
            "validateDeadline" => validate_deadline(&input, args.get(1).map(number), &clock),
            _ => panic!("unexpected source method"),
        };
        assert_result(result, &row["output"], &thrown, name);
        assert_eq!(
            *calls.borrow(),
            row["clocks"].as_u64().unwrap() as usize,
            "{name} native clock count"
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("validateAccountIndex", 24),
            ("validateNonce", 24),
            ("validateOrderId", 24),
            ("validatePrice", 24),
            ("validateAmount", 24),
            ("validateApiKeyIndex", 25),
            ("validateMarketId", 25),
            ("validateChainId", 25),
            ("validatePrivateKey", 27),
            ("validatePublicKey", 27),
            ("validateAddress", 27),
            ("validateDeadline", 34)
        ]),
        "independent executed scalar method floor"
    );
}
struct Reader<'a> {
    row: &'a Value,
    counts: RefCell<BTreeMap<String, usize>>,
    trace: RefCell<Vec<String>>,
    token: HostThrownValue,
}
impl OrderParamsReader for Reader<'_> {
    fn get(&self, field: &str) -> Result<ValidationValue, HostThrownValue> {
        self.trace.borrow_mut().push(field.into());
        let mut counts = self.counts.borrow_mut();
        let c = counts.entry(field.into()).or_default();
        *c += 1;
        let n = *c;
        if self.row["throwAt"]["field"] == field
            && self.row["throwAt"]["occurrence"].as_u64() == Some(n as u64)
        {
            return Err(self.token.clone());
        }
        if let Some(seq) = self.row["sequences"][field].as_array() {
            Ok(value(&seq[(n - 1).min(seq.len() - 1)]))
        } else {
            Ok(self.row["fields"]
                .get(field)
                .map(value)
                .unwrap_or(ValidationValue::Undefined))
        }
    }
}
#[test]
#[ignore = "actual TypeScript getter/short-circuit oracle"]
fn order_validator_native_reads() {
    let rows = capture()["orderCases"].as_array().unwrap();
    assert_eq!(rows.len(), 44, "independent getter case floor");
    let mut kinds = BTreeMap::new();
    for row in rows {
        let reader = Reader {
            row,
            counts: RefCell::new(BTreeMap::new()),
            trace: RefCell::new(vec![]),
            token: token(),
        };
        let result = validate_order_params(&reader);
        assert_result(result, &row["output"], &reader.token, "order params");
        *kinds
            .entry(row["output"]["kind"].as_str().unwrap())
            .or_insert(0usize) += 1;
        let expected = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            *reader.trace.borrow(),
            expected,
            "native getter access order/short-circuit"
        );
    }
    assert_eq!(
        kinds,
        BTreeMap::from([("ok", 3), ("validation", 32), ("thrown", 9)]),
        "independent getter outcome floor"
    );
}
#[test]
#[ignore = "actual TypeScript error fields and safe-number helpers"]
fn error_and_safe_integer_native_values() {
    let v = capture();
    let errors = v["errorCases"].as_array().unwrap();
    assert_eq!(errors.len(), 3, "independent error constructor floor");
    for row in errors {
        let e = ValidationError::new(units(&row["message"]));
        assert_eq!(e.name, u("ValidationError"));
        assert_eq!(e.name, units(&row["name"]), "native error name");
        assert_eq!(
            e.message,
            units(&row["nativeMessage"]),
            "native constructor UTF16 message"
        );
        assert_eq!(row["isError"], true);
        assert_eq!(row["isValidationError"], true); // native class rows explicitly diverge on prototype and V8 stack
    }
    let helpers = v["helperCases"].as_array().unwrap();
    assert_eq!(helpers.len(), 9, "independent helper floor");
    for row in helpers {
        let n = bigint(&row["value"]);
        assert_eq!(
            is_safe_integer_bigint(&n),
            row["safe"].as_bool().unwrap(),
            "native bigint safe-range predicate"
        );
        match to_safe_number(&n, &units(&row["field"])) {
            Ok(n) => {
                assert_eq!(
                    row["output"]["kind"], "ok",
                    "safe-number native result kind"
                );
                assert_eq!(
                    n.to_bits(),
                    number(&row["output"]["value"]).to_bits(),
                    "native safe-number bits"
                );
            }
            Err(e) => {
                assert_eq!(row["output"]["kind"], "error");
                assert_eq!(u(e.name), units(&row["output"]["name"]));
                assert_eq!(
                    e.message,
                    units(&row["output"]["message"]),
                    "safe-number native UTF16 diagnostic"
                );
            }
        }
    }
}
#[derive(Debug, Clone)]
enum Payload {
    Undefined,
    Number(f64),
    BigInt(BigInt),
    Text(Vec<u16>),
    Opaque(Rc<&'static str>),
}
fn payloads(
    v: &Value,
    symbol: &Rc<&'static str>,
    function: &Rc<&'static str>,
) -> Utf16Record<Payload> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            let p = if absent(Some(v)) {
                Payload::Undefined
            } else {
                match v["kind"].as_str().unwrap() {
                    "number" => Payload::Number(number(v)),
                    "bigint" => Payload::BigInt(bigint(v)),
                    "string" => Payload::Text(text(v)),
                    "symbol" => Payload::Opaque(symbol.clone()),
                    "function" => Payload::Opaque(function.clone()),
                    _ => panic!("unsupported fixture payload"),
                }
            };
            (u(k), p)
        })
        .collect()
}
fn assert_payloads(
    v: &Utf16Record<Payload>,
    symbol: &Rc<&'static str>,
    function: &Rc<&'static str>,
) {
    assert_eq!(v.len(), 6, "independent opaque record field count");
    assert!(matches!(v.get(&u("undefined")), Some(Payload::Undefined)));
    match v.get(&u("nan")).unwrap() {
        Payload::Number(n) => assert_eq!(
            n.to_bits(),
            0x7ff8000000000000,
            "native opaque numeric bits"
        ),
        _ => panic!("numeric payload"),
    };
    match v.get(&u("big")).unwrap() {
        Payload::BigInt(n) => assert_eq!(n, &(BigInt::from(1) << 210), "native opaque bigint"),
        _ => panic!("bigint payload"),
    };
    match v.get(&u("text")).unwrap() {
        Payload::Text(v) => assert_eq!(
            v,
            &"opaque".encode_utf16().chain([0xd800]).collect::<Vec<_>>(),
            "native opaque UTF16"
        ),
        _ => panic!("string payload"),
    };
    for (k, want) in [("symbol", symbol), ("function", function)] {
        match v.get(&u(k)).unwrap() {
            Payload::Opaque(h) => assert!(Rc::ptr_eq(h, want), "native generic opaque identity"),
            _ => panic!("opaque payload"),
        }
    }
}
fn api_key(v: &Value) -> ApiKey {
    ApiKey {
        index: number(&v["index"]),
        public_key: text(&v["publicKey"]),
        active: v["active"]["value"].as_bool().unwrap(),
        created_at: optnum(v, "createdAt"),
        last_used_at: optnum(v, "lastUsedAt"),
    }
}
fn assert_key(v: &ApiKey, i: usize) {
    assert_eq!(
        v.index.to_bits(),
        match i {
            0 => 17.25f64.to_bits(),
            1 => (-0.0f64).to_bits(),
            2 => f64::INFINITY.to_bits(),
            _ => panic!("key fixture"),
        },
        "native API key index bits"
    );
    assert_eq!(
        v.public_key,
        format!("public-{i}")
            .encode_utf16()
            .chain([0xd800, 0xd83d, 0xdc69])
            .collect::<Vec<_>>(),
        "native API key UTF16"
    );
    assert_eq!(v.active, i != 2);
    assert_eq!(
        v.created_at.map(f64::to_bits),
        match i {
            0 => None,
            1 => Some(0x7ff8000000000000),
            2 => Some(f64::NEG_INFINITY.to_bits()),
            _ => unreachable!(),
        }
    );
    assert_eq!(
        v.last_used_at.map(f64::to_bits),
        match i {
            0 => None,
            1 => Some((-0.0f64).to_bits()),
            2 => Some(9007199254740992f64.to_bits()),
            _ => unreachable!(),
        }
    );
}
fn error(v: &Value, s: &Rc<&'static str>, f: &Rc<&'static str>) -> ApiErrorResponse<Payload> {
    ApiErrorResponse {
        code: text(&v["code"]),
        message: text(&v["message"]),
        details: v.get("details").map(|v| payloads(v, s, f)),
        request_id: opttext(v, "requestId"),
    }
}
fn assert_error(
    v: &ApiErrorResponse<Payload>,
    i: usize,
    s: &Rc<&'static str>,
    f: &Rc<&'static str>,
) {
    assert_eq!(
        v.code,
        "code-A".encode_utf16().chain([0xd800]).collect::<Vec<_>>()
    );
    assert_eq!(v.message, u("message-B👩"));
    assert_eq!(
        v.request_id,
        if i == 0 {
            None
        } else {
            Some("request-C".encode_utf16().chain([0xdfff]).collect())
        }
    );
    if i == 0 {
        assert!(v.details.is_none())
    } else {
        assert_payloads(v.details.as_ref().unwrap(), s, f)
    }
}
#[test]
#[ignore = "actual TypeScript HTTP schema/native fixture oracle"]
fn http_models_native_fields() {
    let rows = capture()["models"].as_array().unwrap();
    assert_eq!(rows.len(), 27, "independent HTTP model floor");
    let s = Rc::new("symbol");
    let f = Rc::new("function");
    let mut counts = BTreeMap::new();
    for row in rows {
        let record = row["record"].as_str().unwrap();
        *counts.entry(record).or_insert(0usize) += 1;
        let i = row["variant"].as_u64().unwrap() as usize;
        let v = &row["input"];
        match record {
            "LighterApiClientConfig" => {
                let x = LighterApiClientConfig {
                    base_url: text(&v["baseUrl"]),
                    timeout: optnum(v, "timeout"),
                    debug: v.get("debug").map(|v| v["value"].as_bool().unwrap()),
                    headers: v.get("headers").map(|v| {
                        v.as_array()
                            .unwrap()
                            .iter()
                            .map(|entry| (units(&entry["key"]), text(&entry["value"])))
                            .collect()
                    }),
                };
                assert_eq!(
                    x.base_url,
                    "url"
                        .encode_utf16()
                        .chain([0xd800, 0xd83d, 0xdc69])
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    x.timeout.map(f64::to_bits),
                    match i {
                        0 => None,
                        1 => Some((-0.0f64).to_bits()),
                        2 => Some(f64::INFINITY.to_bits()),
                        _ => unreachable!(),
                    }
                );
                assert_eq!(x.debug, if i == 0 { None } else { Some(i == 1) });
                if i == 0 {
                    assert!(x.headers.is_none())
                } else {
                    let h = x.headers.as_ref().unwrap();
                    assert_eq!(h.len(), 3, "independent native header count");
                    assert_eq!(
                        h[&u("2")],
                        "header-A"
                            .encode_utf16()
                            .chain([0xd800])
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(h[&u("10")], u("header-B👩"));
                    assert_eq!(
                        h[&"key".encode_utf16().chain([0xdfff]).collect::<Vec<_>>()],
                        u("value-C")
                    );
                }
            }
            "ApiKey" => assert_key(&api_key(v), i),
            "TransferFeeInfo" => {
                let x = TransferFeeInfo {
                    fee: bigint(&v["fee"]),
                    min_amount: bigint(&v["minAmount"]),
                    max_amount: bigint(&v["maxAmount"]),
                    fee_percentage: optnum(v, "feePercentage"),
                };
                assert_eq!(x.fee, BigInt::from(1) << 200);
                assert_eq!(x.min_amount, -(BigInt::from(1) << 201usize));
                assert_eq!(x.max_amount, BigInt::from(1) << 202);
                assert_eq!(
                    x.fee_percentage.map(f64::to_bits),
                    match i {
                        0 => None,
                        1 => Some(0x7ff8000000000000),
                        2 => Some((-0.0f64).to_bits()),
                        _ => unreachable!(),
                    }
                );
            }
            "ApiErrorResponse" => assert_error(&error(v, &s, &f), i, &s, &f),
            "ApiResponse" => {
                let x = ApiResponse {
                    success: v["success"]["value"].as_bool().unwrap(),
                    data: v.get("data").map(|v| payloads(v, &s, &f)),
                    error: v.get("error").map(|v| error(v, &s, &f)),
                    timestamp: optnum(v, "timestamp"),
                };
                assert_eq!(x.success, i == 1);
                assert_eq!(
                    x.timestamp.map(f64::to_bits),
                    match i {
                        0 => None,
                        1 => Some((-0.0f64).to_bits()),
                        2 => Some(0x7ff8000000000000),
                        _ => unreachable!(),
                    }
                );
                if i == 0 {
                    assert!(x.data.is_none());
                    assert!(x.error.is_none())
                } else {
                    assert_payloads(x.data.as_ref().unwrap(), &s, &f);
                    assert_error(x.error.as_ref().unwrap(), i, &s, &f);
                }
            }
            "GetNonceResponse" => {
                let x = GetNonceResponse {
                    nonce: text(&v["nonce"]),
                    next_nonce: text(&v["nextNonce"]),
                };
                assert_eq!(
                    x.nonce,
                    "nonce-A".encode_utf16().chain([0xd800]).collect::<Vec<_>>(),
                    "native malformed nonce remains UTF16 string"
                );
                assert_eq!(x.next_nonce, u("next-B👩"));
            }
            "GetApiKeysResponse" => {
                let x = GetApiKeysResponse {
                    api_keys: v["apiKeys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(api_key)
                        .collect(),
                };
                assert_eq!(
                    x.api_keys.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent native key-array count"
                );
                if i != 0 {
                    for (v, i) in x.api_keys.iter().zip([2, 1, 2, 0]) {
                        assert_key(v, i)
                    }
                }
            }
            "SendTxResponse" => {
                let status = match String::from_utf16(&text(&v["status"])).unwrap().as_str() {
                    "pending" => SendTxStatus::Pending,
                    "confirmed" => SendTxStatus::Confirmed,
                    "failed" => SendTxStatus::Failed,
                    _ => panic!("status"),
                };
                let x = SendTxResponse {
                    tx_hash: text(&v["txHash"]),
                    status,
                    block_number: optnum(v, "blockNumber"),
                };
                assert_eq!(
                    x.status,
                    [
                        SendTxStatus::Pending,
                        SendTxStatus::Confirmed,
                        SendTxStatus::Failed
                    ][i],
                    "native literal status variant"
                );
                assert_eq!(
                    x.tx_hash,
                    "hash-A"
                        .encode_utf16()
                        .chain([0xd800, 0xd83d, 0xdc69])
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    x.block_number.map(f64::to_bits),
                    match i {
                        0 => None,
                        1 => Some(9007199254740992f64.to_bits()),
                        2 => Some(0x7ff8000000000000),
                        _ => unreachable!(),
                    }
                );
            }
            "GetTransferFeeResponse" => {
                let x = GetTransferFeeResponse {
                    fee: text(&v["fee"]),
                    min_amount: text(&v["minAmount"]),
                    max_amount: text(&v["maxAmount"]),
                    fee_percentage: optnum(v, "feePercentage"),
                };
                assert_eq!(
                    x.fee,
                    "fee-A".encode_utf16().chain([0xd800]).collect::<Vec<_>>()
                );
                assert_eq!(x.min_amount, u("min-B👩"));
                assert_eq!(
                    x.max_amount,
                    "max-C".encode_utf16().chain([0xdfff]).collect::<Vec<_>>(),
                    "native malformed fee remains UTF16 string"
                );
                assert_eq!(
                    x.fee_percentage.map(f64::to_bits),
                    match i {
                        0 => None,
                        1 => Some(f64::INFINITY.to_bits()),
                        2 => Some((-0.0f64).to_bits()),
                        _ => unreachable!(),
                    }
                );
            }
            _ => panic!("unexpected HTTP record"),
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("LighterApiClientConfig", 3),
            ("ApiKey", 3),
            ("TransferFeeInfo", 3),
            ("ApiErrorResponse", 3),
            ("ApiResponse", 3),
            ("GetNonceResponse", 3),
            ("GetApiKeysResponse", 3),
            ("SendTxResponse", 3),
            ("GetTransferFeeResponse", 3)
        ]),
        "independent executed HTTP record floor"
    );
}
