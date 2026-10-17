//! Error metadata through the actual registry constructor and native transport.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccountSpec, native_account_call, open_native_account,
};
use serde_json::json;
use tokio::net::TcpListener;

#[test]
fn native_transport_error_retains_axios_name_message_and_code() {
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
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .starts_with("connect ECONNREFUSED http://127.0.0.1:")
        );
        assert_eq!(error["code"], "ECONNREFUSED");
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}
