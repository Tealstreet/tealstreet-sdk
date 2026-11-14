#[path = "backpack/native.rs"]
mod native;
#[path = "backpack/owners.rs"]
mod owners;
#[path = "support/mod.rs"]
mod sessions;
#[path = "backpack/support.rs"]
mod support;
use safe_cex_engine::signing::{Algorithm, Capability, SignError};
use safe_cex_engine::{Action, Engine, Event, Method, Notice, Outcome};
use safe_cex_venue_backpack::{Backpack, Options, sign};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use support::*;

#[derive(Deserialize)]
struct Golden {
    method: String,
    path: String,
    body: Option<String>,
    canonical: String,
    signatures: Vec<String>,
}
fn method(text: &str) -> Result<Method> {
    match text {
        "GET" => Ok(Method::Get),
        "POST" => Ok(Method::Post),
        "DELETE" => Ok(Method::Delete),
        "PATCH" => Ok(Method::Patch),
        _ => Err("unknown method".into()),
    }
}
fn rows(vector: &Golden) -> Result<Vec<sign::Params>> {
    if vector.method == "GET" {
        return Ok(vec![
            reqwest::Url::parse(&format!("https://api.backpack.exchange{}", vector.path))?
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        ]);
    }
    let value: Value = serde_json::from_str(vector.body.as_deref().ok_or("body missing")?)?;
    let values = if let Value::Array(rows) = value {
        rows
    } else {
        vec![value]
    };
    values
        .into_iter()
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| "row missing".into())
                .map(|row| {
                    row.iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                            )
                        })
                        .collect::<BTreeMap<_, _>>()
                })
        })
        .collect()
}

