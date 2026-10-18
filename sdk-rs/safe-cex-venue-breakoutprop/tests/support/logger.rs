fn logger_error_fixture() -> ExchangeError {
    let mut e = ExchangeError::new("formatter subscriber failed");
    e.name = "SubscriberFailure".into();
    e.store_subscriber_ids = vec![17, 29];
    e.is_axios_error = true;
    e.request = Some(json!({"id":313}));
    e.stack = Some("source-stack".into());
    e = e.with_retained_response((json!({"data":{"msg":"nested-message"}})).into());
    e.context = Some(json!({"phase":"formatter"}));
    e.config = Some(json!({"url":"/sample"}));
    e.enhanced_message = Some("enhanced-report".into());
    e.url = Some("/failure".into());
    e.method = Some("PATCH".into());
    e.original_error = Some(json!({"sentinel":true}));
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "SAMPLE_CODE"
    )));
    e
}
fn logger_error_observe(e: &ExchangeError) -> Value {
    json!({"message":e.message,"name":e.name,"store_subscriber_ids":e.store_subscriber_ids,
        "is_axios_error":e.is_axios_error,"request":e.request,"stack":e.stack,
        "response":e.response_json_view(),"context":e.context,"config":e.config,
        "enhanced_message":e.enhanced_message,"url":e.url,"method":e.method,
        "original_error":e.original_error,"code":observed_error_code(&e.code)})
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
