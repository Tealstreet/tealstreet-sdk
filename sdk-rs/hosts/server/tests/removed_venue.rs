mod support;

use safe_cex_exchange_server::accounts::testing;
use safe_cex_exchange_server::auth::{Verifier, unsigned_token};
use safe_cex_exchange_server::envelope::InitPayload;
use serde_json::json;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn unavailable_hosted_accounts_refuse_typed_without_disturbing_live_accounts() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let app = support::app(
                support::config(Duration::from_secs(1)),
                Verifier::unsigned(),
            );
            let live = InitPayload {
                account_name: "live-bybit".into(),
                exchange_name: "bybit".into(),
                key: "fixture-key".into(),
                instance_id: Some("live-registration".into()),
                ..Default::default()
            };
            testing::trading_account(&app.registry, "saved-accounts", &live);
            let address = support::serve(app.clone()).await;
            let expires = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600;
            let token = unsigned_token("saved-accounts", expires);
            let mut page = support::Page::open(&address, &token, None).await;
            page.init(&live).await;
            page.until("frame", |d| d["accountName"] == live.account_name)
                .await;

            // restored by #3502; #3317 dropped a live venue without a product decision.
            for venue in [
                "bitmex",
                "bitmextestnet",
                "orderly",
                "polymarket",
                "txflow",
                "weex",
            ] {
                for (public_only, same_name) in
                    [(true, false), (false, false), (true, true), (false, true)]
                {
                    let removed = InitPayload {
                        account_name: if same_name {
                            live.account_name.clone()
                        } else {
                            format!("saved-{venue}-{public_only}")
                        },
                        instance_id: Some(format!("refused-{venue}-{public_only}-{same_name}")),
                        exchange_name: venue.into(),
                        key: "fixture-key".into(),
                        public_only: Some(public_only),
                        tealstreet_proxy: Some(true),
                        ..Default::default()
                    };
                    page.init(&removed).await;
                    let refusal = page
                        .until("error", |d| d["accountName"] == removed.account_name)
                        .await;
                    assert_eq!(refusal["instanceId"], removed.instance_id.clone().unwrap());
                    assert_eq!(
                        refusal["data"],
                        json!([
                            format!("unsupported venue: {venue}"),
                            {
                                "forceToast": true,
                                "initRefused": true,
                                "kind": "unsupported",
                                "outcome": "notSent"
                            }
                        ]),
                        "{venue}, public_only={public_only}, same_name={same_name}"
                    );
                }
            }

            page.send(json!({"t":"order.submit","d":{
                "accountName":live.account_name,"instanceId":"live-registration",
                "intentId":"invalid","params":{},"seq":1,"ageMs":0,"fingerprint":""
            }}))
            .await;
            let receipt = page.until("order.outcome", |_| true).await;
            assert_eq!(receipt["instanceId"], "live-registration");
            assert_eq!(receipt["outcomeSeq"], 1);
            assert_eq!(receipt["outcome"], "refused");

            page.send(json!({"t": "ping", "d": {}})).await;
            assert_eq!(
                page.until("pong", |_| true).await["accounts"],
                json!([live.account_name])
            );
            page.op(&live.account_name, "sendSnapshot", json!({})).await;
            page.until("frame", |d| d["accountName"] == live.account_name)
                .await;
            page.close().await;
        })
        .await;
}
