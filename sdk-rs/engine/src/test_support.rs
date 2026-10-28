//! Test-only helpers shared by venue crates (`features = ["test-support"]`).
use crate::book::Book;
use crate::{Action, Command, Cx, Engine, HttpRequest, Response, SocketId, TimerId, Venue};

/// Wraps a venue so a test can drive one of its private request helpers from
/// `start` and observe the emitted requests; every event handler is inert.
pub struct ProbeVenue<V, F> {
    pub venue: V,
    start: F,
}

impl<V: Venue, F: FnMut(&mut V, &mut Cx<'_, V::Pending>)> ProbeVenue<V, F> {
    pub fn new(venue: V, start: F) -> Self {
        Self { venue, start }
    }
}

impl<V: Venue, F: FnMut(&mut V, &mut Cx<'_, V::Pending>)> Venue for ProbeVenue<V, F> {
    type Pending = V::Pending;
    fn managed_sockets() -> bool {
        V::managed_sockets()
    }
    fn http_mutation_barriers() -> bool {
        V::http_mutation_barriers()
    }
    fn start(&mut self, cx: &mut Cx<'_, V::Pending>) {
        (self.start)(&mut self.venue, cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, V::Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, V::Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, V::Pending>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, V::Pending>, _: V::Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, V::Pending>, _: V::Pending, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, V::Pending>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, V::Pending>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// Drain the engine's queued actions, keeping only HTTP requests.
pub fn http_requests<V: Venue>(engine: &mut Engine<V>) -> Vec<HttpRequest> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}

/// `prepared_actions` at the latest handled event's time: managed requests
/// leave only through preparation, so fixtures that drained after `handle`
/// prepare at that same instant.
pub fn prepared_now<V: Venue>(engine: &mut Engine<V>) -> Vec<Action> {
    let now = engine.dispatch_now();
    prepared_actions(engine, now)
}

/// The latest event time, for native drivers that prepare after handle.
pub fn dispatch_now<V: Venue>(engine: &Engine<V>) -> f64 {
    engine.dispatch_now()
}

/// Immediate fake routes for native venue fixtures. Physical HTTP is returned
/// only after prepare, bind and the engine's final synchronous release claim.
/// Delayed pacing/routes and delegated signing deliberately remain unsupported.
pub fn prepared_actions<V: Venue>(engine: &mut Engine<V>, now: f64) -> Vec<Action> {
    use crate::request_context::{Claim, GroupClaim, GroupPacket, HttpAction, Prepared};
    use std::collections::{HashMap, VecDeque};

    engine.prepare_dispatch(now);
    let mut queue = VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut resources = HashMap::new();
    let mut groups: HashMap<crate::RequestId, GroupPacket> = HashMap::new();
    let mut actions = Vec::new();
    let mut count = 0;
    while let Some(action) = queue.pop_front() {
        count += 1;
        assert!(
            count <= 65_536,
            "prepared fixture action loop exceeded its bound"
        );
        let Action::ManagedHttp(action) = action else {
            actions.push(action);
            continue;
        };
        match *action {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => {
                if !engine.http_preparation_pending(preparation) {
                    continue;
                }
                assert_eq!(request.id, preparation.attempt().request);
                let id = request.id.0;
                assert!(
                    resources.insert(request.id, (preparation, id)).is_none(),
                    "fixture route replaced without cancellation"
                );
                engine.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id,
                        expires_at: None,
                    },
                );
            }
            HttpAction::Bind {
                preparation,
                permit,
                request,
            } => {
                assert_eq!(
                    resources.remove(&request.id),
                    Some((preparation, permit.prepared_id())),
                    "fixture bind needs its exact prepared route"
                );
                match engine.claim_http(now, permit) {
                    Ok(Claim::Released(_)) => actions.push(Action::Http(request)),
                    Ok(Claim::WaitUntil(_)) => {
                        panic!("delayed prepared fixture pacing is unsupported")
                    }
                    Err(_) => {} // Engine publishes the typed preparation failure/reprepare.
                }
            }
            HttpAction::BindGroup { permit, members } => {
                if !engine.http_group_pending(&permit) {
                    continue;
                }
                for mut member in members {
                    let request = member.preparation.attempt().request;
                    let current = permit
                        .members()
                        .iter()
                        .find(|p| p.attempt().request == request)
                        .unwrap();
                    assert_eq!(
                        resources.get(&request),
                        Some(&(member.preparation, current.prepared_id()))
                    );
                    if let Some(packet) = member.request.take() {
                        assert_eq!(packet.id, request);
                        assert!(
                            groups
                                .insert(
                                    request,
                                    GroupPacket {
                                        preparation: member.preparation,
                                        request: Some(packet)
                                    }
                                )
                                .is_none()
                        );
                    } else {
                        assert!(
                            groups
                                .get(&request)
                                .is_some_and(|packet| packet.preparation == member.preparation
                                    && packet.request.is_some())
                        );
                    }
                }
                match engine.claim_http_group(now, permit.clone()) {
                    Ok(GroupClaim::Released(_)) => {
                        for token in permit.members() {
                            let request = token.attempt().request;
                            let mut member = groups.remove(&request).unwrap();
                            resources.remove(&request);
                            actions.push(Action::Http(member.request.take().unwrap()));
                        }
                    }
                    Ok(GroupClaim::WaitUntil(_)) => {
                        panic!("delayed group fixture pacing is unsupported")
                    }
                    Err(_) => {}
                }
            }
            HttpAction::Cancel { preparation, .. } => {
                let request = preparation.attempt().request;
                if resources
                    .get(&request)
                    .is_some_and(|(token, _)| *token == preparation)
                {
                    resources.remove(&request);
                    groups.remove(&request);
                }
            }
        }
        // Preserve progress/receipt order at each handoff, rather than moving
        // an earlier packet behind all later sibling actions.
        for action in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    // A Session-parked call keeps its prepared route until the venue's announce
    // re-prepares it, which a later call drives as a fresh preparation.
    #[cfg(feature = "session-bearer")]
    resources.retain(|request, _| !engine.http_bearer_parked(*request));
    assert!(
        resources.is_empty(),
        "parked external signing is unsupported by the immediate fixture"
    );
    actions
}

/// Explicit fixture injection for BOTH identity and signing. Never a production default.
pub fn seeded_entropy(seed: u64) -> crate::entropy::Entropy {
    fn source(mut state: u64) -> crate::entropy::Fill {
        Box::new(move |bytes| {
            for byte in bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            Ok(())
        })
    }
    crate::entropy::Entropy::new(
        source(seed.max(1)),
        source((seed ^ 0xa076_1d64_78bd_642f).max(1)),
    )
}

/// Tests explicitly choose reproducible signing entropy as well as identities.
pub fn seeded_engine<V: Venue>(venue: V) -> Engine<V> {
    Engine::new(venue, Some(seeded_entropy(0x7365_6564_6564_4944))).unwrap()
}

/// Permanently exhaust the order-read admission domain for lifecycle controls.
pub fn exhaust_order_snapshots(store: &mut crate::Store) {
    store.exhaust_order_snapshots_for_test();
}

/// The recorded venue error payloads that pin a family's classification
/// (`tests/fixtures/errors/<case>.json`; RUST_ERROR_CLASSIFICATION.md, "Recorded-payload
/// corpus"). Loading checks provenance and redaction; the venue test drives each body.
pub mod error_corpus {
    use crate::{ErrorKind, Outcome};
    use serde::Deserialize;
    use std::path::Path;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Fixture {
        pub source: Source,
        pub captured_at: String,
        pub endpoint: String,
        #[serde(default)]
        pub channel: Option<String>,
        #[serde(default)]
        pub http_status: Option<u16>,
        /// The reply text exactly as received, after redaction.
        pub body: String,
        #[serde(default)]
        pub doc_url: Option<String>,
        pub expect: Expect,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Source {
        Live,
        Docs,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Expect {
        pub kind: Option<ErrorKind>,
        pub outcome: Option<Outcome>,
        pub path: Pinned,
    }

    /// The production path the fixture is driven through.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Pinned {
        Write,
        Read,
        Startup,
        WsLogin,
        WsOrder,
    }

    pub const REDACTED: &str = "<redacted>";

    /// The shared redaction rules (`error_corpus_redaction.json`), which
    /// `sdk-rs/harness/error-capture.mjs` also writes with.
    #[derive(Deserialize)]
    struct Rules {
        names: Vec<String>,
        suffixes: Vec<String>,
    }

    fn rules() -> &'static Rules {
        static RULES: std::sync::OnceLock<Rules> = std::sync::OnceLock::new();
        RULES.get_or_init(|| {
            serde_json::from_str(include_str!("error_corpus_redaction.json"))
                .expect("redaction rules")
        })
    }

    /// A key that names an account, a credential or an order: lowercased with
    /// `_` and `-` removed, it is a listed name or ends with a listed suffix.
    pub fn is_sensitive(key: &str) -> bool {
        let key: String = key
            .chars()
            .filter(|c| !matches!(c, '_' | '-'))
            .flat_map(char::to_lowercase)
            .collect();
        let rules = rules();
        rules.names.contains(&key)
            || rules
                .suffixes
                .iter()
                .any(|suffix| key.ends_with(suffix.as_str()))
    }

    /// Every `*.json` fixture in `dir`, sorted by file name. A missing or empty
    /// corpus panics, so a wrong or moved path can never pass a test vacuously.
    pub fn load(dir: &Path) -> Vec<(String, Fixture)> {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("error corpus {}: {error}", dir.display()));
        let mut names: Vec<_> = entries
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        names.sort();
        assert!(
            !names.is_empty(),
            "error corpus {} has no fixtures",
            dir.display()
        );
        names
            .into_iter()
            .map(|path| {
                let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&path).unwrap();
                let fixture = parse(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
                (name, fixture)
            })
            .collect()
    }

    /// One fixture, refused unless its provenance and redaction hold.
    pub fn parse(text: &str) -> Result<Fixture, String> {
        let fixture: Fixture = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if !fixture.captured_at.get(..10).is_some_and(is_date) {
            return Err("captured_at must start with a YYYY-MM-DD date".into());
        }
        match fixture.source {
            Source::Live if fixture.doc_url.is_some() => {
                return Err("a live capture carries no doc_url".into());
            }
            Source::Docs => {
                if !fixture
                    .doc_url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("https://"))
                {
                    return Err("a docs fixture needs an https doc_url".into());
                }
                let unavailable = fixture.http_status.is_some_and(|status| status >= 500)
                    || matches!(
                        fixture.expect.kind,
                        Some(
                            ErrorKind::RateLimited
                                | ErrorKind::VenueUnavailable
                                | ErrorKind::Timeout
                        )
                    );
                if !unavailable {
                    return Err("docs fixtures are only for rate limit, 5xx and maintenance".into());
                }
            }
            Source::Live => {}
        }
        if let Some(found) = unredacted(&fixture.endpoint) {
            return Err(format!("endpoint carries {found}"));
        }
        if let Some(found) = unredacted(&fixture.body) {
            return Err(format!("body carries {found}"));
        }
        Ok(fixture)
    }

    fn is_date(text: &str) -> bool {
        let bytes = text.as_bytes();
        bytes.len() == 10
            && bytes.iter().enumerate().all(|(i, b)| match i {
                4 | 7 => *b == b'-',
                _ => b.is_ascii_digit(),
            })
    }

    /// A sensitive field (JSON key at any depth, including JSON inside a string,
    /// or a query parameter) with a value other than blank or [`REDACTED`], or
    /// identifying free text: 40+ hex digits, IPv4, IPv6, a UUID or a mixed-case
    /// base58 run of 32+.
    fn unredacted(text: &str) -> Option<String> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
            && let Some(key) = sensitive_value(&value)
        {
            return Some(format!("an unredacted {key}"));
        }
        for pair in text.split(['?', '&']).skip(1) {
            if let Some((key, value)) = pair.split_once('=')
                && is_sensitive(key)
                && !value.is_empty()
                && value != REDACTED
            {
                return Some(format!("an unredacted {key}"));
            }
        }
        identifying_text(text)
    }

    fn sensitive_value(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Object(map) => map.iter().find_map(|(key, value)| {
                let blank = match value {
                    serde_json::Value::Null => true,
                    serde_json::Value::String(text) => text.is_empty() || text == REDACTED,
                    _ => false,
                };
                if is_sensitive(key) && !blank {
                    Some(key.clone())
                } else {
                    sensitive_value(value)
                }
            }),
            serde_json::Value::Array(items) => items.iter().find_map(sensitive_value),
            serde_json::Value::String(text) if text.starts_with(['{', '[']) => {
                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .and_then(|nested| sensitive_value(&nested))
            }
            _ => None,
        }
    }

    /// Maximal runs of characters satisfying `keep`.
    fn runs(text: &str, keep: impl Fn(char) -> bool) -> impl Iterator<Item = &str> {
        text.split(move |c: char| !keep(c))
            .filter(|run| !run.is_empty())
    }

    fn identifying_text(text: &str) -> Option<String> {
        if runs(text, |c| c.is_ascii_hexdigit()).any(|run| run.len() >= 40) {
            return Some("a 40+ digit hex run".into());
        }
        let ipv4 = |run: &str| {
            let parts: Vec<_> = run.split('.').collect();
            parts.len() == 4
                && parts.iter().all(|part| {
                    (1..=3).contains(&part.len())
                        && part.bytes().all(|b| b.is_ascii_digit())
                        && part.parse::<u16>().is_ok_and(|n| n <= 255)
                })
        };
        if runs(text, |c| c.is_ascii_digit() || c == '.').any(|run| ipv4(run.trim_matches('.'))) {
            return Some("an IPv4 address".into());
        }
        let ipv6 = |run: &str| {
            let groups: Vec<_> = run.split(':').collect();
            let filled = groups.iter().filter(|group| !group.is_empty()).count();
            groups.iter().all(|group| group.len() <= 4)
                && (groups.len() == 8 && filled == 8 || run.contains("::") && filled >= 2)
        };
        if runs(text, |c| c.is_ascii_hexdigit() || c == ':').any(ipv6) {
            return Some("an IPv6 address".into());
        }
        let uuid = |run: &str| {
            let lengths: Vec<_> = run.split('-').map(str::len).collect();
            lengths == [8, 4, 4, 4, 12]
        };
        if runs(text, |c| c.is_ascii_hexdigit() || c == '-').any(uuid) {
            return Some("a UUID".into());
        }
        let base58 = |c: char| c.is_ascii_alphanumeric() && !matches!(c, '0' | 'O' | 'I' | 'l');
        let mixed = |run: &str| {
            run.len() >= 32
                && run.bytes().any(|b| b.is_ascii_digit())
                && run.bytes().any(|b| b.is_ascii_uppercase())
                && run.bytes().any(|b| b.is_ascii_lowercase())
        };
        if runs(text, |c| c.is_ascii_alphanumeric())
            .filter(|run| run.chars().all(base58))
            .any(mixed)
        {
            return Some("a base58 run".into());
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn fixture(fields: &str) -> String {
            format!(
                r#"{{"captured_at":"2026-10-28T10:00:00Z","endpoint":"/api/v1/order",
                "expect":{{"kind":"insufficientBalance","outcome":"rejected","path":"write"}},{fields}}}"#
            )
        }

        #[test]
        fn a_redacted_live_capture_loads() {
            let fixture = parse(&fixture(
                r#""source":"live","http_status":400,"body":"{\"code\":-2019,\"msg\":\"Margin is insufficient.\",\"orderId\":\"<redacted>\"}""#,
            ))
            .unwrap();
            assert_eq!(fixture.source, Source::Live);
            assert_eq!(fixture.expect.kind, Some(ErrorKind::InsufficientBalance));
            assert_eq!(fixture.expect.outcome, Some(Outcome::Rejected));
            assert_eq!(fixture.expect.path, Pinned::Write);
        }

        fn corpus(name: &str) -> std::path::PathBuf {
            let dir =
                std::env::temp_dir().join(format!("error-corpus-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        #[test]
        fn load_reads_sorted_fixtures_and_ignores_other_files() {
            let dir = corpus("sorted");
            let live = fixture(r#""source":"live","body":"x""#);
            std::fs::write(dir.join("b-second.json"), &live).unwrap();
            std::fs::write(dir.join("a-first.json"), &live).unwrap();
            std::fs::write(dir.join("README.md"), "notes").unwrap();
            let names: Vec<_> = load(&dir).into_iter().map(|(name, _)| name).collect();
            assert_eq!(names, ["a-first", "b-second"]);
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        #[should_panic(expected = "os error")]
        fn a_missing_corpus_directory_panics() {
            load(Path::new("/nonexistent/tests/fixtures/errors"));
        }

        #[test]
        #[should_panic(expected = "has no fixtures")]
        fn an_empty_corpus_panics() {
            let dir = corpus("empty");
            std::fs::write(dir.join("README.md"), "notes").unwrap();
            load(&dir);
        }

        #[test]
        fn provenance_is_enforced() {
            let docs = |status: u16, url: &str| {
                parse(&fixture(&format!(
                    r#""source":"docs","http_status":{status},"body":"busy"{url}"#
                )))
            };
            assert!(docs(503, r#","doc_url":"https://venue.example/errors""#).is_ok());
            assert!(docs(503, "").unwrap_err().contains("doc_url"));
            assert!(
                docs(400, r#","doc_url":"https://venue.example/errors""#)
                    .unwrap_err()
                    .contains("only for rate limit")
            );
            let live_with_url =
                fixture(r#""source":"live","body":"x","doc_url":"https://venue.example/errors""#);
            assert!(parse(&live_with_url).unwrap_err().contains("no doc_url"));
            let undated =
                fixture(r#""source":"live","body":"x""#).replace("2026-10-28", "yesterday!");
            assert!(parse(&undated).unwrap_err().contains("captured_at"));
            assert!(parse(&fixture(r#""source":"live","body":"x","note":1"#)).is_err());
        }

        /// The shared vectors the JS writer is also tested against: every raw
        /// text is refused, its redacted form and every keep text pass.
        #[test]
        fn the_loader_agrees_with_the_writer_on_the_shared_vectors() {
            let rules: serde_json::Value =
                serde_json::from_str(include_str!("error_corpus_redaction.json")).unwrap();
            let vectors = &rules["vectors"];
            for vector in vectors["redact"].as_array().unwrap() {
                let raw = vector["raw"].as_str().unwrap();
                assert!(unredacted(raw).is_some(), "not refused: {raw}");
                let redacted = vector["redacted"].as_str().unwrap();
                assert_eq!(unredacted(redacted), None, "{redacted}");
            }
            for keep in vectors["keep"].as_array().unwrap() {
                let keep = keep.as_str().unwrap();
                assert_eq!(unredacted(keep), None, "{keep}");
            }
            assert!(identifying_text("Your IP 203.0.113.7.").is_some());
            assert!(is_sensitive("sub_account_id") && is_sensitive("orderLinkId"));
            assert!(!is_sensitive("sCode") && !is_sensitive("code") && !is_sensitive("msg"));
        }

        #[test]
        fn unredacted_material_is_refused() {
            let body = |body: &str| parse(&fixture(&format!(r#""source":"live","body":{body}"#)));
            assert!(
                body(r#""{\"data\":[{\"ordId\":\"6129383\"}]}""#)
                    .unwrap_err()
                    .contains("ordId")
            );
            assert!(
                body(r#""{\"apiKey\":\"abc\"}""#)
                    .unwrap_err()
                    .contains("apiKey")
            );
            assert!(body(r#""{\"apiKey\":\"<redacted>\",\"uid\":null}""#).is_ok());
            let hex = format!(r#""sig {}""#, "a".repeat(40));
            assert!(body(&hex).unwrap_err().contains("hex run"));
            let query = fixture(r#""source":"live","body":"x""#).replace(
                "/api/v1/order",
                "/api/v1/order?symbol=BTC&signature=deadbeef",
            );
            assert!(parse(&query).unwrap_err().contains("signature"));
        }
    }
}
