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
        options,
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
