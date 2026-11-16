//! Exact GRVT production strategy; no synthetic local-proof binding or venue I/O.
use super::*;
use safe_cex_crypto::admission::{
    Family,
    identity::Projection,
    wire::{Request, Response},
};
use std::time::Duration;

const SIGNER: &str = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";
fn init() -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "grvt".into(),
        key: "synthetic-grvt-api-key".into(),
        grvt_signer_address: Some(SIGNER.into()),
        grvt_trading_account_id: Some("42".into()),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("grvt-proof-registration".into()),
        ..Default::default()
    }
}
fn registry() -> Arc<Registry> {
    let mut registry = crate::accounts::tests::registry();
    crate::accounts::testing::set_join_admission(Arc::get_mut(&mut registry).unwrap(), true);
    registry
}
async fn challenge(sent: &mut crate::out::Receiver) -> Request {
    let text = tokio::time::timeout(Duration::from_secs(5), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "GRVT must prove its scalar, never API-key login"
    );
    let request = Request::from_json(&frame["d"].to_string()).unwrap();
    assert_eq!(request.account_name(), "acct");
    assert_eq!(request.instance_id(), "grvt-proof-registration");
    request
}
fn response(request: &Request, scalar: u8) -> String {
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::{KeyRef, Provider};
    let mut material = vec![0; 32];
    material[31] = scalar;
    let mut signer = LocalSigner::new();
    let key = KeyRef("fixture-grvt-scalar".into());
    signer
        .insert_declared(key.clone(), material, Family::Ethereum.algorithm())
        .unwrap();
    let provider: Provider = Arc::new(signer);
    let signature = request
        .statement()
        .sign(Family::Ethereum, &provider, &key, &[0; 32])
        .unwrap();
    let reply = Response::signed(request.id(), Family::Ethereum, &signature).unwrap();
    json!({"t":"sign-res","d":serde_json::to_value(reply).unwrap()}).to_string()
}
async fn complete(
    session: &mut Session,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
) {
    let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    session.verification_completed(completion).await.unwrap();
}
#[tokio::test]
async fn grvt_correct_scalar_uses_production_strategy_before_attachment() {
    tokio::task::LocalSet::new().run_until(async {
        let registry=registry();
        let resident=crate::accounts::testing::trading_account(&registry,"alice",&init());
        let (mut session,mut sent)=user_session(registry.clone(),"alice");
        let mut done=session.take_verified();
        session.init(init()).await.unwrap();
        assert!(session.pending_verifications.contains_key("acct"),"GRVT must enter fresh possession proof");
        assert!(session.joined.is_empty());
        assert!(!registry.holds(&resident,session.socket));
        let request=challenge(&mut sent).await;
        let facts=json!({"venue":"grvt","network":"mainnet","credential":{"kind":"grvt","tradingAccount":"42","signer":SIGNER}});
        let digest=Projection::from_json("alice",&facts.to_string()).unwrap().digest();
        assert_eq!(serde_json::to_value(request.statement()).unwrap()["identityDigest"],json!(digest));
        session.handle(&response(&request,1)).await.unwrap();
        complete(&mut session,&mut done).await;
        assert!(Arc::ptr_eq(&session.joined["acct"].account,&resident));
        assert!(registry.holds(&resident,session.socket));
        session.close(); resident.handle.dispose();
    }).await;
}
