//! Real native dispatch on a production TxFlow account. Its Phase-0 history
//! refusal distinguishes accepted options from argument rejection without IO.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccount, NativeAccountSpec, native_account_call, open_native_account,
};
use safe_cex_registry::native_value::NativeValue;
use serde_json::json;
use std::rc::Rc;

fn account() -> Rc<NativeAccount> {
    open_native_account(NativeAccountSpec {
        hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange: "txflow".into(),
        options: json!({"key":"", "secret":""}).into(),
        account_name: None,
        sign: None,
        proxy_token: None,
    })
    .unwrap()
}
fn invalid_argument(result: &Result<NativeValue, NativeValue>) -> bool {
    result.as_ref().err().is_some_and(|error| {
        error["message"].to_js().as_str().is_some_and(|message| {
            String::from_utf16_lossy(message).starts_with("Invalid argument 0:")
        })
    })
}

#[test]
fn order_id_records_require_objects_at_the_real_native_entrypoint() {
    support::run_local(async {
        let account = account();
        for input in [
            json!([["77"]]),
            json!([null]),
            json!([{}]),
            json!([{"id":7}]),
        ] {
            let result = native_account_call(
                account.clone(),
                "removeOrders".into(),
                vec![input.clone().into()],
            )
            .await;
            assert!(
                invalid_argument(&result),
                "native_account_records::order_object {input}: {result:?}"
            );
        }
        assert_eq!(
            native_account_call(
                account.clone(),
                "removeOrders".into(),
                vec![json!([{"id":"absent", "extra":[1]}]).into()]
            )
            .await,
            Ok(NativeValue::Null)
        );
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}

#[test]
fn historical_options_require_an_object_before_the_real_venue_refusal() {
    support::run_local(async {
        let account = account();
        for input in [
            json!(["account", 1, 2]),
            json!(["account", null, null]),
            json!({}),
            json!({"account":7}),
            json!({"account":"account","from":"1"}),
        ] {
            let result = native_account_call(
                account.clone(),
                "fetchHistoricalPositions".into(),
                vec![input.clone().into()],
            )
            .await;
            assert!(
                invalid_argument(&result),
                "native_account_records::history_object {input}: {result:?}"
            );
        }
        for input in [
            json!({"account":"account"}),
            json!({"account":"account","from":null,"to":null,"extra":true}),
            json!({"account":"account","from":1,"to":2}),
        ] {
            let result = native_account_call(
                account.clone(),
                "fetchHistoricalPositions".into(),
                vec![input.clone().into()],
            )
            .await;
            assert!(
                !invalid_argument(&result),
                "native_account_records::history_accepted {input}: {result:?}"
            );
            let error = support::boundary_json(result.unwrap_err());
            assert!(
                error["message"]
                    .as_str()
                    .unwrap()
                    .contains("fetchHistoricalPositions"),
                "the real venue's existing refusal must be reached: {error}"
            );
        }
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}
