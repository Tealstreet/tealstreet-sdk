use super::*;

async fn order_frame(sent: &mut crate::out::Receiver, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            assert_ne!(frame["t"], "sign-req");
            if frame["t"] == kind {
                return frame;
            }
        }
    })
    .await
    .expect("bounded loopback order reply")
}

async fn submit(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    id: Option<&str>,
    seq: u64,
) -> String {
    let epoch = format!("{:012x}", crate::accounts::now_ms());
    let intent = format!("{}-{}-7101-8101-{seq:012x}", &epoch[..8], &epoch[8..]);
    let params =
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000});
    let fingerprint = crate::order_contract::fingerprint("acct", &intent, &params, seq).unwrap();
    session
        .handle(
            &json!({"t":"order.submit","d":{"accountName":"acct","instanceId":id,
        "intentId":intent,"params":params,"seq":seq,"ageMs":0,"fingerprint":fingerprint}})
            .to_string(),
        )
        .await
        .unwrap();
    let ack = order_frame(sent, "order.ack").await;
    assert_eq!(ack["d"]["intentId"], intent);
    assert_eq!(ack["d"]["seq"], seq);
    assert_eq!(ack["d"]["instanceId"].as_str(), id);
    assert_eq!(
        ack["d"]["received"], true,
        "current rebuilt registration admits the order"
    );
    let outcome = order_frame(sent, "order.outcome").await;
    assert_eq!(outcome["d"]["intentId"], intent);
    assert_eq!(outcome["d"]["seq"], seq);
    assert_eq!(outcome["d"]["instanceId"].as_str(), id);
    assert_eq!(outcome["d"]["outcome"], "placed");
    assert_eq!(outcome["d"]["venueOrderId"], "loopback-order");
    assert_eq!(outcome["d"]["clientOrderId"], intent);
    assert_eq!(
        outcome["d"]["outcomeSeq"], 1,
        "each replacement owns fresh receipts"
    );
    intent
}

#[tokio::test]
async fn apply_settings_keeps_rebuilt_orders_bound_and_previous_receipts_retired() {
    for id in [Some("current"), None] {
        let f = fixture().await;
        let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
        session.order_frames_enabled = true;
        let mut results = session.take_verified();
        let init = payload(&mut session, id);
        session.init(init).await.unwrap();
        finish(&mut session, &mut results).await;
        snapshot(&mut session, &mut sent, "acct").await;
        let before_intent = submit(&mut session, &mut sent, id, 1).await;
        let previous = session.joined["acct"].out.clone();
        let account = session.joined["acct"].account.clone();
        let settings = InitPayload {
            account_name: "acct".into(),
            exchange_name: "bybit".into(),
            key: "synthetic-key".into(),
            signing_mode: Some("delegated".into()),
            enable_spot_markets: Some(true),
            ..Default::default()
        };
        session
            .handle(
                &json!({"t":"msg","d":{"op":"applySettings","accountName":"acct",
            "data":{"requestId":7,"settings":settings}}})
                .to_string(),
            )
            .await
            .unwrap();
        let reply = frame(&mut sent, "applySettings:done").await;
        assert_eq!(reply["d"]["data"]["success"], true);
        assert_eq!(reply["d"]["instanceId"].as_str(), id);
        assert!(!Arc::ptr_eq(&account, &session.joined["acct"].account));
        assert!(!previous.order_binding(id));
        assert!(!previous.order_outcome("acct", json!({"intentId":"retired"})));
        snapshot(&mut session, &mut sent, "acct").await;
        let after_intent = submit(&mut session, &mut sent, id, 2).await;
        assert!(session.joined["acct"].out.order_binding(id));
        assert!(session.handover_out("acct").order_binding(id));
        let orders = f.orders.lock().unwrap().clone();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0]["symbol"], "BTCUSDT");
        assert_eq!(orders[1]["symbol"], "BTCUSDT");
        assert_eq!(orders[0]["orderLinkId"], before_intent);
        assert_eq!(orders[1]["orderLinkId"], after_intent);
        session.close();
    }
}
