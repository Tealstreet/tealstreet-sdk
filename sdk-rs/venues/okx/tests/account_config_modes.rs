//! The account config read gates every private read: each OKX account mode must either
//! configure the account or end the start visibly, never retry it forever.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Engine, Notice};
use safe_cex_venue_okx::Okx;

/// Starts a keyed account up to its config read, answers it with `config`, and
/// returns the actions that followed.
fn answer_config(config: &str) -> (Engine<Okx>, Vec<Action>) {
    let mut e = engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let mut out = prepared(&mut e, 1.0);
    let a = take_http(&mut out);
    reply(&mut e, &a, &envelope(TICKER), 2.0);
    out.extend(prepared(&mut e, 2.0));
    let a = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .expect("the config read")
        .clone();
    let body = if config.starts_with('{') && config.contains("\"code\"") {
        config.to_owned()
    } else {
        envelope(config)
    };
    reply(&mut e, &a, &body, 3.0);
    let after = prepared(&mut e, 3.0);
    (e, after)
}

fn private_reads_started(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.contains("/api/v5/account/") && !r.url.ends_with("/config")))
}

fn startup_failed(actions: &[Action]) -> Option<String> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::StartupFailed { error, .. }) => Some(error.message.clone()),
        _ => None,
    })
}

// OKX docs: acctLv 1 simple, 2 single-currency margin, 3 multi-currency margin, 4 portfolio margin.
#[test]
fn every_documented_account_level_with_a_position_mode_starts_its_private_reads() {
    for (level, mode) in [
        ("2", "net_mode"),
        ("2", "long_short_mode"),
        ("3", "net_mode"),
        ("4", "long_short_mode"),
        ("1", "net_mode"),
    ] {
        let (e, actions) = answer_config(&format!(
            r#"{{"uid":"1","acctLv":"{level}","posMode":"{mode}","autoLoan":false,"greeksType":"PA","level":"Lv1","ctIsoMode":"automatic","mgnIsoMode":"automatic"}}"#
        ));
        assert!(
            private_reads_started(&actions),
            "acctLv {level} {mode}: {actions:?}"
        );
        assert!(startup_failed(&actions).is_none(), "acctLv {level} {mode}");
        assert_eq!(
            e.store().hedged(),
            mode == "long_short_mode",
            "acctLv {level}"
        );
    }
}

#[test]
fn a_simple_mode_account_without_a_position_mode_trades_one_way_and_says_so_once() {
    for config in [
        r#"{"uid":"1","acctLv":"1","posMode":"","autoLoan":false}"#,
        r#"{"uid":"1","acctLv":"1","autoLoan":false}"#,
    ] {
        let (e, actions) = answer_config(config);
        assert!(private_reads_started(&actions), "{config}: {actions:?}");
        assert!(startup_failed(&actions).is_none(), "{config}");
        assert!(!e.store().hedged());
        let notes = actions
            .iter()
            .filter(|a| format!("{a:?}").contains("reports no position mode (acctLv 1)"))
            .count();
        assert_eq!(notes, 1, "{config}: {actions:?}");
    }
}

#[test]
fn an_unknown_position_mode_fails_the_start_naming_it() {
    let (_e, actions) = answer_config(r#"{"uid":"1","acctLv":"3","posMode":"portfolio_mode"}"#);
    assert!(!private_reads_started(&actions), "{actions:?}");
    let message = startup_failed(&actions).expect("a visible startup failure");
    assert!(
        message.contains("unknown position mode 'portfolio_mode' (acctLv 3)"),
        "{message}"
    );
}

#[test]
fn a_config_answer_with_no_account_fails_the_start_instead_of_retrying_forever() {
    let (_e, actions) = answer_config(r#"{"code":"0","msg":"","data":[]}"#);
    assert!(!private_reads_started(&actions), "{actions:?}");
    assert!(startup_failed(&actions).is_some(), "{actions:?}");
}
