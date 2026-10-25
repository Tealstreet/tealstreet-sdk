use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::test_support::{ProbeVenue, prepared_actions};
use safe_cex_engine::{Action, CredentialSlot};
const PRIMARY_SECRET: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
const READ_SECRET: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";
fn setup() -> (Options, safe_cex_engine::signing::Provider) {
    let (signer, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("primary-key".into()),
            secret: Secret::new(PRIMARY_SECRET.into()),
            ..Default::default()
        }),
        secondary: Some(LocalIdentity {
            key: Secret::new("read-key".into()),
            secret: Secret::new(READ_SECRET.into()),
            ..Default::default()
        }),
    })
    .unwrap();
    (
        Options {
            credentials: identities.primary,
            read_credentials: identities.secondary,
            ..Options::default()
        },
        std::sync::Arc::new(signer),
    )
}

fn probe(
    venue: BybitLinear,
) -> ProbeVenue<BybitLinear, impl FnMut(&mut BybitLinear, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        use safe_cex_engine::request_context::{Credential, Policy, Scope};
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            let rest = Rest {
                path: "/secondary-test",
                payload: if method == Method::Get {
                    "category=linear&symbol=BTC%26USDT".into()
                } else {
                    r#"{"category":"linear","clientId":"quote\"+&"}"#.into()
                },
                stream: false,
                signed: true,
                resent: false,
            };
            if method == Method::Get {
                venue.send_read(cx, rest, Then::Tickers(vec![]));
                continue;
            }
            // Bybit writes sign on a command plan; drive that recipe on a Background leg.
            let identity = venue.signing_identity(method).cloned().unwrap();
            let policy = Policy {
                credential: Some(Credential {
                    slot: safe_cex_engine::CredentialSlot::Primary,
                    reference: identity.credential.clone(),
                }),
                ..venue.read_policy(false, 20_000).unwrap()
            };
            let context = cx.http_context(Scope::Background, policy).unwrap();
            let step = sign::Http::new(identity, rest.payload.clone(), venue.recv_window)
                .with_header_policy(venue.signing_policy.header_policy());
            let url = format!("{}{}", venue.paths.rest, rest.path);
            let body = Some(rest.payload.clone());
            let pending = Pending {
                category: venue.category,
                rest,
                then: Then::Tickers(vec![]),
                read: Some(context.clone()),
            };
            assert!(
                cx.request_signed_managed(
                    context,
                    None,
                    method,
                    url,
                    body,
                    Box::new(step),
                    pending
                )
                .is_ok()
            );
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let (options, signer) = setup();
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(BybitLinear::new(options)));
    engine.set_signer(Some(signer)).unwrap();
    engine.start(1700000000000.0);
    let requests: Vec<_> = prepared_actions(&mut engine, 1700000001234.75)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.contains("secondary-test") => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 4);
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let secret = if request.method == Method::Get {
            READ_SECRET
        } else {
            PRIMARY_SECRET
        };
        let expected_headers = sign::oracle::Credentials {
            key: expected.into(),
            secret: secret.into(),
        }
        .headers(
            1700000001234.0,
            if request.method == Method::Get {
                request.url.split_once('?').unwrap().1
            } else {
                request.body.as_deref().unwrap()
            },
        );
        let headers = &request.headers;
        assert_eq!(*headers, expected_headers);
        assert!(
            headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-BAPI-API-KEY")
                    && value == expected),
            "{headers:?}"
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let (mut opts, _) = setup();
    opts.read_credentials
        .as_mut()
        .unwrap()
        .set_signing_ref(Some(safe_cex_engine::signing::KeyRef(String::new())));
    let mut engine = safe_cex_engine::test_support::seeded_engine(BybitLinear::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

/// One venue per key configuration: (primary, read key).
fn keyed(primary: bool, read: bool) -> BybitLinear {
    let (mut options, _) = setup();
    if !primary {
        options.credentials = None;
    }
    if !read {
        options.read_credentials = None;
    }
    BybitLinear::new(options)
}

fn chosen(
    venue: &BybitLinear,
    key: KeyUse,
) -> Option<safe_cex_engine::request_context::Credential> {
    venue
        .select_key(key)
        .map(|(selection, identity)| selection.credential(identity.credential.clone()))
}

fn slot_of(
    credential: Option<safe_cex_engine::request_context::Credential>,
) -> Option<CredentialSlot> {
    credential.map(|credential| credential.slot)
}

#[test]
fn each_key_use_takes_the_engine_slot_rule() {
    use CredentialSlot::{Primary, Secondary};
    // (primary, read key) -> (Read, AccountRead, Write)
    let table = [
        (
            (true, true),
            (Some(Secondary), Some(Secondary), Some(Primary)),
        ),
        ((true, false), (Some(Primary), Some(Primary), Some(Primary))),
        ((false, true), (Some(Secondary), None, None)),
        ((false, false), (None, None, None)),
    ];
    for ((primary, read), (reads, account_reads, writes)) in table {
        let venue = keyed(primary, read);
        let case = (primary, read);
        assert_eq!(slot_of(chosen(&venue, KeyUse::Read)), reads, "{case:?}");
        assert_eq!(
            slot_of(chosen(&venue, KeyUse::AccountRead)),
            account_reads,
            "{case:?}"
        );
        assert_eq!(slot_of(chosen(&venue, KeyUse::Write)), writes, "{case:?}");
    }
}

#[test]
fn every_signed_call_site_pins_its_key_use() {
    for (primary, read) in [(true, true), (true, false), (false, true), (false, false)] {
        let venue = keyed(primary, read);
        let case = (primary, read);
        let reads = chosen(&venue, KeyUse::Read);
        let account_reads = chosen(&venue, KeyUse::AccountRead);
        let writes = chosen(&venue, KeyUse::Write);
        // Signed reads, the environment probe and the GET signer: any key, read key first.
        assert_eq!(
            venue
                .read_policy(true, 20_000)
                .and_then(|policy| policy.credential),
            reads,
            "{case:?}"
        );
        let signer = |method| {
            venue
                .signing_identity(method)
                .map(|identity| identity.credential.clone())
        };
        assert_eq!(
            signer(Method::Get),
            reads.clone().map(|c| c.reference),
            "{case:?}"
        );
        // Writes: the primary only.
        for method in [Method::Post, Method::Put, Method::Delete] {
            assert_eq!(
                signer(method),
                writes.clone().map(|c| c.reference),
                "{case:?}"
            );
        }
        // Option Greeks and the ordinary Plan's reads: the read key only beside a primary.
        let greeks = venue
            .greeks_credentials()
            .map(|(slot, identity)| (slot, identity.credential));
        assert_eq!(
            greeks,
            account_reads.clone().map(|c| (c.slot, c.reference)),
            "{case:?}"
        );
        let (_, plan_read, plan_write) = venue.ordinary_preflight();
        assert_eq!(plan_read, account_reads, "{case:?}");
        assert_eq!(plan_write, writes, "{case:?}");
    }
}

/// dazzy3 on #1971: a signed read that cannot be admitted (no key, or a
/// configuration error) fails into its caller's failure path, so the caller's
/// in-flight flag clears, instead of returning silently with it still set.
fn read_without_admission(options: Options) -> bool {
    let cleared = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = cleared.clone();
    let probe = ProbeVenue::new(BybitLinear::new(options), move |venue, cx| {
        venue.startup.tickers_in_flight = true;
        let rest = Rest {
            path: "/signed-without-admission",
            payload: String::new(),
            stream: false,
            signed: true,
            resent: false,
        };
        venue.send_read(cx, rest, Then::Tickers(vec![]));
        seen.set(!venue.startup.tickers_in_flight);
    });
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
    engine.start(1700000000000.0);
    assert!(
        !engine.drain_actions().any(|a| matches!(a, Action::Http(_))),
        "nothing is sent"
    );
    cleared.get()
}

#[test]
fn a_signed_read_without_a_key_fails_into_its_caller() {
    assert!(read_without_admission(Options::default()));
}

#[test]
fn a_read_under_a_configuration_error_fails_into_its_caller() {
    // HyroTrader without its endpoints is a configuration error.
    let (mut options, _) = setup();
    options.variant = Variant::Hyrotrader;
    options.endpoints = None;
    assert!(BybitLinear::new(options.clone()).config_error.is_some());
    assert!(read_without_admission(options));
}
