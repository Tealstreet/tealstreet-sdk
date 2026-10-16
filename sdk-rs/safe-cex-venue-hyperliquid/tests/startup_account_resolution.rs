#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/startup_account_host.rs"]
mod support;
use account::get;
use cache::{N, eq, object, s};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperObserver, startup_account_resolution::*, ws_ping_lifecycle::WsPingHost,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use support::{Host, assert_value_local};
const MODE: [&str; 12] = [
    "unifiedAccount",
    "portfolioMargin",
    "disabled",
    "default",
    "dexAbstraction",
    "standard",
    "empty",
    "post-error",
    "log-error",
    "timeout",
    "no-user",
    "application-only",
];
const ROLE: [&str; 15] = [
    "user",
    "agent",
    "agent-missing",
    "subAccount",
    "subAccount-missing",
    "vault",
    "missing",
    "unknown",
    "post-error",
    "no-user",
    "existing",
    "existing-vault",
    "swap-options",
    "type-mutates-key",
    "timeout",
];
const REFRESH: [&str; 14] = [
    "baseline",
    "empty",
    "post-error",
    "handler-error",
    "log-error",
    "disposed",
    "no-track",
    "no-user",
    "throttle",
    "force",
    "concurrent",
    "drop",
    "application-only",
    "sync-throw",
];
fn poll(f: &mut HelperObserver<N, N>) -> Poll<Result<N, N>> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}
fn role_data(scene: &str) -> N {
    object(vec![
        (
            "role",
            s(match scene {
                "agent-missing" => "agent",
                "subAccount-missing" => "subAccount",
                "swap-options" | "type-mutates-key" | "timeout" => "user",
                other => other,
            }),
        ),
        (
            "data",
            object(vec![
                (
                    "user",
                    if scene == "agent" {
                        s("agent-master")
                    } else {
                        N::U
                    },
                ),
                (
                    "master",
                    if scene == "subAccount" {
                        s("sub-master")
                    } else {
                        N::U
                    },
                ),
            ]),
        ),
    ])
}
fn diagnostic(
    state: &support::State,
    index: usize,
    channel: &str,
    message: &str,
    severity: Option<&str>,
) {
    assert_eq!(
        state.channels[index], channel,
        "BOOT native diagnostic channel"
    );
    let args = &state.effects[index];
    assert_eq!(
        args.len(),
        if severity.is_some() { 2 } else { 1 },
        "BOOT independent diagnostic argument count"
    );
    assert_value_local(
        &args[0],
        &s(message),
        "BOOT native exact diagnostic message",
    );
    if let Some(level) = severity {
        assert_value_local(&args[1], &s(level), "BOOT native exact diagnostic severity")
    }
}
#[test]
fn primary_bootstrap_native_state_aliases_and_source() {
    let mut completed = 0;
    let mut last = None;
    for (family, scenes) in [
        ("mode", MODE.as_slice()),
        ("role", ROLE.as_slice()),
        ("refresh", REFRESH.as_slice()),
    ] {
        for &scene in scenes {
            let data = match family {
                "mode" => {
                    if scene == "empty" {
                        N::Null
                    } else {
                        s(
                            if [
                                "post-error",
                                "log-error",
                                "timeout",
                                "no-user",
                                "application-only",
                            ]
                            .contains(&scene)
                            {
                                "unifiedAccount"
                            } else {
                                scene
                            },
                        )
                    }
                }
                "role" => role_data(scene),
                _ => {
                    if scene == "empty" {
                        N::Null
                    } else {
                        object(vec![(
                            "balances",
                            cache::arr(vec![object(vec![("coin", s("USDC")), ("total", s("17"))])]),
                        )])
                    }
                }
            };
            let mut h = Host::new(family, scene, data);
            let e = h.base.exchange.clone();
            let original_options = h.state.borrow().original_options.clone();
            if scene == "disposed" {
                h.set(&e, "isDisposed", &N::B(true)).unwrap()
            }
            let mut observer = match family {
                "mode" => resolve_user_abstraction_mode(h.clone(), e.clone()),
                "role" => resolve_application_id_from_key(h.clone(), e.clone()),
                _ => refresh_unified_spot_collateral_balance(
                    h.clone(),
                    e.clone(),
                    N::B(["force", "concurrent"].contains(&scene)),
                ),
            };
            let early = [
                "no-user",
                "existing",
                "existing-vault",
                "no-track",
                "throttle",
            ]
            .contains(&scene);
            if !early {
                assert_eq!(
                    h.state.borrow().posted,
                    1,
                    "BOOT native eager post before observer poll {family} {scene}"
                );
                assert_eq!(
                    h.state.borrow().trace,
                    if family == "refresh" {
                        vec!["post"]
                    } else {
                        vec!["timeout-get", "post"]
                    },
                    "BOOT native timeout getter before eager request"
                );
                assert!(
                    poll(&mut observer).is_pending(),
                    "BOOT native first await suspension {family} {scene}"
                );
            }
            let mut second = None;
            if scene == "concurrent" {
                let original = get(&e, "spotCollateralRefreshInFlight");
                second = Some(refresh_unified_spot_collateral_balance(
                    h.clone(),
                    e.clone(),
                    N::B(true),
                ));
                assert!(
                    eq(&get(&e, "spotCollateralRefreshInFlight"), &original),
                    "BOOT native shared collateral flight identity"
                );
                assert_eq!(
                    h.state.borrow().posted,
                    1,
                    "BOOT native concurrent request suppression"
                );
            }
            let replacement = object(vec![("key", s("replacement-key"))]);
            if scene == "swap-options" {
                h.set(&e, "options", &replacement).unwrap()
            }
            let dropped = scene == "drop";
            let mut observer = if dropped {
                drop(observer);
                None
            } else {
                Some(observer)
            };
            h.base.drain();
            if let Some(f) = observer.as_mut() {
                let Poll::Ready(result) = poll(f) else {
                    panic!("BOOT observer must settle {family} {scene}")
                };
                if scene == "log-error" {
                    let err = result.expect_err("BOOT native throwing logger escapes");
                    assert!(
                        eq(&err, &h.state.borrow().log_error),
                        "BOOT original logger error identity"
                    );
                } else {
                    assert!(
                        matches!(result, Ok(N::U)),
                        "BOOT native void result {family} {scene}: {result:?}"
                    )
                }
            }
            if let Some(f) = second.as_mut() {
                assert!(
                    matches!(poll(f), Poll::Ready(Ok(N::U))),
                    "BOOT native shared flight adopted void"
                )
            }
            let state = h.state.borrow();
            assert_eq!(
                state.posted,
                if early { 0 } else { 1 },
                "BOOT independent native HTTP count {family} {scene}"
            );
            if family == "mode" && !early && scene != "log-error" {
                let expected = if scene == "application-only" {
                    "unifiedAccount"
                } else if [
                    "unifiedAccount",
                    "portfolioMargin",
                    "disabled",
                    "default",
                    "dexAbstraction",
                ]
                .contains(&scene)
                {
                    scene
                } else {
                    "standard"
                };
                assert_value_local(
                    &get(&e, "userAbstractionMode"),
                    &s(expected),
                    "BOOT native abstraction mode assignment",
                );
                let expected_logs = if ["standard", "empty", "post-error"].contains(&scene) {
                    2
                } else {
                    1
                };
                assert_eq!(
                    state.logs, expected_logs,
                    "BOOT independent mode diagnostic count"
                );
                assert_eq!(
                    state.channels.last(),
                    Some(&"log"),
                    "BOOT native detected-mode log channel"
                );
                let final_log = state.effects.last().unwrap();
                assert_value_local(
                    &final_log[0],
                    &s(&format!(
                        "Detected Hyperliquid account abstraction mode: {expected}"
                    )),
                    "BOOT native detected-mode diagnostic",
                );
                assert_value_local(
                    &final_log[1],
                    &s(if scene == "portfolioMargin" {
                        "info"
                    } else {
                        "debug"
                    }),
                    "BOOT native detected-mode severity",
                );
                if scene == "standard" || scene == "empty" {
                    assert_value_local(
                        &state.effects[0][0],
                        &s(&format!(
                            "userAbstraction returned unexpected mode={}; defaulting to standard",
                            if scene == "empty" { "" } else { "standard" }
                        )),
                        "BOOT native unexpected mode diagnostic",
                    )
                }
            }
            if family == "mode" && ["post-error", "log-error"].contains(&scene) {
                diagnostic(
                    &state,
                    0,
                    "log",
                    "userAbstraction resolution failed: [object Object]",
                    Some("warning"),
                );
            }
            if family == "role" {
                if !early && scene != "timeout" {
                    let (channel, message, level) = match scene {
                        "user" | "type-mutates-key" | "swap-options" => (
                            "log",
                            "Resolved applicationId from userRole=user (key is master wallet)",
                            Some("info"),
                        ),
                        "agent" => (
                            "log",
                            "Resolved applicationId from userRole=agent: master=agent-master",
                            Some("info"),
                        ),
                        "subAccount" => (
                            "log",
                            "Resolved applicationId from userRole=subAccount: master=sub-master",
                            Some("info"),
                        ),
                        "vault" => (
                            "log",
                            "Resolved vault from userRole=vault (key is vault wallet)",
                            Some("info"),
                        ),
                        "agent-missing" => (
                            "emit",
                            "Hyperliquid recognises this wallet as an agent but did not return its master address. Please re-add the account.",
                            None,
                        ),
                        "subAccount-missing" => (
                            "emit",
                            "Hyperliquid recognises this wallet as a subaccount but did not return its master address. Please re-add the account.",
                            None,
                        ),
                        "missing" => (
                            "emit",
                            "Hyperliquid does not recognise wallet key. The wallet has no deposits, agent approvals, or subaccount linkage on this network. Please re-add the account using the correct wallet address (and main wallet if this is an agent key).",
                            None,
                        ),
                        "post-error" => (
                            "log",
                            "userRole resolution failed: [object Object]",
                            Some("warning"),
                        ),
                        "unknown" => (
                            "log",
                            "userRole returned unexpected role=unknown; leaving applicationId unresolved",
                            Some("warning"),
                        ),
                        _ => panic!("BOOT unexpected role diagnostic scene"),
                    };
                    diagnostic(&state, 0, channel, message, level);
                }
                let options = get(&e, "options");
                if scene == "swap-options" {
                    assert!(
                        eq(&options, &replacement),
                        "BOOT native replaced options identity"
                    );
                    assert_value_local(
                        &get(&options, "applicationId"),
                        &s("replacement-key"),
                        "BOOT native fresh post-await key",
                    );
                    assert_value_local(
                        &get(&original_options, "applicationId"),
                        &s(""),
                        "BOOT original options left unpatched",
                    );
                } else {
                    assert!(
                        eq(&options, &original_options),
                        "BOOT native in-place options identity"
                    );
                    let expected = match scene {
                        "user" => Some("key"),
                        "type-mutates-key" => Some("new-key"),
                        "agent" => Some("agent-master"),
                        "subAccount" => Some("sub-master"),
                        "existing" => Some("existing"),
                        _ => None,
                    };
                    if let Some(expected) = expected {
                        assert_value_local(
                            &get(&options, "applicationId"),
                            &s(expected),
                            "BOOT native role applicationId",
                        )
                    } else {
                        assert_value_local(
                            &get(&options, "applicationId"),
                            &s(""),
                            "BOOT native unresolved applicationId",
                        )
                    }
                    if scene == "vault" {
                        assert_value_local(
                            &get(&options, "vault"),
                            &s("key"),
                            "BOOT native vault routing",
                        )
                    }
                }
                assert_eq!(
                    state.logs,
                    if early || scene == "timeout" { 0 } else { 1 },
                    "BOOT independent role diagnostic count"
                );
            }
            if family == "refresh" {
                if ["post-error", "handler-error", "log-error", "sync-throw"].contains(&scene) {
                    diagnostic(
                        &state,
                        usize::from(scene == "handler-error"),
                        "log",
                        "Unified spot collateral refresh failed: [object Object]",
                        Some("warning"),
                    );
                }
                if scene == "sync-throw" {
                    assert!(
                        h.truthy(&get(&e, "spotCollateralRefreshInFlight")),
                        "BOOT source synchronous failure leaves assigned settled flight"
                    )
                } else {
                    assert!(
                        matches!(get(&e, "spotCollateralRefreshInFlight"), N::Null),
                        "BOOT native finally clears collateral flight"
                    )
                }
                assert_value_local(
                    &get(&e, "lastSpotCollateralRefreshAt"),
                    &N::Num(if early {
                        if scene == "throttle" { 99999. } else { 0. }
                    } else {
                        100000.
                    }),
                    "BOOT native collateral refresh timestamp",
                );
                assert_eq!(
                    state.effects.len(),
                    if early || scene == "disposed" {
                        0
                    } else if scene == "handler-error" {
                        2
                    } else {
                        1
                    },
                    "BOOT independent collateral effect count"
                );
            }
            completed += 1;
            last = Some((family, scene));
        }
    }
    assert_eq!(
        completed, 41,
        "BOOT independent completed native case floor"
    );
    assert_eq!(
        last,
        Some(("refresh", "sync-throw")),
        "BOOT native last completed case index40"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/startup-account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "BOOT live source assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["cases"], 41, "BOOT independent actual source floor");
    assert_eq!(
        v["last"], "refresh/sync-throw",
        "BOOT actual source last case"
    );
}
