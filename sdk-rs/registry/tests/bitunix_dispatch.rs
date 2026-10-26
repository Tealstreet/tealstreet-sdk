#![cfg(feature = "bitunix")]
//! Originals for docs/plans/RUST_BITUNIX_DISPATCH_SIGNER.md: Bitunix signs
//! through the engine provider from trusted-setup identities, by role, and
//! every signature matches an independent double-SHA oracle.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, Venue, VenueError,
};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer, signer_setup_policy};

const PAIRS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}]}"#;
const TICKERS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","last":"96000","open":"95000","markPrice":"96001","baseVol":"1000","quoteVol":"96000000"}]}"#;
const ACCOUNT: &str = r#"{"code":0,"data":[{"marginCoin":"USDT","available":"900","margin":"100","frozen":"3","positionMode":"ONE_WAY","crossUnrealizedPNL":"0","isolationUnrealizedPNL":"0"}]}"#;
const EMPTY_LIST: &str = r#"{"code":0,"data":{"orderList":[],"positionList":[]}}"#;
const EMPTY_TPSL: &str = r#"{"code":0,"data":[]}"#;
const PRIMARY_KEY: &str = "primary-key";
const PRIMARY_MATERIAL: &str = "primary-material";
const READ_KEY: &str = "read-key";
const READ_MATERIAL: &str = "read-material";

/// Everything the venue released, after startup and one leverage write.
struct Drive;
impl Visit for Drive {
    type Output = Driven;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        let mut requests = Vec::new();
        let mut frames = Vec::new();
        let mut failures = Vec::new();
        let mut now = 1_700_000_000_000.0;
        engine.start(now);
        for round in 0..40 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
            if actions.is_empty() {
                if round == 20 {
                    engine.handle(
                        now,
                        Event::Command(Command::SetLeverage {
                            command: 7,
                            symbol: "BTCUSDT".into(),
                            leverage: 10.0,
                            side: None,
                        }),
                    );
                    continue;
                }
                now += 1.0;
                continue;
            }
            for action in actions {
                match action {
                    Action::WsConnect { socket, .. } => {
                        engine.handle(now, Event::WsOpened { socket });
                    }
                    Action::WsSend { text, .. } => frames.push(text),
                    Action::Notice(Notice::CommandFailed { command: 7, error }) => {
                        failures.push(error)
                    }
                    Action::Http(request) => {
                        let body = if request.url.ends_with("/trading_pairs") {
                            PAIRS
                        } else if request.url.ends_with("/tickers") {
                            TICKERS
                        } else if request.url.contains("/tpsl/") {
                            EMPTY_TPSL
                        } else if request.url.contains("get_pending") {
                            EMPTY_LIST
                        } else {
                            ACCOUNT
                        };
                        engine.handle(
                            now,
                            Event::HttpResponse {
                                request: request.id,
                                response: Response {
                                    status: 200,
                                    headers: "",
                                    body,
                                },
                            },
                        );
                        requests.push(request);
                    }
                    _ => {}
                }
            }
        }
        (requests, frames, failures)
    }
}

