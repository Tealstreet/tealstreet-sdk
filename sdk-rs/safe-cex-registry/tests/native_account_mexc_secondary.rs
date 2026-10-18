#![cfg(not(target_arch = "wasm32"))]
mod support;

use hmac::{Hmac, Mac};
use safe_cex_registry::native_account::{
    NativeAccountSpec, native_account_call, open_native_account,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use support::{Seen, run_local, serve_http};
use tokio::net::TcpListener;

fn assert_signature(request: &Seen, key: &str, secret: &str) {
    assert_eq!(request.headers["apikey"], key);
    let data = if request.method == "GET" {
        &request.query
    } else {
        &request.body
    };
    let preimage = format!("{key}{}{data}", request.headers["request-time"]);
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(preimage.as_bytes());
    let expected = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(request.headers["signature"], expected);
}

async fn requests(secondary: Option<&str>) -> Vec<Seen> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_http(
        listener,
        Rc::new(|request| {
            assert!(
                request.path.ends_with("/api/v1/private/account/assets")
                    || request.path.ends_with("/api/v1/private/order/cancel_all"),
                "{request:?}"
            );
            Some(json!({"success":true,"code":0,"data":[]}))
        }),
        seen.clone(),
    ));
    let mut options = json!({"key":"primary-key","secret":"primary-secret","overrideProxyUrl":format!("{origin}/primary"),"secondary_secret":"secondary-secret","secondaryProxyUrl":format!("{origin}/secondary")});
    if let Some(key) = secondary {
        options["secondary_key"] = json!(key);
    }
    let account = open_native_account(NativeAccountSpec {
        hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange: "mexc".into(),
        options: options.into(),
        account_name: None,
        sign: None,
        proxy_token: None,
    })
    .unwrap();
    for method in ["getAccount", "cancelAllOrders", "dispose"] {
        native_account_call(account.clone(), method.into(), vec![])
            .await
            .unwrap_or_else(|error| panic!("{method}: {error:?}"));
    }
    let requests = seen.borrow().clone();
    requests
}

#[test]
fn native_mexc_secondary_gets_use_the_read_key_and_proxy_but_writes_use_primary() {
    run_local(async {
        let requests = requests(Some("secondary-key")).await;
        assert_eq!(requests.len(), 2);
        let read = &requests[0];
        let write = &requests[1];
        assert_eq!(read.method, "GET");
        assert_eq!(write.method, "POST");
        assert_signature(read, "secondary-key", "secondary-secret");
        assert_signature(write, "primary-key", "primary-secret");
        assert!(read.path.starts_with("/secondary/"), "{read:?}");
        assert!(write.path.starts_with("/primary/"), "{write:?}");
        assert_eq!(
            serde_json::from_str::<Value>(&write.body).unwrap(),
            json!({})
        );
    });
}

#[test]
fn native_mexc_absent_or_empty_secondary_key_keeps_reads_and_writes_primary() {
    run_local(async {
        for key in [None, Some("")] {
            let requests = requests(key).await;
            assert_eq!(requests.len(), 2);
            for request in requests {
                assert_signature(&request, "primary-key", "primary-secret");
                assert!(request.path.starts_with("/primary/"), "{request:?}");
            }
        }
    });
}

async fn receipt_case(rejected: bool) {
    use safe_cex_registry::{native_host, rust_venues::RustVenueHost};
    use safe_cex_transport::{RestBody, RestRequest};
    use safe_cex_types::retained_json::RetainedJsonInput;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_http(
        listener,
        Rc::new(move |request| {
            let body = json!({"success":!rejected,"code":if rejected {403} else {0},
            "data":[],"message":"receipt response","method":request.method});
            Some(if rejected {
                json!({"__status":403,"body":body})
            } else {
                body
            })
        }),
        seen.clone(),
    ));
    let options = serde_json::from_value(json!({"key":"primary-key","secret":"primary-secret",
        "overrideProxyUrl":format!("{origin}/primary"),"secondary_key":"read-key",
        "secondary_secret":"read-secret","secondaryProxyUrl":format!("{origin}/secondary")}))
    .unwrap();
    let host = native_host::native_venue_host(native_host::hyperliquid_rate_limiter());
    let client = host.mexc(&options).unwrap().http;
    for (method, path, key, secret, proxy) in [
        (
            "GET",
            "/api/v1/private/account/assets",
            "read-key",
            "read-secret",
            "secondary",
        ),
        (
            "POST",
            "/api/v1/private/order/cancel_all",
            "primary-key",
            "primary-secret",
            "primary",
        ),
    ] {
        let mut request = RestRequest {
            method: method.into(),
            url: path.into(),
            body: (method == "POST").then(|| {
                RestBody::Untransformed(RetainedJsonInput::Object(vec![(
                    "receipt".encode_utf16().collect(),
                    RetainedJsonInput::Bool(true),
                )]))
            }),
            ..Default::default()
        };
        request
            .config_extensions
            .insert("receiptTag".into(), RetainedJsonInput::Float(-0.0));
        let result = client.execute_with_receipt(request).await;
        let (config, response) = if rejected {
            let failure = result.unwrap_err();
            assert_eq!(failure.code.as_deref(), Some("ERR_BAD_REQUEST"));
            (*failure.request, *failure.response.unwrap())
        } else {
            let execution = result.unwrap();
            (execution.config, execution.response)
        };
        assert_eq!(config.method.to_ascii_uppercase(), method);
        assert_eq!(
            config.base_url.as_deref(),
            Some(format!("{origin}/{proxy}/https://api.mexc.com").as_str())
        );
        assert_eq!(config.url, path);
        let wire = seen.borrow().last().unwrap().clone();
        assert_eq!(wire.path, format!("/{proxy}/https://api.mexc.com{path}"));
        assert_signature(&wire, key, secret);
        for name in ["apikey", "signature", "request-time"] {
            assert_eq!(
                config
                    .headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .unwrap()
                    .1,
                &wire.headers[name],
                "effective signed receipt {method}/{name}"
            );
        }
        assert!(matches!(config.config_extensions.get("receiptTag"),
            Some(RetainedJsonInput::Float(value)) if value.to_bits() == (-0.0f64).to_bits()));
        assert_eq!(response.status, if rejected { 403 } else { 200 });
        assert_eq!(response.body.json_value().unwrap()["method"], method);
        if method == "POST" {
            assert_eq!(
                serde_json::from_str::<Value>(&wire.body).unwrap(),
                json!({"receipt":true})
            );
        }
    }
    assert_eq!(seen.borrow().len(), 2);
}

#[test]
fn native_mexc_secondary_receipts_keep_effective_signing_and_response() {
    run_local(receipt_case(false));
}

#[test]
fn native_mexc_secondary_failures_keep_effective_signing_and_response() {
    run_local(receipt_case(true));
}
