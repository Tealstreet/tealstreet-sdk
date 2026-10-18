use safe_cex_transport::{RestBody, RestRequest};
use serde_json::Value;

#[test]
fn explicit_form_transform_matches_live_axios_signing_and_wire_views() {
    let output = std::process::Command::new("node")
        .arg("scripts/form-transform-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 19);
    for row in rows {
        let mut request = RestRequest {
            method: "post".into(),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(row["data"].clone())
                    .unwrap(),
            )),
            ..Default::default()
        };
        request.headers.insert(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        );
        assert_eq!(
            request.body.as_ref().unwrap().untransformed().unwrap(),
            &safe_cex_types::retained_json::RetainedJsonInput::from_value(row["signed"].clone())
                .unwrap(),
            "form signing input must remain structured"
        );
        let result = request.transform_axios_request();
        if row.get("error").is_some() {
            assert!(
                result.is_err(),
                "form coercion refusal must not transmit: {row}"
            );
            continue;
        }
        let wire = result.unwrap();
        let bytes = wire.body.as_ref().unwrap().wire_bytes().unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            row["wire"].as_str().unwrap(),
            "form wire encoding must match Axios including plus spaces: {row}"
        );
        assert_eq!(
            request.body.as_ref().unwrap().untransformed().unwrap(),
            &safe_cex_types::retained_json::RetainedJsonInput::from_value(row["wire"].clone())
                .unwrap(),
            "retry signing must receive the actual form string"
        );
        let retry = request.transform_axios_request().unwrap();
        assert_eq!(
            retry.body, wire.body,
            "form retry identity must not encode twice"
        );
    }
}
