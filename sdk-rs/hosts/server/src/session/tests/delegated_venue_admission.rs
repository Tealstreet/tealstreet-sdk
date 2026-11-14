use super::*;

#[tokio::test]
async fn venue_declaration_controls_handover_requests() {
    for (bybit, binance, requested, accepted) in [
        (false, true, Some("binance"), true),
        (true, false, Some("binance"), false),
        (false, true, Some("bybit"), false),
        (false, true, None, false),
        (true, false, None, false),
        (true, true, None, false),
        (false, false, None, false),
        (true, false, Some("unknown"), false),
    ] {
        crate::delegated::with_test_declaration(
            json!({"venues": {
                "bybit": {"remoteSigning": {"enabled": bybit}},
                "binance": {"remoteSigning": {"enabled": binance}}
            }}),
            async {
                let (mut session, mut sent) =
                    user_session(crate::accounts::tests::registry(), "fixture");
                let data = requested.map_or(Value::Null, |venue| json!({"exchangeName": venue}));
                session
                    .handle(
                        &json!({"t":"msg", "d": {"op":"delegated-key-req",
                "accountName":"fixture", "data":data}})
                        .to_string(),
                    )
                    .await
                    .unwrap();
                let reply: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
                assert_eq!(
                    reply["d"]["op"] == "delegated-key",
                    accepted,
                    "bybit={bybit}, binance={binance}, requested={requested:?}: {reply}"
                );
                assert!(
                    session.joined.is_empty(),
                    "an offer grants no account admission"
                );
                session.close();
            },
        )
        .await;
    }
}
