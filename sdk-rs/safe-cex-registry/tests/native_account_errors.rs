//! Error metadata through the actual registry constructor and native transport.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccountSpec, native_account_call, open_native_account,
};
use serde_json::{Value, json};
use std::process::Command;
use std::sync::Arc;
use tokio::net::TcpListener;

#[test]
fn native_transport_error_retains_axios_name_message_and_code() {
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-exchange/tests/http-error-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    support::run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            exchange: "bitrue".into(),
            options: json!({"key":"native-key","secret":"native-secret","overrideProxyUrl":origin})
                .into(),
            account_name: None,
            sign: None,
            proxy_token: None,
        })
        .unwrap();
        let error = native_account_call(account.clone(), "validateAccount".into(), vec![])
            .await
            .unwrap_err();
        let error = support::boundary_json(error);
        assert_eq!(error["name"], "AxiosError");
        assert_eq!(error["message"], source["network"]["message"]);
        assert_eq!(error["code"], "ECONNREFUSED");
        assert_eq!(error["isAxiosError"], true);
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}

#[test]
fn native_http_bridge_retains_network_request_and_omits_setup_request() {
    use safe_cex_exchange::js_runtime::{client, value::Js};
    support::run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let host = client::rest_http_host(
            safe_cex_exchange::native::native_runtime(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let config = Js::object(vec![
            ("method", Js::str("get")),
            ("baseURL", Js::str(&origin)),
            ("url", Js::str("/network-proof")),
        ]);
        let error = host.send(config).await.unwrap_err();
        assert!(error.value.get("isAxiosError").unwrap().truthy());
        assert_eq!(
            error.value.get("code").unwrap().to_display(),
            "ECONNREFUSED"
        );
        let request = error.value.get("request").unwrap();
        let request = request
            .resource::<safe_cex_transport::RestRequest>()
            .expect("network rejection retains its real native transport request");
        assert_eq!(request.method, "get");
        assert_eq!(request.url, "/network-proof");
        assert_eq!(request.base_url.as_deref(), Some(origin.as_str()));
        let error = host
            .send(Js::object(vec![(
                "url",
                Js::str("unsupported://setup-proof"),
            )]))
            .await
            .unwrap_err();
        assert_eq!(
            error.value.get("code").unwrap().to_display(),
            "ERR_BAD_REQUEST"
        );
        assert!(
            !error.value.has_own("request"),
            "setup refusal has no request"
        );
    });
}
