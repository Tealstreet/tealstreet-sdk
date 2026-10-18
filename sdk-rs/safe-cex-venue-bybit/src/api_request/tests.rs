use super::*;
use safe_cex_crypto::{PayloadSigner, PayloadSignerRequest, PayloadSignerResult};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

struct Recorder(RefCell<Option<String>>);
impl PayloadSigner for Recorder {
    fn sign<'a>(
        &'a self,
        request: PayloadSignerRequest<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>> {
        *self.0.borrow_mut() = Some(request.payload.to_owned());
        Box::pin(async {
            Ok(PayloadSignerResult {
                signature: "signed".into(),
                headers: Some(BTreeMap::from([
                    ("X-Test".into(), "signer".into()),
                    ("X-Signer".into(), " \tclean\r\n\0\u{7f}value\t ".into()),
                ])),
            })
        })
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("recording signer must complete synchronously"),
    }
}
#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn request_signing_matches_live_interceptor() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/request-fixtures.mjs"
        ))
        .output()
        .expect("Node and installed TS/qs/axios required");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        fixtures["cases"].as_array().unwrap().len(),
        540,
        "REQUEST-CARDINALITY source total"
    );
    for (i, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let config = &case["config"];
        let options = &case["options"];
        let recorder = Recorder(RefCell::new(None));
        // Use a recording signer for the local-secret path too, to inspect exact signed bytes.
        let signable =
            options["payloadSigner"] == true || !options["secret"].as_str().unwrap().is_empty();
        let request = RestRequest {
            method: config["method"].as_str().unwrap().into(),
            url: config["url"].as_str().unwrap().into(),
            params: Some(config["params"].clone()),
            body: Some(safe_cex_transport::RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    config["data"].clone(),
                )
                .unwrap(),
            )),
            headers: config["headers"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                .collect(),
            ..RestRequest::default()
        };
        let result = ready(prepare_request(
            request,
            RequestSigning {
                exchange: ExchangeName::Bybit,
                credentials: SigningOptions {
                    key: options["key"].as_str().unwrap(),
                    secret: options["secret"].as_str().unwrap(),
                    payload_signer: signable.then_some(&recorder as &dyn PayloadSigner),
                },
                remote_signer: options["remoteSigner"] == true,
                recv_window: 20000.0,
                timestamp: &|| 1791518966503.0,
                request_id: "fixed-request-id",
                site_headers: options["siteHeaders"].as_object().unwrap(),
            },
        ))
        .unwrap();
        let expected = &case["expected"];
        assert_eq!(
            result.params.as_ref().unwrap(),
            &expected["params"],
            "params case {i}"
        );
        assert_eq!(
            serde_json::to_value(&result.headers).unwrap(),
            expected["headers"],
            "headers case {i}"
        );
        assert_eq!(
            result.skip_remote_signing,
            expected["skip"].as_bool().unwrap(),
            "skip case {i}"
        );
        assert_eq!(
            recorder.0.borrow().as_deref(),
            expected["payload"].as_str(),
            "payload case {i}"
        );
        if config["method"] == "post" && expected["payload"].is_string() {
            assert_eq!(
                String::from_utf8(result.body.unwrap().wire_bytes().unwrap()).unwrap(),
                expected["body"].as_str().unwrap(),
                "wire body case {i}"
            );
        }
    }
    assert_eq!(fixtures["statuses"].as_array().unwrap().len(), 27);
    let mut blocked = 0;
    for case in fixtures["statuses"].as_array().unwrap() {
        let cdn = blocked_by_cdn(&case["body"]);
        blocked += usize::from(cdn);
        assert_eq!(
            http_failure_message(
                case["status"].as_u64().unwrap() as u16,
                case["statusText"].as_str(),
                cdn
            ),
            case["message"].as_str().unwrap(),
            "status message {case}"
        );
    }
    assert_eq!(blocked, 18, "CloudFront body cases");
    assert_eq!(fixtures["paths"].as_array().unwrap().len(), 10);
    for case in fixtures["paths"].as_array().unwrap() {
        assert_eq!(
            header_value(case["raw"].as_str().unwrap()),
            case["shared"].as_str().unwrap()
        );
    }
}
