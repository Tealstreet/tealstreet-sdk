//! SDK master dd9cae4: fees belong to the trading account, never the agent.
mod support;
use safe_cex_engine::{Action, Engine, HttpRequest};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::{META, kind, reply};

const AGENT: &str = "0x1111111111111111111111111111111111111111";
const MASTER: &str = "0x2222222222222222222222222222222222222222";
const VAULT: &str = "0x3333333333333333333333333333333333333333";

fn startup(options: Options) -> (Engine<Hyperliquid>, Vec<HttpRequest>) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(options));
    engine.start(1000.);
    let mut requests = Vec::new();
    for _ in 0..8 {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return (engine, requests);
        }
        for action in actions {
            if let Action::Http(request) = action {
                match kind(&request).as_str() {
                    "metaAndAssetCtxs" => reply(&mut engine, &request, 200, META),
                    "perpDexs" => reply(&mut engine, &request, 200, "[null]"),
                    _ => requests.push(request),
                }
            }
        }
    }
    panic!("bounded startup fixture did not settle");
}
fn subject(request: &HttpRequest) -> String {
    serde_json::from_str::<serde_json::Value>(request.body.as_deref().unwrap()).unwrap()["user"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn fee_subject_prefers_nonempty_vault_then_master_and_never_the_agent() {
    for (vault, expected) in [(None, MASTER), (Some(VAULT), VAULT), (Some(""), MASTER)] {
        let (mut engine, requests) = startup(Options {
            key: Some(AGENT.into()),
            application_id: Some(MASTER.into()),
            vault: vault.map(Into::into),
            ..Options::default()
        });
        let fees: Vec<_> = requests
            .iter()
            .filter(|request| kind(request) == "userFees")
            .collect();
        assert_eq!(fees.len(), 1);
        assert_eq!(subject(fees[0]), expected);
        reply(
            &mut engine,
            fees[0],
            200,
            r#"{"userCrossRate":"0.00032","userAddRate":"0.00012"}"#,
        );
        assert_eq!(
            engine.store().market("BTC-PERP").unwrap().taker_fee,
            Some(0.00032)
        );
        assert_eq!(
            engine.store().market("BTC-PERP").unwrap().maker_fee,
            Some(0.00012)
        );
    }
}

#[test]
fn public_or_unresolved_fee_subject_keeps_defaults_without_fee_http() {
    for options in [
        Options::default(),
        Options {
            application_id: Some("".into()),
            ..Options::default()
        },
        Options {
            public_data_only: true,
            key: Some(AGENT.into()),
            application_id: Some(MASTER.into()),
            vault: Some(VAULT.into()),
            ..Options::default()
        },
    ] {
        let (engine, requests) = startup(options);
        assert!(
            !requests.iter().any(|request| kind(request) == "userFees"),
            "no fees request without a private trading subject"
        );
        assert_eq!(
            engine.store().market("BTC-PERP").unwrap().taker_fee,
            Some(0.00045)
        );
        assert_eq!(
            engine.store().market("BTC-PERP").unwrap().maker_fee,
            Some(0.00015)
        );
    }
}

#[test]
fn empty_or_missing_application_id_resolves_the_agent_before_requesting_fees() {
    for application_id in [None, Some(String::new())] {
        let (mut engine, requests) = startup(Options {
            key: Some(AGENT.into()),
            application_id,
            ..Options::default()
        });
        assert!(!requests.iter().any(|request| kind(request) == "userFees"));
        let role = requests
            .iter()
            .find(|request| kind(request) == "userRole")
            .expect("agent discovery");
        assert_eq!(subject(role), AGENT);
        reply(
            &mut engine,
            role,
            200,
            &format!(r#"{{"role":"agent","data":{{"user":"{MASTER}"}}}}"#),
        );
        let fees = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Http(request) if kind(&request) == "userFees" => Some(request),
                _ => None,
            })
            .expect("resolved trading account fees");
        assert_eq!(subject(&fees), MASTER);
    }
}
