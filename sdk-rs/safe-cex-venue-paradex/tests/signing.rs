use safe_cex_venue_paradex::signing_operations::{self as api, OrderSigningData};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<str>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn surrogate(s: &str) -> Vec<u16> {
    units(s).into_iter().chain([0xd800]).collect()
}
fn decode(v: &Value) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn poll<F: Future + ?Sized>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}
fn outcome<T>(result: &Result<T, Error>, expected: &Value, errors: &[Error], name: &str) {
    match result {
        Ok(_) => assert_eq!(expected["ok"], true, "{name}_native_success"),
        Err(e) => {
            assert_eq!(expected["ok"], false, "{name}_native_failure");
            let expected = errors
                .iter()
                .find(|e| e.as_ref() == expected["error"].as_str().unwrap())
                .unwrap();
            assert!(Rc::ptr_eq(e, expected), "{name}_native_error_identity");
        }
    }
}
fn quantum(row: &Value) {
    let value = decode(&row["value"]);
    let field = surrogate("size-");
    assert_eq!(field, decode(&row["field"]));
    let failure = row["failure"].as_str().unwrap();
    let errors: [Error; 3] = [
        Rc::from("primary"),
        Rc::from("message"),
        Rc::from("decimal"),
    ];
    let trace = RefCell::new(vec![]);
    let messages = RefCell::new(vec![]);
    let touch = |n: &str| {
        trace.borrow_mut().push(json!(n));
        if failure == n {
            Err(errors[0].clone())
        } else {
            Ok(())
        }
    };
    let decimal = Rc::new(());
    let multiplied = Rc::new(());
    let rounded = Rc::new(());
    let valid = !([units("NaN"), units("Infinity"), surrogate("bad-")].contains(&value));
    let negative = [units("-1"), units("-0")].contains(&value);
    let receipt = match value.as_slice() {
        [48] => "0",
        [49, 46, 50, 51, 52, 53, 54, 55, 56, 57, 49] => "123456789",
        [53, 54, 46, 55, 56, 57, 49, 50, 51, 52, 53, 54] => "5678912345",
        [48, 46, 48, 48, 48, 48, 48, 48, 48, 48, 57] => "0",
        [49, 101, 51] => "100000000000",
        _ => "unreachable",
    };
    let result = api::to_chain_quantum(
        &value,
        &field,
        |input| {
            assert_eq!(input, value, "quantum_native_input");
            touch("decimal")?;
            if value == surrogate("bad-") {
                return Err(errors[2].clone());
            }
            Ok(decimal.clone())
        },
        |d| {
            assert!(Rc::ptr_eq(d, &decimal));
            touch("finite")?;
            Ok(valid)
        },
        |d| {
            assert!(Rc::ptr_eq(d, &decimal));
            touch("nan")?;
            Ok(false)
        },
        |d| {
            assert!(Rc::ptr_eq(d, &decimal));
            touch("negative")?;
            Ok(negative)
        },
        |d, factor| {
            assert!(Rc::ptr_eq(&d, &decimal));
            assert_eq!(factor, 100000000.0, "quantum_native_scale");
            trace.borrow_mut().push(json!(["multiply", bits(factor)]));
            if failure == "multiply" {
                Err(errors[0].clone())
            } else {
                Ok(multiplied.clone())
            }
        },
        |m, round| {
            assert!(Rc::ptr_eq(&m, &multiplied));
            assert_eq!(round, 1, "quantum_native_round_down");
            trace.borrow_mut().push(json!(["integer", round]));
            if failure == "integer" {
                Err(errors[0].clone())
            } else {
                Ok(rounded.clone())
            }
        },
        |r, places| {
            assert!(Rc::ptr_eq(&r, &rounded));
            assert_eq!(places, 0, "quantum_native_fixed_places");
            trace.borrow_mut().push(json!(["fixed", places]));
            if failure == "fixed" {
                Err(errors[0].clone())
            } else {
                assert_ne!(receipt, "unreachable");
                Ok(units(receipt))
            }
        },
        |message| {
            let expected = if valid {
                field
                    .iter()
                    .copied()
                    .chain(units(" cannot be negative for signing: "))
                    .chain(value.iter().copied())
                    .collect::<Vec<_>>()
            } else {
                units("Invalid ")
                    .into_iter()
                    .chain(field.iter().copied())
                    .chain(units(" value for signing: "))
                    .chain(value.iter().copied())
                    .collect()
            };
            assert_eq!(message, expected, "quantum_native_error_message");
            trace.borrow_mut().push(json!("error"));
            messages.borrow_mut().push(json!(message));
            if failure == "error" {
                Err(errors[0].clone())
            } else {
                Ok(errors[1].clone())
            }
        },
    );
    outcome(&result, &row["outcome"], &errors, "quantum");
    if let Ok(value) = result {
        assert_eq!(
            value,
            decode(&row["outcome"]["value"]),
            "quantum_native_dependency_receipt"
        );
    }
    assert_eq!(json!(*trace.borrow()), row["trace"]);
    assert_eq!(json!(*messages.borrow()), row["messages"]);
}
#[derive(Clone)]
struct Held {
    ready: Rc<Cell<bool>>,
    result: Result<(), Error>,
}
impl Future for Held {
    type Output = Result<(), Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.get() {
            Poll::Ready(self.result.clone())
        } else {
            Poll::Pending
        }
    }
}
fn fields(names: &[&str]) -> Value {
    json!(
        names
            .iter()
            .map(|n| json!({"name":n,"type":"felt"}))
            .collect::<Vec<_>>()
    )
}
fn signing(row: &Value) {
    let modify = row["op"] == "signModifyOrder";
    let buy = row["buy"].as_bool().unwrap();
    let profile = row["profile"].as_str().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let now = number(&row["now"]);
    let trace = RefCell::new(vec![]);
    let requests = RefCell::new(vec![]);
    let errors: [Error; 3] = [Rc::from("primary"), Rc::from("logger"), Rc::from("missing")];
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if failure == name {
            Err(errors[0].clone())
        } else {
            Ok(())
        }
    };
    let read_now = || {
        touch("now")?;
        Ok(now)
    };
    let read_buy = || {
        touch("side")?;
        Ok(buy)
    };
    let size = || {
        touch("size")?;
        touch("size-quantum")?;
        Ok(units("123456789"))
    };
    let type_reads = Cell::new(0);
    let kind = if profile == "market" {
        "MARKET"
    } else {
        "LIMIT"
    };
    let read_type = || {
        let i = type_reads.get();
        type_reads.set(i + 1);
        touch(&format!("type{i}"))?;
        Ok(units(kind))
    };
    let price_reads = Cell::new(0);
    let read_price = || {
        let i = price_reads.get();
        price_reads.set(i + 1);
        touch(&format!("price{i}"))
    };
    let price_truth = || {
        read_price()?;
        Ok(profile != "empty")
    };
    let price = || {
        read_price()?;
        touch("price-quantum")?;
        Ok(units("987654321"))
    };
    let chain = || {
        touch("account0")?;
        touch("config")?;
        touch("chain")?;
        touch("chain-hex")?;
        Ok(surrogate("0xabc-"))
    };
    let timestamp = |n| {
        assert_eq!(bits(n), bits(now), "sign_native_clock_identity");
        touch("timestamp")?;
        Ok(units(
            if n.is_nan() {
                "NaN".to_owned()
            } else if n.is_infinite() {
                "Infinity".to_owned()
            } else if n == 0.0 {
                "0".to_owned()
            } else {
                format!("{n:.0}")
            }
            .as_str(),
        ))
    };
    let market = || {
        touch("market")?;
        Ok(surrogate("market-"))
    };
    let ready = Rc::new(Cell::new(false));
    let held = Held {
        ready: ready.clone(),
        result: if ["sign-async", "logger"].contains(&failure) {
            Err(errors[0].clone())
        } else {
            Ok(())
        },
    };
    let signature = Rc::new(());
    let headers = Rc::new(());
    let sign = |data: OrderSigningData| {
        assert_eq!(data.domain_name, "Paradex", "sign_native_domain_name");
        assert_eq!(data.domain_version, "1", "sign_native_domain_version");
        assert_eq!(data.chain_id, surrogate("0xabc-"), "sign_native_chain_id");
        assert_eq!(
            data.domain_fields,
            [("name", "felt"), ("chainId", "felt"), ("version", "felt")],
            "sign_native_domain_fields"
        );
        assert_eq!(
            data.primary_type,
            if modify { "ModifyOrder" } else { "Order" },
            "sign_native_primary_type"
        );
        let mut names = vec!["timestamp", "market", "side", "orderType", "size", "price"];
        if modify {
            names.push("id");
        }
        assert_eq!(
            data.message_fields,
            names.iter().map(|n| (*n, "felt")).collect::<Vec<_>>(),
            "sign_native_message_fields"
        );
        assert_eq!(
            data.timestamp,
            units(
                if now.is_nan() {
                    "NaN".to_owned()
                } else if now.is_infinite() {
                    "Infinity".to_owned()
                } else if now == 0.0 {
                    "0".to_owned()
                } else {
                    format!("{now:.0}")
                }
                .as_str()
            ),
            "sign_native_timestamp"
        );
        assert_eq!(data.market, surrogate("market-"), "sign_native_market");
        assert_eq!(
            data.side,
            if buy { "1" } else { "2" },
            "{}",
            if modify {
                "modify_native_side"
            } else {
                "order_native_side"
            }
        );
        assert_eq!(data.order_type, units(kind), "sign_native_type");
        assert_eq!(data.size, units("123456789"), "sign_native_size");
        assert_eq!(
            data.price,
            units(if modify || profile == "limit" {
                "987654321"
            } else {
                "0"
            }),
            "sign_native_price"
        );
        assert_eq!(
            data.id,
            modify.then(|| surrogate("id-")),
            "sign_native_amendment_id"
        );
        touch("account1")?;
        touch("options")?;
        let mut message = json!({"timestamp":data.timestamp,"market":data.market,"side":units(data.side),"orderType":data.order_type,"size":data.size,"price":data.price});
        if let Some(id) = data.id {
            message["id"] = json!(id);
        }
        let mut types = json!({"StarkNetDomain":fields(&["name","chainId","version"])});
        types[data.primary_type] = fields(&names);
        requests.borrow_mut().push(json!({"domain":{"name":data.domain_name,"chainId":data.chain_id,"version":data.domain_version},"types":types,"primaryType":data.primary_type,"message":message}));
        touch("sign-sync")?;
        Ok(held)
    };
    let result_read = |()| {
        touch("signature")?;
        touch("headers")?;
        Ok((signature.clone(), headers.clone()))
    };
    let log = |prefix, e: &Error| {
        assert_eq!(prefix, "[Paradex Order Signing] Failed to sign order:");
        assert!(Rc::ptr_eq(e, &errors[0]));
        trace.borrow_mut().push("logger".to_owned());
        if failure == "logger" {
            Err(errors[1].clone())
        } else {
            Ok(())
        }
    };
    let mut future: Pin<
        Box<dyn Future<Output = Result<api::SignedOrder<Rc<()>, Rc<()>>, Error>> + '_>,
    > = if modify {
        Box::pin(api::sign_modify_order(
            || {
                touch("id0")?;
                Ok(profile != "empty")
            },
            |message| {
                assert_eq!(
                    message, "Cannot sign Paradex order amendment without an order id",
                    "modify_native_missing_id_message"
                );
                touch("missing")?;
                Ok(errors[2].clone())
            },
            read_now,
            read_buy,
            size,
            price,
            chain,
            timestamp,
            market,
            read_type,
            || {
                touch("id1")?;
                Ok(surrogate("id-"))
            },
            sign,
            result_read,
        ))
    } else {
        Box::pin(api::sign_order(
            read_now,
            read_buy,
            size,
            read_type,
            price_truth,
            price,
            chain,
            timestamp,
            market,
            sign,
            result_read,
            log,
        ))
    };
    let first = poll(future.as_mut());
    assert_eq!(
        json!({"trace":*trace.borrow(),"settled":first.is_ready(),"requests":*requests.borrow()}),
        row["before"],
        "sign_native_await_boundary"
    );
    ready.set(true);
    let result = match first {
        Poll::Ready(r) => r,
        Poll::Pending => match poll(future.as_mut()) {
            Poll::Ready(r) => r,
            Poll::Pending => panic!("sign_ready"),
        },
    };
    outcome(&result, &row["outcome"], &errors, "sign");
    if let Ok(signed) = result {
        assert!(
            Rc::ptr_eq(&signed.signature, &signature),
            "sign_native_signature_identity"
        );
        assert!(
            Rc::ptr_eq(&signed.headers, &headers),
            "sign_native_headers_identity"
        );
        assert_eq!(
            bits(signed.signature_timestamp),
            bits(now),
            "sign_native_result_timestamp"
        );
        assert_eq!(row["outcome"]["signature"], true);
        assert_eq!(row["outcome"]["headers"], true);
        assert_eq!(
            bits(signed.signature_timestamp),
            row["outcome"]["timestamp"]
        );
    }
    assert_eq!(json!(*trace.borrow()), row["trace"]);
    assert_eq!(json!(*requests.borrow()), row["requests"]);
}
#[test]
fn original_signing_bodies() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "signing_source_observer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["quantums"].as_array().unwrap().len(),
        90,
        "quantum_independent_floor"
    );
    assert_eq!(
        data["signs"].as_array().unwrap().len(),
        308,
        "signing_independent_floor"
    );
    let inputs = [
        "0",
        "1.234567891",
        "56.789123456",
        "0.000000009",
        "1e3",
        "NaN",
        "Infinity",
        "-1",
        "-0",
    ];
    for (i, row) in data["quantums"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            decode(&row["value"]),
            if i / 9 == 9 {
                surrogate("bad-")
            } else {
                units(inputs[i / 9])
            },
            "quantum_live_input"
        );
        quantum(row);
    }
    for row in data["signs"].as_array().unwrap() {
        signing(row);
    }
}
