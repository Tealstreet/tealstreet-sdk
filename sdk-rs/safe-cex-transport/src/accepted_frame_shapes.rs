use crate::frames::{Config, HttpRequestArgs, RequestDump, ResponseData, RpcHttpResponse};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub(crate) fn record<T: DeserializeOwned>(object: Value, array: Value, observe: impl Fn(T)) {
    for decoded in [
        serde_json::from_str::<T>(&object.to_string()).unwrap(),
        serde_json::from_value::<T>(object.clone()).unwrap(),
        T::deserialize(&object).unwrap(),
        <T as Deserialize>::deserialize(object).unwrap(),
    ] {
        observe(decoded);
    }
    assert!(
        serde_json::from_str::<T>(&array.to_string()).is_err(),
        "complete ordered array must be refused by raw frame decoder"
    );
    assert!(
        serde_json::from_value::<T>(array.clone()).is_err(),
        "complete ordered array must be refused by owned frame decoder"
    );
    assert!(
        T::deserialize(&array).is_err(),
        "complete ordered array must be refused by borrowed frame decoder"
    );
    assert!(
        <T as Deserialize>::deserialize(array).is_err(),
        "complete ordered array must be refused by qualified frame decoder"
    );
}

#[test]
fn config_requires_object_and_preserves_native_url() {
    record::<Config>(
        json!({"url":"native-config-url"}),
        json!(["native-config-url"]),
        |row| assert_eq!(row.url, "native-config-url"),
    );
}

#[test]
fn http_request_requires_object_and_preserves_native_fields() {
    record::<HttpRequestArgs<Value>>(
        json!({"headers":{"X-Witness":"request-header"},"method":"PATCH","url":"native-request-url","data":{"id":"9007199254740993","quantity":2.5},"params":{"limit":"7"}}),
        json!([{"X-Witness":"request-header"}, "PATCH", "native-request-url", {"id":"9007199254740993","quantity":2.5}, {"limit":"7"}]),
        |row| {
            assert_eq!(row.headers["X-Witness"], json!("request-header"));
            assert_eq!(row.method, "PATCH");
            assert_eq!(row.url, "native-request-url");
            let body = row.data.unwrap();
            assert_eq!(body["id"], json!("9007199254740993"));
            assert_eq!(
                body["quantity"].as_f64().unwrap().to_bits(),
                2.5_f64.to_bits()
            );
            assert_eq!(row.params["limit"], json!("7"));
        },
    );
    for suffix in ["", r#", "data":null"#] {
        let raw = format!(r#"{{"headers":{{}},"method":"GET","url":"x","params":{{}}{suffix}}}"#);
        assert!(
            serde_json::from_str::<HttpRequestArgs<Value>>(&raw)
                .unwrap()
                .data
                .is_none()
        );
    }
}

#[test]
fn generic_request_data_still_borrows_from_the_original_json() {
    let raw =
        r#"{"headers":{},"method":"GET","url":"borrow-url","params":{},"data":"borrowed-payload"}"#;
    let row: HttpRequestArgs<&str> = serde_json::from_str(raw).unwrap();
    let payload = row.data.unwrap();
    assert_eq!(payload, "borrowed-payload");
    let begin = raw.find("borrowed-payload").unwrap();
    assert_eq!(
        payload.as_ptr(),
        raw[begin..].as_ptr(),
        "generic data must retain the source borrow"
    );
}

fn dump() -> Value {
    json!({"method":"DELETE","url":"native-dump-url","headers":{"X-Dump":"dump-header"},"body":{"payload":"body-witness"},"form_data":{"first":"one","second":"two"},"raw_form_data":"first=one&second=two"})
}

fn observe_dump(row: RequestDump) {
    assert_eq!(row.method, "DELETE");
    assert_eq!(row.url, "native-dump-url");
    assert_eq!(row.headers["X-Dump"], "dump-header");
    assert_eq!(row.body.unwrap()["payload"], json!("body-witness"));
    let form = row.form_data.unwrap();
    assert_eq!(form["first"], "one");
    assert_eq!(form["second"], "two");
    assert_eq!(row.raw_form_data.as_deref(), Some("first=one&second=two"));
}

#[test]
fn request_dump_requires_object_and_preserves_native_fields() {
    record::<RequestDump>(
        dump(),
        json!(["DELETE", "native-dump-url", {"X-Dump":"dump-header"}, {"payload":"body-witness"}, {"first":"one","second":"two"}, "first=one&second=two"]),
        observe_dump,
    );
}

fn response() -> Value {
    json!({"config":{"url":"response-config-url"},"data":{"result":"native-data"},"data_raw":"native-raw","data_encoded":"bmF0aXZl","data_encoding":"base64","status":207,"status_text":"Multi-Status","headers":{"X-Response":"response-header"},"request_dump":dump()})
}

fn observe_response(row: ResponseData) {
    assert_eq!(row.config.url, "response-config-url");
    assert_eq!(row.data.unwrap()["result"], json!("native-data"));
    assert_eq!(row.data_raw.as_deref(), Some("native-raw"));
    assert_eq!(row.data_encoded.as_deref(), Some("bmF0aXZl"));
    assert_eq!(row.data_encoding.as_deref(), Some("base64"));
    assert_eq!(row.status, 207);
    assert_eq!(row.status_text, "Multi-Status");
    assert_eq!(row.headers["X-Response"], "response-header");
    observe_dump(row.request_dump.unwrap());
}

#[test]
fn response_data_requires_object_and_preserves_native_fields() {
    record::<ResponseData>(
        response(),
        json!([{"url":"response-config-url"}, {"result":"native-data"}, "native-raw", "bmF0aXZl", "base64", 207, "Multi-Status", {"X-Response":"response-header"}, dump()]),
        observe_response,
    );
}

#[test]
fn rpc_response_requires_object_and_preserves_native_fields() {
    record::<RpcHttpResponse>(
        json!({"type":"rpc.http","nonce":"native-nonce","data":response()}),
        json!(["rpc.http", "native-nonce", response()]),
        |row| {
            assert_eq!(row.response_type, "rpc.http");
            assert_eq!(row.nonce, "native-nonce");
            observe_response(row.data);
        },
    );
}
