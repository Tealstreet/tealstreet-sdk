use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::margin_mode::change_binance_pm_margin_type;
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn pm_margin_mode_rejection_matches_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/margin-mode.mjs"
        ))
        .output()
        .expect("Node PM margin-mode oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 20);
    for (index, case) in cases.iter().enumerate() {
        let mut trace = vec![];
        let mut sentinel = ExchangeError::new("fixture emitter");
        sentinel.name = "RangeError".into();
        sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "fixture-code"
        )));
        sentinel.response = Some(json!({"status":503,"data":{"msg":"detail"}}));
        let error = {
            let mut future = std::pin::pin!(change_binance_pm_margin_type(|message| {
                trace.push(message.to_owned());
                if case["emitReject"] == true {
                    Err(sentinel.clone())
                } else {
                    Ok(())
                }
            }));
            match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(Err(error)) => error,
                other => panic!("PM margin mode must reject immediately: {other:?}"),
            }
        };
        assert_eq!(
            trace,
            serde_json::from_value::<Vec<String>>(case["trace"].clone()).unwrap(),
            "PM margin emits once before rejection case{index}"
        );
        assert_eq!(
            json!({"name":error.name,"message":error.message,"same":error==sentinel,"code":observed_error_code(&error.code),"response":error.response}),
            case["error"],
            "PM margin error precedence and metadata case{index}"
        );
    }
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