fn identity(key: &str, material: &str) -> LocalIdentity {
    LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(material.into()),
        ..Default::default()
    }
}
type Driven = (Vec<HttpRequest>, Vec<String>, Vec<VenueError>);
/// Setup applies Bitunix's declared policy, as the trusted host does.
fn setup(
    primary: Option<LocalIdentity>,
    secondary: Option<LocalIdentity>,
) -> (LocalSigner, safe_cex_engine::signing::Identities) {
    LocalSigner::from_setup_with_policy(
        LocalSetup { primary, secondary },
        signer_setup_policy("bitunix"),
    )
    .unwrap()
}
fn drive(primary: Option<LocalIdentity>, secondary: Option<LocalIdentity>, public: bool) -> Driven {
    let (provider, identities) = setup(primary, secondary);
    try_drive(provider, identities, public).unwrap()
}
fn try_drive(
    provider: LocalSigner,
    identities: safe_cex_engine::signing::Identities,
    public: bool,
) -> Result<Driven, VenueError> {
    let mut options: EngineOptions =
        serde_json::from_str(&format!(r#"{{"publicDataOnly":{public}}}"#)).unwrap();
    options.signing = Some(identities);
    build_with_signer(
        "bitunix",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Some(std::sync::Arc::new(provider)),
        Drive,
    )
}
fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("{name} missing on {}", request.url))
}
/// Native Bitunix: sorted query pairs joined without `=` or `&`.
fn signing_query(url: &str) -> String {
    let Some((_, query)) = url.split_once('?') else {
        return String::new();
    };
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key.to_owned(), percent_decode(value))
        })
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}{value}"))
        .collect()
}
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}
/// Independent oracle: hex(sha256(hex(sha256(nonce‖ts‖key‖query‖body))‖material)).
fn oracle(
    nonce: &str,
    timestamp: &str,
    key: &str,
    query: &str,
    body: &str,
    material: &str,
) -> String {
    let inner = safe_cex_crypto::hex(&safe_cex_crypto::sha256(
        format!("{nonce}{timestamp}{key}{query}{body}").as_bytes(),
    ));
    safe_cex_crypto::hex(&safe_cex_crypto::sha256(
        format!("{inner}{material}").as_bytes(),
    ))
}
fn assert_signed(request: &HttpRequest, key: &str, material: &str) {
    assert_eq!(header(request, "api-key"), key, "{}", request.url);
    let body = request.body.as_deref().unwrap_or("");
    let expected = oracle(
        header(request, "nonce"),
        header(request, "timestamp"),
        key,
        &signing_query(&request.url),
        body,
        material,
    );
    assert_eq!(header(request, "sign"), expected, "{}", request.url);
}
fn private_read(requests: &[HttpRequest]) -> &HttpRequest {
    requests
        .iter()
        .find(|r| r.method == Method::Get && r.url.contains("/account"))
        .expect("a signed account read")
}
fn write(requests: &[HttpRequest]) -> &HttpRequest {
    requests
        .iter()
        .find(|r| r.method == Method::Post)
        .expect("a signed leverage write")
}

#[test]
fn reads_sign_with_the_read_key_and_writes_with_the_primary() {
    let (requests, _, _) = drive(
        Some(identity(PRIMARY_KEY, PRIMARY_MATERIAL)),
        Some(identity(READ_KEY, READ_MATERIAL)),
        false,
    );
    assert_signed(private_read(&requests), READ_KEY, READ_MATERIAL);
    assert_signed(write(&requests), PRIMARY_KEY, PRIMARY_MATERIAL);
}

