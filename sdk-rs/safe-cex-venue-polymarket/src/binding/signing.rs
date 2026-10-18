//! Native bindings for ethers and the pinned clob-client-v2 order builder.
//! SDK algorithm reference: @polymarket/clob-client-v2/dist/order-builder and
//! dist/order-utils/exchangeOrderBuilderV2.js. See SDK_LICENSE.txt.
use super::{Shared, error, text};
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::{eip712, ethereum};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    scope::scope_of,
    value::{Js, Thrown, arg, from_json, to_json},
};
use std::rc::{Rc, Weak};

const ZERO: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";
const ORDER_TYPE: &str = "Order(uint256 salt,address maker,address signer,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint8 side,uint8 signatureType,uint256 timestamp,bytes32 metadata,bytes32 builder)";

fn bytes(v: &Js) -> Result<Vec<u8>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error("Expected bytes"))?
        .iter()
        .map(|v| v.to_number().map(|n| n as u8))
        .collect()
}
fn byte_value(b: &[u8]) -> Js {
    Js::array(b.iter().map(|b| Js::Number(*b as f64)).collect())
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>, Thrown> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err(error("Invalid hex bytes"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| error("Invalid hex bytes")))
        .collect()
}
pub(crate) fn install(g: &Js, weak: Weak<Shared>) {
    let ethers = Js::object(vec![]);
    let def = |k: &str, v: Js| ethers.define(k, v).expect("fresh ethers bindings");
    def(
        "Wallet",
        Js::constructor("Wallet", None, |_, a| wallet(arg(&a, 0))),
    );
    def(
        "decodeBase64",
        Js::function("decodeBase64", |_, a| {
            Ok(byte_value(&safe_cex_crypto::node_base64_decode(&text(
                &arg(&a, 0),
            )?)))
        }),
    );
    def(
        "toUtf8Bytes",
        Js::function("toUtf8Bytes", |_, a| {
            Ok(byte_value(text(&arg(&a, 0))?.as_bytes()))
        }),
    );
    def(
        "computeHmac",
        Js::function("computeHmac", |_, a| {
            if text(&arg(&a, 0))? != "sha256" {
                return Err(error("Unsupported HMAC algorithm"));
            }
            Ok(Js::str(&format!(
                "0x{}",
                safe_cex_crypto::hmac_sha256(
                    &bytes(&arg(&a, 1))?,
                    &bytes(&arg(&a, 2))?,
                    safe_cex_crypto::Encoding::Hex
                )
            )))
        }),
    );
    def(
        "getBytes",
        Js::function("getBytes", |_, a| {
            Ok(byte_value(&unhex(&text(&arg(&a, 0))?)?))
        }),
    );
    def(
        "encodeBase64",
        Js::function("encodeBase64", |_, a| {
            Ok(Js::str(&STANDARD.encode(bytes(&arg(&a, 0))?)))
        }),
    );
    g.define("ethers", ethers).expect("fresh globals");
    let sdk = Js::object(vec![]);
    sdk.define(
        "OrderBuilder",
        Js::constructor("OrderBuilder", None, move |_, a| {
            let weak = weak.clone();
            let signer = arg(&a, 0);
            let chain = arg(&a, 1);
            let signature_type = arg(&a, 2);
            let funder = arg(&a, 3);
            Ok(Js::object(vec![(
                "buildOrder",
                Js::function("buildOrder", move |_, a| {
                    let rt = scope_of(&weak)?;
                    let signer = signer.clone();
                    let chain = chain.clone();
                    let signature_type = signature_type.clone();
                    let funder = funder.clone();
                    let clock = rt.shared.runtime.clone();
                    let random = clock.clone();
                    let builder = Builder {
                        runtime: rt.exchange_runtime(),
                        signer,
                        chain,
                        signature_type,
                        funder,
                        now: Rc::new(move || clock.now_ms()),
                        random: Rc::new(move || random.random()),
                    };
                    let task = build_order(builder, arg(&a, 0), arg(&a, 1), arg(&a, 2));
                    Ok(rt.spawn(Box::pin(task)))
                }),
            )]))
        }),
    )
    .expect("fresh SDK");
    sdk.define(
        "isV2Order",
        Js::function("isV2Order", |_, a| {
            Ok(Js::Bool(arg(&a, 0).has_own("timestamp")))
        }),
    )
    .expect("fresh SDK");
    g.define("SDK", sdk).expect("fresh globals");
    g.define(
        "orderToJsonV2",
        Js::function("orderToJsonV2", |_, a| {
            let o = arg(&a, 0);
            let order = Js::object(vec![("salt", Js::Number(o.get("salt")?.parse_float()?))]);
            for k in [
                "maker",
                "signer",
                "taker",
                "tokenId",
                "makerAmount",
                "takerAmount",
                "side",
                "signatureType",
                "timestamp",
                "expiration",
                "metadata",
                "builder",
                "signature",
            ] {
                order.define(k, o.get(k)?)?;
            }
            Ok(Js::object(vec![
                ("deferExec", Js::Bool(false)),
                ("postOnly", Js::Bool(false)),
                ("order", order),
                ("owner", arg(&a, 1)),
                ("orderType", arg(&a, 2)),
            ]))
        }),
    )
    .expect("fresh globals");
}
fn wallet(key: Js) -> Result<Js, Thrown> {
    let key = text(&key)?;
    let wallet = safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&key)
        .map_err(|e| error(&e))?;
    let address = wallet.address().to_checksum(None);
    let get = address.clone();
    Ok(Js::object(vec![
        ("address", Js::str(&address)),
        (
            "getAddress",
            Js::function("getAddress", move |_, _| Ok(Js::str(&get))),
        ),
        (
            "signTypedData",
            Js::function("signTypedData", move |_, a| {
                let types = to_json(&arg(&a, 1))?;
                let declarations = types.as_object().ok_or_else(|| error("Missing types"))?;
                let primary = declarations
                    .keys()
                    .find(|k| {
                        *k != "EIP712Domain"
                            && !declarations
                                .values()
                                .filter_map(|v| v.as_array())
                                .flatten()
                                .any(|f| f["type"].as_str() == Some(k.as_str()))
                    })
                    .ok_or_else(|| error("Missing primary type"))?;
                let mut domain = to_json(&arg(&a, 0))?;
                if let Some(chain) = domain.get_mut("chainId") {
                    integer_text(chain)?;
                }
                let mut message = to_json(&arg(&a, 2))?;
                integer_fields(&types, primary, &mut message)?;
                let data = serde_json::json!({"domain":domain,"types":types,"primaryType":primary,"message":message});
                eip712::sign_typed_data(&data.to_string(), &key)
                    .map(|s| Js::str(&s))
                    .map_err(|e| error(&e))
            }),
        ),
    ]))
}
fn integer_text(value: &mut serde_json::Value) -> Result<(), Thrown> {
    if let Some(n) = value.as_f64() {
        if !n.is_finite() || n.fract() != 0. {
            return Err(error("Invalid typed-data integer"));
        }
        *value = serde_json::Value::String(safe_cex_num::js_number_string(n));
    }
    Ok(())
}
fn integer_fields(
    types: &serde_json::Value,
    kind: &str,
    value: &mut serde_json::Value,
) -> Result<(), Thrown> {
    if kind.starts_with("uint") || kind.starts_with("int") {
        return integer_text(value);
    }
    if let Some(fields) = types.get(kind).and_then(|v| v.as_array()) {
        for f in fields {
            if let (Some(name), Some(kind)) = (f["name"].as_str(), f["type"].as_str()) {
                if let Some(value) = value.get_mut(name) {
                    integer_fields(types, kind, value)?;
                }
            }
        }
    }
    Ok(())
}
fn decimal_places(n: f64) -> usize {
    let text = safe_cex_num::js_number_string(n);
    text.split_once('.').map_or(0, |(_, d)| d.len())
}
fn down(n: f64, d: usize) -> f64 {
    if decimal_places(n) <= d {
        n
    } else {
        let p = 10f64.powi(d as i32);
        (n * p).floor() / p
    }
}
fn up(n: f64, d: usize) -> f64 {
    if decimal_places(n) <= d {
        n
    } else {
        let p = 10f64.powi(d as i32);
        (n * p).ceil() / p
    }
}
fn normal(n: f64, d: usize) -> f64 {
    if decimal_places(n) <= d {
        n
    } else {
        let p = 10f64.powi(d as i32);
        safe_cex_num::math_round((n + f64::EPSILON) * p) / p
    }
}
fn units(n: f64) -> Result<String, Thrown> {
    // SDK rounds raw amounts to at most six decimal places before parseUnits.
    let s = safe_cex_num::js_number_string(n);
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    if frac.len() > 6 || int.contains('e') {
        return Err(error("Invalid decimal for collateral units"));
    }
    let full = format!("{int}{frac:0<6}");
    let trimmed = full.trim_start_matches('0');
    Ok(if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.into()
    })
}
fn order_fields() -> serde_json::Value {
    serde_json::json!([
        {"name":"salt","type":"uint256"},{"name":"maker","type":"address"},{"name":"signer","type":"address"},{"name":"tokenId","type":"uint256"},{"name":"makerAmount","type":"uint256"},{"name":"takerAmount","type":"uint256"},{"name":"side","type":"uint8"},{"name":"signatureType","type":"uint8"},{"name":"timestamp","type":"uint256"},{"name":"metadata","type":"bytes32"},{"name":"builder","type":"bytes32"}
    ])
}
struct Builder {
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
    signer: Js,
    chain: Js,
    signature_type: Js,
    funder: Js,
    now: Rc<dyn Fn() -> f64>,
    random: Rc<dyn Fn() -> f64>,
}
async fn build_order(builder: Builder, user: Js, options: Js, version: Js) -> Result<Js, Thrown> {
    let Builder {
        runtime,
        signer,
        chain,
        signature_type,
        funder,
        now,
        random,
    } = builder;
    if version.to_number()? != 2. {
        return Err(error("Only CLOB V2 orders are bound"));
    }
    let address = await_js(&runtime, signer.call_method("getAddress", vec![])?).await?;
    let maker = if funder.is_undefined() {
        address.clone()
    } else {
        funder
    };
    let order_signer = if signature_type.strict_equals(&Js::Number(3.)) {
        maker.clone()
    } else {
        address
    };
    let tick = text(&options.get("tickSize")?)?;
    let price_digits = match tick.as_str() {
        "0.1" => 1,
        "0.01" => 2,
        "0.005" | "0.001" => 3,
        "0.0025" | "0.0001" => 4,
        _ => return Err(error("Unsupported SDK tick size")),
    };
    let price = normal(user.get("price")?.to_number()?, price_digits);
    let size = down(user.get("size")?.to_number()?, 2);
    let amount_digits = price_digits + 2;
    let mut amount = size * price;
    if decimal_places(amount) > amount_digits {
        amount = up(amount, amount_digits + 4);
        if decimal_places(amount) > amount_digits {
            amount = down(amount, amount_digits);
        }
    }
    let buy = user.get("side")?.strict_equals(&Js::str("BUY"));
    let (maker_amount, taker_amount) = if buy {
        (units(amount)?, units(size)?)
    } else {
        (units(size)?, units(amount)?)
    };
    let salt = safe_cex_num::js_number_string(safe_cex_num::math_round(random() * now()));
    let order = Js::object(vec![
        ("salt", Js::str(&salt)),
        ("maker", maker),
        ("signer", order_signer),
        ("tokenId", user.get("tokenID")?),
        ("makerAmount", Js::str(&maker_amount)),
        ("takerAmount", Js::str(&taker_amount)),
        ("side", Js::str(if buy { "BUY" } else { "SELL" })),
        ("signatureType", signature_type.clone()),
        ("metadata", Js::str(ZERO)),
        ("builder", Js::str(ZERO)),
        ("timestamp", Js::str(&safe_cex_num::js_number_string(now()))),
        ("expiration", Js::str("0")),
    ]);
    let contracts = crate::constants::POLYMARKET_CONFIG.contracts;
    let contract = if options.get("negRisk")?.truthy() {
        contracts.neg_risk_exchange_v2
    } else {
        contracts.exchange_v2
    };
    let domain = serde_json::json!({"name":"Polymarket CTF Exchange","version":"2","chainId":chain.to_number()?,"verifyingContract":contract});
    let message = to_json(&order)?;
    let mut message = message;
    message["side"] = serde_json::json!(if buy { 0 } else { 1 });
    let mut types = serde_json::json!({"Order":order_fields()});
    let signature = if signature_type.strict_equals(&Js::Number(3.)) {
        let contents_hash = contents_hash(&message)?;
        let domain_hash = domain_hash(contract, chain.to_number()? as u64)?;
        types["TypedDataSign"] = serde_json::json!([{"name":"contents","type":"Order"},{"name":"name","type":"string"},{"name":"version","type":"string"},{"name":"chainId","type":"uint256"},{"name":"verifyingContract","type":"address"},{"name":"salt","type":"bytes32"}]);
        let value = serde_json::json!({"contents":message,"name":"DepositWallet","version":"1","chainId":chain.to_number()?,"verifyingContract":message["signer"],"salt":ZERO});
        let inner = await_js(
            &runtime,
            signer.call_method(
                "_signTypedData",
                vec![from_json(&domain), from_json(&types), from_json(&value)],
            )?,
        )
        .await?;
        format!(
            "0x{}{}{}{}00ba",
            text(&inner)?.trim_start_matches("0x"),
            hex(&domain_hash),
            hex(&contents_hash),
            hex(ORDER_TYPE.as_bytes())
        )
    } else {
        text(
            &await_js(
                &runtime,
                signer.call_method(
                    "_signTypedData",
                    vec![from_json(&domain), from_json(&types), from_json(&message)],
                )?,
            )
            .await?,
        )?
    };
    order.define("signature", Js::str(&signature))?;
    Ok(order)
}
fn word(v: &serde_json::Value, kind: &str) -> Result<[u8; 32], Thrown> {
    if kind == "bytes32" {
        let b = unhex(v.as_str().ok_or_else(|| error("Expected bytes32"))?)?;
        return b.try_into().map_err(|_| error("Expected 32 bytes"));
    }
    let s = v.as_str().map(str::to_owned).unwrap_or_else(|| {
        v.as_f64()
            .map(safe_cex_num::js_number_string)
            .unwrap_or_else(|| v.to_string())
    });
    if kind == "address" {
        let address = ethereum::get_address(&s).map_err(|e| error(&e))?;
        let mut out = [0; 32];
        out[12..].copy_from_slice(&unhex(&address)?);
        return Ok(out);
    }
    // Fixed-width ABI uint encoding; reject overflow instead of truncating the
    // token ID. Decimal arithmetic here does not enter JavaScript Number.
    let (digits, radix) = s
        .strip_prefix("0x")
        .map_or((s.as_str(), 10u16), |s| (s, 16));
    if digits.is_empty() {
        return Err(error("Invalid ABI uint"));
    }
    let mut out = [0u8; 32];
    for digit in digits.chars() {
        let mut carry = digit
            .to_digit(u32::from(radix))
            .ok_or_else(|| error("Invalid ABI uint"))? as u16;
        for byte in out.iter_mut().rev() {
            let value = u16::from(*byte) * radix + carry;
            *byte = value as u8;
            carry = value >> 8;
        }
        if carry != 0 {
            return Err(error("ABI uint overflow"));
        }
    }
    Ok(out)
}
fn contents_hash(m: &serde_json::Value) -> Result<[u8; 32], Thrown> {
    let mut b = ethereum::keccak256(ORDER_TYPE.as_bytes()).to_vec();
    for f in order_fields().as_array().expect("fixed declaration") {
        b.extend(word(
            &m[f["name"].as_str().expect("fixed name")],
            f["type"].as_str().expect("fixed type"),
        )?);
    }
    Ok(ethereum::keccak256(&b))
}
fn domain_hash(contract: &str, chain: u64) -> Result<[u8; 32], Thrown> {
    let mut b = vec![];
    for text in [
        "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
        "Polymarket CTF Exchange",
        "2",
    ] {
        b.extend(ethereum::keccak256(text.as_bytes()));
    }
    b.extend(word(&serde_json::json!(chain), "uint256")?);
    b.extend(word(&serde_json::json!(contract), "address")?);
    Ok(ethereum::keccak256(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    fn ready<T>(f: impl Future<Output = T>) -> T {
        let mut f = Box::pin(f);
        match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("native signer unexpectedly suspended"),
        }
    }
    #[test]
    fn pinned_sdk_and_actual_source_signatures() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/binding_signing_oracle.cjs"
            ))
            .output()
            .expect("source oracle executes");
        assert!(
            output.status.success(),
            "source SDK oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let corpus: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("source capture");
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 96, "independent SDK corpus floor");
        let wallet = wallet(from_json(&corpus["key"])).unwrap();
        let globals = Js::object(vec![]);
        install(&globals, Weak::new());
        for case in cases {
            let params = &case["params"];
            let get = wallet.clone();
            let sign = wallet.clone();
            let signer = Js::object(vec![
                (
                    "getAddress",
                    Js::function("getAddress", move |_, _| {
                        get.call_method("getAddress", vec![])
                    }),
                ),
                (
                    "_signTypedData",
                    Js::function("_signTypedData", move |_, a| {
                        sign.call_method("signTypedData", a)
                    }),
                ),
            ]);
            let now = corpus["now"].as_f64().unwrap();
            let builder = Builder {
                runtime: rt.exchange_runtime(),
                signer,
                chain: Js::Number(137.),
                signature_type: from_json(&params["signatureType"]),
                funder: params.get("funderAddress").map_or(Js::Undefined, from_json),
                now: Rc::new(move || now),
                random: Rc::new(|| 0.5),
            };
            let user = Js::object(vec![
                ("tokenID", from_json(&params["tokenId"])),
                ("price", from_json(&params["price"])),
                ("size", from_json(&params["size"])),
                ("side", from_json(&params["side"])),
            ]);
            let options = Js::object(vec![
                ("tickSize", from_json(&params["tickSize"])),
                ("negRisk", from_json(&params["negRisk"])),
            ]);
            let order = ready(build_order(builder, user, options, Js::Number(2.)))
                .unwrap_or_else(|e| panic!("SDK params {params}: {}", e.message()));
            let native = to_json(&order).unwrap();
            let expected = case["order"].as_object().unwrap();
            assert_eq!(
                native.as_object().unwrap().len(),
                expected.len(),
                "SDK native field cardinality {params}"
            );
            for (k, v) in expected {
                if v.is_number() {
                    assert_eq!(
                        native[k].as_f64(),
                        v.as_f64(),
                        "SDK native number {k} {params}"
                    );
                } else {
                    assert_eq!(&native[k], v, "SDK native field {k} {params}");
                }
            }
            let keys = order
                .own_keys()
                .iter()
                .map(|k| String::from_utf16(k).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::to_value(keys).unwrap(),
                case["keys"],
                "SDK order own-key order {params}"
            );
            let payload = globals
                .get("orderToJsonV2")
                .unwrap()
                .call(
                    Js::Undefined,
                    vec![order, Js::str("account-key"), Js::str("GTC")],
                )
                .unwrap();
            let bytes = safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    to_json(&payload).unwrap(),
                )
                .unwrap(),
            )
            .wire_bytes_for_content_type(Some("application/json"))
            .unwrap();
            assert_eq!(
                String::from_utf8(bytes).unwrap(),
                case["payloadString"].as_str().unwrap(),
                "actual SDK exact native wire payload {params}"
            );
        }
    }
}