#[tokio::test]
async fn backpack_hosted_all_signed_endpoints_match_local_goldens() -> Result {
    let vectors: Vec<Golden> =
        serde_json::from_str(include_str!("fixtures/backpack-hosted-goldens.json"))?;
    assert_eq!(vectors.len(), 16);
    for vector in &vectors {
        for (time_index, offset) in [0., 1000.].into_iter().enumerate() {
            for (window_index, window) in [60000, 20000, 60000].into_iter().enumerate() {
                let mut device = Device::new()?;
                let (remote, options) = device.remote()?;
                let options: safe_cex_venues::EngineOptions = serde_json::from_str(&options)?;
                let ids = options.signing.clone().ok_or("missing identities")?;
                let identity = ids.primary.clone().ok_or("no primary")?;
                let verb = method(&vector.method)?;
                let declaration = sign::Request {
                    method: verb,
                    path: vector.path.split('?').next().ok_or("missing path")?.into(),
                    rows: rows(vector)?,
                    body: vector.body.clone(),
                    window,
                };
                let body = vector.body.clone();
                let url = format!("https://api.backpack.exchange{}", vector.path);
                let venue = safe_cex_engine::test_support::ProbeVenue::new(
                    Backpack::new(Options::default()),
                    move |_, cx| {
                        let request = cx.request(
                            verb,
                            url.clone(),
                            vec![],
                            body.clone(),
                            safe_cex_venue_backpack::Pending::SocketSnapshot(String::new()),
                        );
                        cx.sign_http(
                            request,
                            Box::new(sign::Http::new(identity.clone(), declaration.clone())),
                        );
                    },
                );
                let mut engine = Engine::new(
                    venue,
                    Some(safe_cex_engine::test_support::seeded_entropy(5)),
                )?;
                engine.set_signer(Some(remote.provider))?;
                engine.set_identities(ids)?;
                let mut driver = Drive::new(engine);
                driver.set_time(NOW + offset);
                driver.start();
                let packets = requests(driver.pump(&mut device).await?);
                assert_eq!(packets.len(), 1);
                let timestamp = safe_cex_engine::num::format((NOW + offset).floor());
                assert_eq!(
                    device.payloads,
                    [format!(
                        "{}&timestamp={timestamp}&window={window}",
                        vector.canonical
                    )]
                );
                let golden = &vector.signatures[time_index * 3 + window_index];
                assert_eq!(&device.signatures[0], golden);
                assert_eq!(signature(&packets[0])?, golden);
                assert!(
                    packets[0]
                        .headers
                        .iter()
                        .any(|(name, value)| *name == "X-Window" && value == &window.to_string())
                );
                assert_eq!(
                    packets[0].url,
                    format!("https://api.backpack.exchange{}", vector.path)
                );
                assert_eq!(packets[0].body, vector.body);
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn backpack_hosted_startup_uses_primary_device_custody() -> Result {
    native::prove().await?;
    let mut device = Device::new()?;
    let (remote, options) = device.remote()?;
    let value: Value = serde_json::from_str(&options)?;
    for field in ["key", "secret", "passphrase", "remoteSigner"] {
        assert!(value.get(field).is_none());
    }
    assert_eq!(value["remoteSigning"], true);
    assert_eq!(value["payloadSigner"], false);
    assert_eq!(
        value["signing"].as_object().ok_or("missing signing")?.len(),
        1
    );
    let (driver, actions) = ready(&mut device, &format!("[{ORDER}]")).await?;
    assert!(private_socket(&actions));
    for path in ["/account", "/position", "/collateral", "/orders"] {
        assert!(actions.iter().any(|action|matches!(action,Action::Http(request) if request.url.ends_with(path) && signature(request).is_ok())));
    }
    assert!(actions.iter().any(|action|matches!(action,Action::WsSend {text,..} if text.contains("account.orderUpdate") && text.contains("signature"))));
    assert!(
        device
            .payloads
            .iter()
            .any(|payload| payload == "instruction=subscribe&timestamp=1700000000123&window=5000")
    );
    assert!(driver.ids().secondary.is_none());
    assert!(
        safe_cex_remote_signer::remote_account(
            device.link.clone(),
            "backpack",
            None,
            r#"{"key":"fixture-key","secondary_key":"read-key"}"#
        )
        .is_err()
    );
    assert!(safe_cex_native::resolve::rest_base_url("binance", false).is_ok());
    assert!(safe_cex_venues::supported("backpack"));
    assert!(safe_cex_native::resolve::rest_base_url("backpack", true).is_err());
    let ws_goldens: Vec<String> =
        serde_json::from_str(include_str!("fixtures/backpack-hosted-ws-goldens.json"))?;
    for (time_index, offset) in [0., 1000.].into_iter().enumerate() {
        for (raw, expected) in [(None, 60000), (Some(20000.9), 20000), (Some(90000.), 60000)] {
            let mut device = Device::new()?;
            let (mut driver, actions) = ready_at(&mut device, "[]", raw, NOW + offset).await?;
            let timestamp = safe_cex_engine::num::format((NOW + offset).floor());
            let ws = device
                .payloads
                .iter()
                .position(|payload| {
                    payload == &format!("instruction=subscribe&timestamp={timestamp}&window=5000")
                })
                .ok_or("missing WS auth")?;
            assert_eq!(device.signatures[ws], ws_goldens[time_index]);
            assert!(actions.iter().any(|action|matches!(action,Action::Http(request) if request.url.ends_with("/account")&&request.headers.iter().any(|(name,value)|*name=="X-Window"&&value==&expected.to_string()))));
            command(&mut *driver, json!({"op":"fetchAccount","command":31}))?;
            let packets = requests(driver.pump(&mut device).await?);
            assert!(!packets.is_empty());
            for packet in packets {
                assert!(
                    packet
                        .headers
                        .iter()
                        .any(|(name, value)| *name == "X-Window" && value == &expected.to_string())
                );
            }
        }
    }
    assert_eq!(
        remote.provider.can_sign(
            &safe_cex_engine::signing::KeyRef("foreign".into()),
            Algorithm::Ed25519
        ),
        Capability::Unsupported
    );
    Ok(())
}

#[tokio::test]
async fn backpack_hosted_skew_discards_old_signature_and_resigns() -> Result {
    let mut device = Device::new()?;
    let (mut driver, _) = ready(&mut device, "[]").await?;
    device.payloads.clear();
    device.signatures.clear();
    driver.skew_next_signature();
    command(&mut *driver, json!({"op":"fetchAccount","command":41}))?;
    let packets = requests(driver.pump(&mut device).await?);
    assert!(!packets.is_empty());
    let old = device
        .payloads
        .iter()
        .position(|payload| payload.contains("timestamp=1700000000123"))
        .ok_or("no old signing attempt")?;
    let fresh = device
        .payloads
        .iter()
        .position(|payload| payload.contains("timestamp=1700000001123"))
        .ok_or("no fresh signing attempt")?;
    assert_ne!(device.signatures[old], device.signatures[fresh]);
    for packet in &packets {
        assert!(
            packet
                .headers
                .iter()
                .any(|(name, value)| *name == "X-Timestamp" && value == "1700000001123")
        );
        assert_ne!(signature(packet)?, device.signatures[old]);
    }
    let before = device.payloads.len();
    driver.set_time(f64::NAN);
    command(
        &mut *driver,
        json!({"op":"placeOrders","command":42,"orders":[{"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140}]}),
    )?;
    let out = driver.pump(&mut device).await?;
    assert_eq!(writes(&out), 0);
    assert_eq!(device.payloads.len(), before);
    assert_eq!(failure(&out, 42)?.outcome, Some(Outcome::NotSent));
    Ok(())
}

#[tokio::test]
async fn backpack_hosted_unknown_write_never_replays() -> Result {
    for op in [
        "place",
        "batch",
        "cancel",
        "cancel-all",
        "amend-cancel",
        "amend-replace",
        "limit-amend-cancel",
        "limit-amend-replace",
        "put-back",
        "protective-child",
    ] {
        for status in [0, 408, 429, 500, 503] {
            let mut device = Device::new()?;
            let resting = if op.starts_with("limit-") {
                ORDER
            } else {
                STOP
            };
            let (mut driver, _) = ready(&mut device, &format!("[{resting}]")).await?;
            let order = json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140});
            let value = match op {
                "place" => json!({"op":"placeOrders","command":51,"orders":[order]}),
                "batch" => json!({"op":"placeOrders","command":51,"orders":[order.clone(),order]}),
                "cancel" => json!({"op":"cancelOrders","command":51,"ids":["123"]}),
                "cancel-all" => json!({"op":"cancelSymbolOrders","command":51,"symbol":SYMBOL}),
                "protective-child" => {
                    json!({"op":"placeOrders","command":51,"orders":[{"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}]})
                }
                _ => json!({"op":"amendOrder","command":51,"id":"123","price":125}),
            };
            command(&mut *driver, value)?;
            let mut packet = requests(driver.pump(&mut device).await?)
                .pop()
                .ok_or("write not dispatched")?;
            assert_signature_shape(&packet)?;
            if op.ends_with("amend-replace") || op == "put-back" {
                response(
                    &mut *driver,
                    &packet,
                    200,
                    r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
                );
                packet = requests(driver.pump(&mut device).await?)
                    .pop()
                    .ok_or("replacement not dispatched")?;
                assert_eq!(packet.method, Method::Post);
            }
            if op == "put-back" {
                response(
                    &mut *driver,
                    &packet,
                    400,
                    r#"{"code":"INVALID_ORDER","message":"bad"}"#,
                );
                let proof = requests(driver.pump(&mut device).await?)
                    .pop()
                    .ok_or("proof read not dispatched")?;
                assert_eq!(proof.method, Method::Get);
                response(&mut *driver, &proof, 200, "[]");
                packet = requests(driver.pump(&mut device).await?)
                    .pop()
                    .ok_or("put-back not dispatched")?;
            }
            if op == "protective-child" {
                response(&mut *driver, &packet, 200, r#"{"id":"accepted-entry"}"#);
                packet = requests(driver.pump(&mut device).await?)
                    .pop()
                    .ok_or("protective child not dispatched")?;
            }
            if status == 0 {
                driver.event(Event::HttpFailed {
                    request: packet.id,
                    error: "ambiguous transport loss",
                });
            } else {
                response(
                    &mut *driver,
                    &packet,
                    status,
                    r#"{"code":"NATIVE_UNAVAILABLE","message":"native detail"}"#,
                );
            }
            let out = driver.pump(&mut device).await?;
            assert_eq!(writes(&out), 0, "ambiguous leg replayed: {op} {status}");
            let error = failure(&out, 51)?;
            assert_eq!(error.outcome, Some(Outcome::Unknown), "{op} {status}");
            if op == "put-back" || op.ends_with("amend-replace") {
                let warning = error.legacy_text().contains("Check Open Orders")
                    || out.iter().any(|action| {
                        matches!(action,
                        Action::Notice(Notice::CommandError { command: 51, message })
                        if message.contains("Check Open Orders"))
                    });
                assert!(warning, "{op} {status}");
            }
            if op == "protective-child" {
                assert_eq!(
                    error.partial.as_ref().ok_or("lost prior ids")?.order_ids,
                    ["accepted-entry"]
                );
            }
            response(
                &mut *driver,
                &packet,
                200,
                r#"{"id":"late","status":"Cancelled"}"#,
            );
            let late = driver.pump(&mut device).await?;
            assert_eq!(writes(&late), 0);
            assert!(!late.iter().any(|action| matches!(
                action,
                Action::Notice(
                    Notice::CommandDone { command: 51, .. }
                        | Notice::CommandFailed { command: 51, .. }
                )
            )));
        }
    }
    let mut device = Device::new()?;
    let (mut driver, _) = ready(&mut device, &format!("[{ORDER}]")).await?;
    device.refuse = true;
    command(
        &mut *driver,
        json!({"op":"cancelOrders","command":52,"ids":["123"]}),
    )?;
    let out = driver.pump(&mut device).await?;
    assert_eq!(writes(&out), 0);
    let error = failure(&out, 52)?;
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(!error.message.contains("SYNTHETIC-DEVICE-ERROR"));
    Ok(())
}

#[tokio::test]
async fn backpack_public_init_never_installs_signer_or_opens_private_ws() -> Result {
    for fields in [
        json!({"accountName":"backpack-proof","exchangeName":"backpack","key":"unused","publicOnly":true}),
        json!({"accountName":"__pub_backpack","exchangeName":"backpack"}),
        json!({"accountName":"backpack-proof","exchangeName":"backpack","key":"","publicOnly":true}),
    ] {
        let init: safe_cex_exchange_server::envelope::InitPayload = serde_json::from_value(fields)?;
        let public = init.public_only == Some(true) || init.account_name.starts_with("__pub_");
        assert!(public);
        let prepared = safe_cex_exchange_server::accounts::prepare(&init, public, None)?;
        assert!(prepared.remote.is_none());
        let built: Value = serde_json::from_str(&prepared.options)?;
        assert!(built.get("key").is_none());
        assert!(built.get("signing").is_none());
        let mut device = Device::new()?;
        let mut driver = build(&device, true)?;
        assert!(driver.ids().primary.is_none());
        assert!(driver.signer().is_none());
        driver.start();
        let mut actions = vec![];
        for _ in 0..12 {
            let next = driver.pump(&mut device).await?;
            for action in &next {
                if let Action::Http(request) = action {
                    response(
                        &mut *driver,
                        request,
                        200,
                        &startup_body(&request.url, "[]"),
                    );
                }
            }
            let empty = next.is_empty();
            actions.extend(next);
            if empty {
                break;
            }
        }
        assert!(driver.store().loaded().markets);
        assert!(!private_socket(&actions));
        assert!(device.payloads.is_empty());
        assert!(device.frames.try_recv().is_err());
        assert!(actions.iter().any(
            |action| matches!(action,Action::Http(request) if request.url.ends_with("/markets"))
        ));
    }
    Ok(())
}

#[tokio::test]
async fn backpack_private_payload_and_undeclared_venue_stay_refused() -> Result {
    let device = Device::new()?;
    let (remote, options) = device.remote()?;
    let mut value: Value = serde_json::from_str(&options)?;
    value["payloadSigner"] = true.into();
    struct Ignore;
    impl safe_cex_venues::Visit for Ignore {
        type Output = ();
        fn visit<V: safe_cex_engine::Venue + 'static>(self, _: Engine<V>) {}
    }
    let error = safe_cex_venues::build_with_signer(
        "backpack",
        serde_json::from_value(value)?,
        None,
        Some(remote.provider),
        Ignore,
    )
    .err()
    .ok_or("PayloadSigner admitted")?;
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(error.message == "Rust backpack has no REST-only PayloadSigner lifecycle");
    assert!(
        safe_cex_remote_signer::remote_account(
            device.link,
            "undeclared-fixture",
            None,
            r#"{"key":"fixture-key"}"#
        )
        .is_err()
    );
    assert_eq!(
        SignError::Refused.venue_error().outcome,
        Some(Outcome::NotSent)
    );
    Ok(())
}