#[test]
fn the_private_login_signs_with_the_primary_against_the_oracle() {
    let (_, frames, _) = drive(
        Some(identity(PRIMARY_KEY, PRIMARY_MATERIAL)),
        Some(identity(READ_KEY, READ_MATERIAL)),
        false,
    );
    let login = frames
        .iter()
        .find(|frame| frame.contains(r#""op":"login""#))
        .expect("one login frame");
    let frame: serde_json::Value = serde_json::from_str(login).unwrap();
    let args = &frame["args"][0];
    assert_eq!(args["apiKey"], PRIMARY_KEY);
    let expected = oracle(
        args["nonce"].as_str().unwrap(),
        args["timestamp"].as_str().unwrap(),
        PRIMARY_KEY,
        "",
        "",
        PRIMARY_MATERIAL,
    );
    assert_eq!(args["sign"], expected);
}

#[test]
fn padded_setup_material_signs_trimmed_on_rest_and_login() {
    let (requests, frames, _) = drive(
        Some(identity(" primary-key\t", " primary-material \n")),
        None,
        false,
    );
    assert_signed(private_read(&requests), PRIMARY_KEY, PRIMARY_MATERIAL);
    let login = frames
        .iter()
        .find(|f| f.contains(r#""op":"login""#))
        .unwrap();
    let frame: serde_json::Value = serde_json::from_str(login).unwrap();
    assert_eq!(frame["args"][0]["apiKey"], PRIMARY_KEY);
}

#[test]
fn public_data_only_with_identities_signs_nothing() {
    let (requests, frames, _) = drive(Some(identity(PRIMARY_KEY, PRIMARY_MATERIAL)), None, true);
    assert!(!requests.is_empty(), "public startup still runs");
    assert!(
        requests.iter().all(|r| !r
            .headers
            .iter()
            .any(|(h, _)| h.eq_ignore_ascii_case("sign"))),
        "no signed request"
    );
    assert!(!frames.iter().any(|f| f.contains(r#""op":"login""#)));
}

/// Raw keys never travel beside setup identities to the venue.
#[test]
fn raw_engine_option_keys_beside_identities_are_refused() {
    let (provider, identities) = setup(Some(identity(PRIMARY_KEY, PRIMARY_MATERIAL)), None);
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities);
    options.key = Some("must-not-reach-venue".into());
    let error = build_with_signer(
        "bitunix",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Some(std::sync::Arc::new(provider)),
        Drive,
    )
    .map(|_| ())
    .expect_err("raw keys must not reach the venue");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert_eq!(
        error.message,
        "Bitunix credentials belong in signer setup, not engine options"
    );
}

/// Legacy raw input converges on the trusted provider path, never inline signing.
#[test]
fn legacy_raw_keys_sign_through_the_provider() {
    let options: EngineOptions = serde_json::from_str(&format!(
        r#"{{"key":"{PRIMARY_KEY}","secret":"{PRIMARY_MATERIAL}"}}"#
    ))
    .unwrap();
    let (requests, _, _) = build(
        "bitunix",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Drive,
    )
    .unwrap();
    assert_signed(private_read(&requests), PRIMARY_KEY, PRIMARY_MATERIAL);
}

/// Every engine signing provider is required once an identity is set.
#[test]
fn identities_without_a_provider_are_refused() {
    let (_, identities) = setup(Some(identity(PRIMARY_KEY, PRIMARY_MATERIAL)), None);
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities);
    let error = build_with_signer(
        "bitunix",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        None,
        Drive,
    )
    .map(|_| ())
    .expect_err("a provider is required");
    assert_eq!(error.message, "Engine signing provider is required");
}

/// dazzy3 P1: a provider refusal sends no write and fails it NotSent.
#[test]
fn a_provider_refusal_sends_no_write_and_fails_it_not_sent() {
    let (_, identities) = setup(Some(identity(PRIMARY_KEY, PRIMARY_MATERIAL)), None);
    // A provider holding none of these refs: every sign is MissingKey.
    let (empty, _) = LocalSigner::from_setup(LocalSetup::default()).unwrap();
    let (requests, _, failures) = try_drive(empty, identities, false).unwrap();
    assert!(
        !requests.iter().any(|r| r.method == Method::Post),
        "no write reaches the wire"
    );
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
}

/// dazzy3 P2: whitespace-only signing material is absent, as with no
/// credentials: nothing is ever signed with an empty key.
#[test]
fn whitespace_only_signing_material_signs_nothing() {
    let Ok((provider, identities)) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(identity(PRIMARY_KEY, " \t\n")),
            secondary: None,
        },
        signer_setup_policy("bitunix"),
    ) else {
        return;
    };
    let Ok((requests, frames, _)) = try_drive(provider, identities, false) else {
        return;
    };
    assert!(
        requests.iter().all(|r| !r
            .headers
            .iter()
            .any(|(h, _)| h.eq_ignore_ascii_case("sign"))),
        "never signed with an empty key"
    );
    assert!(!frames.iter().any(|f| f.contains(r#""op":"login""#)));
}
