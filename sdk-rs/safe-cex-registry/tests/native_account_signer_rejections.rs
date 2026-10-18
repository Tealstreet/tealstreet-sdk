#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccountSpec, native_account_call, open_native_account,
};
use safe_cex_registry::native_value::NativeValue;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use tokio::net::TcpListener;

#[test]
fn real_native_account_preserves_hosted_signer_rejection_fields() {
    support::run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let seen = Rc::new(RefCell::new(vec![]));
        tokio::task::spawn_local(support::serve_http(
            listener,
            Rc::new(|_| Some(json!({"unexpected":"unsigned request"}))),
            seen.clone(),
        ));
        let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            exchange: "binance".into(),
            options: json!({"key":"hosted-key","secret":"","overrideProxyUrl":origin}).into(),
            account_name: Some("hosted-account".into()),
            sign: Some(Rc::new(|_| Box::pin(async {
                Err(json!({"name":"SignerUnavailableError", "message":"Remote signer unavailable", "code":"SIGNER_OFFLINE"}).into())
            }))),
            proxy_token: None,
        }).unwrap();
        let error = native_account_call(account.clone(), "getAccount".into(), vec![])
            .await
            .unwrap_err();
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
        assert!(
            seen.borrow().is_empty(),
            "failed signing must not send HTTP"
        );
        assert_eq!(
            error["message"],
            NativeValue::string("Remote signer unavailable")
        );
        assert_eq!(error["name"], NativeValue::string("SignerUnavailableError"));
        assert_eq!(error["code"], NativeValue::string("SIGNER_OFFLINE"));
    });
}

#[test]
fn real_native_account_preserves_signer_utf16_metadata() {
    support::run_local(async {
        for (name, message) in [
            (vec![0xd800], vec![0xdc00]),
            (vec![0xdc00], vec![0xd800]),
            (vec![0xd83d, 0xde00, 0], vec![0, 0xd83d, 0xde00]),
            (vec![0xfffd], vec![0xfffd]),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let seen = Rc::new(RefCell::new(vec![]));
            tokio::task::spawn_local(support::serve_http(
                listener,
                Rc::new(|_| Some(json!({"unexpected":"unsigned request"}))),
                seen.clone(),
            ));
            let failure = NativeValue::object([
                ("name", NativeValue::String(name.clone())),
                ("message", NativeValue::String(message.clone())),
                ("code", NativeValue::String(vec![0xd800])),
            ]);
            let account = open_native_account(NativeAccountSpec {
                hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
                exchange: "binance".into(),
                options: json!({"key":"hosted-key","secret":"","overrideProxyUrl":origin}).into(),
                account_name: Some("hosted-account".into()),
                sign: Some(Rc::new(move |_| {
                    let failure = failure.clone();
                    Box::pin(async move { Err(failure) })
                })),
                proxy_token: None,
            })
            .unwrap();
            let error = native_account_call(account.clone(), "getAccount".into(), vec![])
                .await
                .unwrap_err();
            native_account_call(account, "dispose".into(), vec![])
                .await
                .unwrap();
            assert!(
                seen.borrow().is_empty(),
                "failed signing must not send HTTP"
            );
            assert_eq!(error["name"], NativeValue::String(name));
            assert_eq!(error["message"], NativeValue::String(message));
            assert_eq!(error["code"], NativeValue::String(vec![0xd800]));
        }
    });
}
