//! `stages_us.queued` means the account engine's queue accepted the command.
use super::publish_interval::{drain, setup};
use super::*;
use std::time::Duration;

fn capture() {
    crate::audit::CAPTURED.with(|lines| *lines.borrow_mut() = Some(Vec::new()));
}

/// The one order audit line captured since `capture`, and its stages.
fn audited_stages() -> serde_json::Map<String, Value> {
    let lines = crate::audit::CAPTURED.with(|lines| lines.borrow_mut().take().unwrap());
    let audits: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line["msg"] == "order audit")
        .collect();
    assert_eq!(audits.len(), 1, "{lines:?}");
    audits[0]["stages_us"].as_object().unwrap().clone()
}

async fn place(session: &mut Session, request: u32, order: Value) {
    session
        .handle(
            &json!({"t":"msg","d":{"op":"callMethod","accountName":"acct",
            "data":{"requestId":request,"method":"placeOrders","args":[[order]]}}})
            .to_string(),
        )
        .await
        .unwrap();
}

async fn done(sent: &mut crate::out::Receiver, request: u32) -> Value {
    for _ in 0..200 {
        if let Some(done) = drain(sent)
            .into_iter()
            .find(|v| v["d"]["op"] == "callMethod:done" && v["d"]["data"]["requestId"] == request)
        {
            return done["d"]["data"].clone();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("callMethod {request} never completed")
}

fn order() -> Value {
    json!({"symbol":"BTCUSDT","side":"buy","type":"limit","price":100,"amount":1})
}

#[tokio::test(start_paused = true)]
async fn an_accepted_write_records_its_queue_handoff() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut session, mut sent, account, _) = setup(None).await;
            capture();
            place(&mut session, 1, order()).await;
            assert_eq!(done(&mut sent, 1).await["success"], true);
            let stages = audited_stages();
            for stage in ["received", "admitted", "queued", "responded", "replied"] {
                assert!(stages[stage].is_u64(), "{stage}: {stages:?}");
            }
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_busy_account_queue_records_no_queue_handoff() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut session, mut sent, account, _) = setup(None).await;
            // The actor shares this LocalSet, so nothing drains until the test yields.
            let client = session.joined["acct"].client.id;
            let mut filler = 10_000;
            while account.handle.command(
                client,
                safe_cex_engine::Command::FetchAccount { command: filler },
            ) {
                filler += 1;
                assert!(filler < 20_000, "account queue never filled");
            }
            capture();
            place(&mut session, 2, order()).await;
            let reply = done(&mut sent, 2).await;
            assert_eq!(reply["success"], false);
            assert_eq!(
                reply["result"]["message"],
                "account is busy; command not sent"
            );
            let stages = audited_stages();
            assert!(!stages.contains_key("queued"), "{stages:?}");
            assert!(stages["admitted"].is_u64() && stages["replied"].is_u64());
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_engine_input_records_no_queue_handoff() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut session, mut sent, account, _) = setup(None).await;
            let mut untyped = order();
            untyped.as_object_mut().unwrap().remove("type");
            capture();
            place(&mut session, 3, untyped).await;
            let reply = done(&mut sent, 3).await;
            assert_eq!(reply["success"], false);
            assert!(
                reply["result"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("invalid command")),
                "{reply}"
            );
            let stages = audited_stages();
            assert!(!stages.contains_key("queued"), "{stages:?}");
            assert!(stages["admitted"].is_u64() && stages["replied"].is_u64());
            session.close();
            account.handle.dispose();
        })
        .await;
}
