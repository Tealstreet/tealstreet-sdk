use safe_cex_transport::signing::*;
use safe_cex_transport::{RestAdapter, RestBody, RestRequest, RestResponse, TransportFuture};
use safe_cex_types::{ExchangeName, retained_json::RetainedJsonInput};
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending recorder"),
    }
}
fn native_json_fixture(value: Value) -> RetainedJsonInput {
    RetainedJsonInput::parse_json(&value.to_string().encode_utf16().collect::<Vec<_>>()).unwrap()
}
struct Browser {
    expected: Value,
}
impl RemoteSigner for Browser {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        Box::pin(async move {
            let RemoteSignerRequest::Http(request) = request else {
                panic!("HTTP request required")
            };
            let expected = &self.expected["remote"];
            assert_eq!(
                request.config.method,
                expected["method"].as_str().unwrap(),
                "native remote method identity"
            );
            assert_eq!(
                request.config.url,
                expected["url"].as_str().unwrap(),
                "native remote URL identity"
            );
            assert_eq!(
                request.config.base_url,
                expected["baseURL"].as_str().unwrap(),
                "native remote baseURL identity"
            );
            assert!(
                expected.get("params").is_none(),
                "source remote params absent"
            );
            assert!(
                request.config.params.is_none(),
                "native remote params identity"
            );
            assert_eq!(
                request.config.data.as_ref(),
                expected
                    .get("data")
                    .cloned()
                    .map(native_json_fixture)
                    .as_ref(),
                "native remote data identity"
            );
            assert_eq!(
                request.config.headers.len(),
                expected["headers"].as_object().unwrap().len()
            );
            for (name, value) in &request.config.headers {
                assert_eq!(
                    Some(value.as_str()),
                    expected["headers"][name].as_str(),
                    "native remote header identity: {name}"
                );
            }
            assert_eq!(
                serde_json::to_value(&request.config).unwrap(),
                self.expected["remote"],
                "remote signer must receive server-transformed data and generated headers"
            );
            if self.expected["mode"] == "echo" {
                return Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                    id: request.id,
                    config: Some(SignedConfig {
                        url: Some(request.config.url),
                        params: request.config.params,
                        data: request.config.data,
                        headers: request.config.headers,
                    }),
                    error: None,
                }));
            }
            let mut client = RestRequest {
                method: request.config.method,
                url: request.config.url,
                base_url: Some(request.config.base_url),
                headers: request.config.headers,
                body: request
                    .config
                    .data
                    .map(try_parse_json)
                    .map(RestBody::Untransformed),
                params: request.config.params,
                ..Default::default()
            };
            let mut input = json!({"headers":client.headers});
            if let Some(body) = &client.body {
                let data = body
                    .untransformed()
                    .unwrap()
                    .json_stringify()
                    .unwrap()
                    .unwrap();
                input["data"] = serde_json::from_str(&data).unwrap();
            }
            assert_eq!(
                input, self.expected["clientInput"],
                "browser signing input must match actual client interceptors"
            );
            client.transform_axios_request()?;
            Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                config: Some(SignedConfig {
                    url: Some(client.url),
                    params: client.params,
                    data: client
                        .body
                        .as_ref()
                        .map(|body| body.untransformed().unwrap().clone()),
                    headers: client.headers,
                }),
                error: None,
            }))
        })
    }
}
#[derive(Default)]
struct Wire(Mutex<Vec<RestRequest>>);
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.0.lock().unwrap().push(request);
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(vec![]),
            })
        })
    }
}
#[test]
#[ignore = "mandatory core CI step installs frozen real nanoid and executes ignored oracles"]
fn remote_stage_matches_whole_source_adapter_and_real_axios() {
    let output = std::process::Command::new("node")
        .arg("scripts/remote-stage-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 54);
    for row in rows {
        let browser = Arc::new(Browser {
            expected: row.clone(),
        });
        let wire = Arc::new(Wire::default());
        let adapter = create_remote_signing_adapter(
            browser,
            ExchangeName::Kucoin,
            None,
            Arc::new(|| "id".into()),
            Arc::new(|| 0),
            wire.clone(),
        );
        let mut request = RestRequest {
            method: "post".into(),
            url: "/data".into(),
            base_url: Some("https://example.test".into()),
            body: row
                .get("data")
                .cloned()
                .map(native_json_fixture)
                .map(RestBody::Untransformed),
            ..Default::default()
        };
        request
            .headers
            .insert("Accept".into(), "application/json, text/plain, */*".into());
        if let Some(ct) = row.get("contentType").and_then(Value::as_str) {
            request.headers.insert("Content-Type".into(), ct.into());
        }
        run(adapter.execute(request)).unwrap();
        let requests = wire.0.lock().unwrap();
        let sent = &requests[0];
        assert_eq!(
            serde_json::to_value(&sent.headers).unwrap(),
            row["wire"]["headers"],
            "remote final headers must match whole source adapter"
        );
        assert_eq!(
            sent.body.as_ref().map(|body| body
                .untransformed()
                .expect("remote inner adapter must receive structured data")),
            row["wire"]
                .get("data")
                .cloned()
                .map(native_json_fixture)
                .as_ref(),
            "remote inner adapter must retain echoed object versus string identity: {row}"
        );
    }
}
