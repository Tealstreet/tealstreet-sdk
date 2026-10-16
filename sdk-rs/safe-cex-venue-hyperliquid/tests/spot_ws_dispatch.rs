#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/spot_ws_host.rs"]
mod support;
use cache::{N, arr, object, s};
use native::{assert_value, decode};
use safe_cex_venue_hyperliquid::{spot_ws_dispatch::*, ws_ping_lifecycle::WsPingHost};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};
use support::{Host, flag, get, input};
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/spot_ws_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["sourceHashes"].as_object().unwrap().len(),
            6,
            "WS independent source floor"
        );
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            73,
            "WS independent case floor through72"
        );
        v
    })
}
fn execute(row: &Value) -> (Result<N, N>, Host) {
    let i = input(row);
    let mut h = Host::new(
        i.clone(),
        decode(&oracle()["tables"]),
        decode(&oracle()["systems"]),
    );
    let family = row["family"].as_str().unwrap();
    let result = (|| {
        let socket = if family == "public" || family == "mids" {
            h.public_socket()
        } else {
            let class = private_socket_class(&mut h)?;
            let parent = h.parent.clone();
            if family == "constructor" {
                let target = if flag(&i, "customTarget") || flag(&i, "prototypeThrow") {
                    h.custom_target()
                } else {
                    class.constructor.clone()
                };
                let socket = new_private_socket(&mut h, &class, &parent, Some(&target))?.native;
                return Ok(h.constructor_summary(&class, &socket, &target));
            }
            let socket = new_private_socket(&mut h, &class, &parent, None)?.native;
            if family == "class" {
                let other = new_private_socket(&mut h, &class, &parent, None)?.native;
                if flag(&i, "lexical") {
                    let f = h.get(&socket, "handleUserNonFundingLedgerUpdates")?;
                    h.call(&f, &object(vec![]), &[])?;
                }
                if flag(&i, "chain") {
                    h.invoke(&socket, "connectAndSubscribe", &[])?;
                    h.invoke(&socket, "onOpen", &[])?;
                }
                return Ok(h.class_summary(&class, &socket, &other));
            }
            socket
        };
        let data = if flag(&i, "poison") {
            let p = object(vec![]);
            h.poison = Some(p.clone());
            p
        } else {
            get(&i, "data")
        };
        match family {
            "balances" => handle_spot_clearinghouse_state(&mut h, &socket, &data)?,
            "fills" => handle_fills(&mut h, &socket, &data)?,
            "fundings" => handle_fundings(&mut h, &socket, &data),
            "ledger" => handle_user_non_funding_ledger_updates(&mut h, &socket, &data)?,
            "mids" => {
                if flag(&i, "inherited") {
                    let N::O(o) = &data else { panic!("data") };
                    o.borrow_mut().prototype = Some(object(vec![("ETH", s("9.5"))]))
                }
                handle_all_mids(&mut h, &socket, &data)?
            }
            "public" | "private" => {
                let event = if flag(&i, "eventNull") {
                    N::Null
                } else if flag(&i, "eventUndefined") {
                    N::U
                } else {
                    object(vec![("data", get(&i, "wire"))])
                };
                if family == "public" {
                    public_on_message(&mut h, &socket, &event)?
                } else {
                    h.invoke(&socket, "onMessage", &[event])?;
                }
            }
            _ => panic!("family"),
        };
        Ok(N::U)
    })();
    (result, h)
}
fn check(family: &str, count: usize) {
    let rows = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count, "WS independent family floor {family}");
    for row in rows {
        let (result, h) = execute(row);
        let label = format!("WS native {family} {}", row["name"]);
        assert_eq!(
            result.is_err(),
            row["rejected"].as_bool().unwrap(),
            "{label}: explicit rejected discriminator"
        );
        match result {
            Ok(v) => assert_value(&v, &row["result"], &label),
            Err(v) => assert_value(&v, &row["error"], &label),
        }
        assert_value(&arr(h.effects), &row["effects"], &label);
    }
}
#[test]
fn private_class_composition() {
    check("class", 4)
}
#[test]
fn private_constructor() {
    check("constructor", 4)
}
#[test]
fn private_frames() {
    check("private", 20)
}
#[test]
fn balance_forwarding() {
    check("balances", 5)
}
#[test]
fn fills_native() {
    check("fills", 7)
}
#[test]
fn funding_noop() {
    check("fundings", 3)
}
#[test]
fn ledger_refresh() {
    check("ledger", 3)
}
#[test]
fn public_frames() {
    check("public", 21)
}
#[test]
fn mids_native() {
    check("mids", 6)
}
