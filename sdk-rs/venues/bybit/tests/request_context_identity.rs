//! Slice 3: account identity is a signed command Read on the engine's managed
//! authority. Its native parse and failure ownership stay pinned by identity.rs.
mod support;
use safe_cex_engine::{Command, ErrorKind, Notice, Outcome, ReadCredentials, VenueError};
use support::{Harness, Options};

fn keyed(read_key: bool) -> Harness {
    Harness::started(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read_key.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    })
}

fn failure(h: &Harness, command: u32) -> VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            Notice::CommandFailed { command: id, error } if *id == command => Some(error.clone()),
            _ => None,
        })
        .expect("one typed failure")
}

/// I1: identity is a managed signed read, with the read key when configured.
#[test]
fn identity_is_a_managed_signed_read_with_the_read_key() {
    for read_key in [false, true] {
        let mut h = keyed(read_key);
        h.command(Command::FetchAccount { command: 42 });
        let request = h.take("/v5/user/query-api");
        assert_eq!(request.timeout_ms, Some(20_000), "a managed read window");
        let expected = if read_key { "read-key" } else { "primary-key" };
        assert_eq!(
            Harness::header(&request, "X-BAPI-API-KEY").as_deref(),
            Some(expected)
        );
    }
}

/// I2: a refusal before any request is typed and sends nothing.
#[test]
fn identity_without_credentials_is_a_typed_refusal() {
    let mut h = Harness::started(Options::default());
    h.requests.clear();
    h.command(Command::FetchAccount { command: 43 });
    assert!(!h.has_request("/v5/user/query-api"));
    let error = failure(&h, 43);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
}

/// I3: a native timestamp refusal re-reads once with Bybit's corrected clock.
#[test]
fn identity_re_reads_once_on_a_native_timestamp_refusal() {
    let mut h = keyed(false);
    h.command(Command::FetchAccount { command: 44 });
    let first = h.take("/v5/user/query-api");
    h.reply(
        first.id,
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[1],server_timestamp[9000],recv_window[5000]","result":{}}"#,
    );
    let retry = h.take("/v5/user/query-api");
    assert_eq!(
        Harness::header(&retry, "X-BAPI-TIMESTAMP").as_deref(),
        Some("9000")
    );
}

/// The re-read completes the command with the identity it reads.
#[test]
fn a_re_read_identity_completes_the_command() {
    let mut h = keyed(false);
    h.command(Command::FetchAccount { command: 45 });
    let first = h.take("/v5/user/query-api");
    h.reply(
        first.id,
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[1],server_timestamp[9000],recv_window[5000]","result":{}}"#,
    );
    let retry = h.take("/v5/user/query-api");
    h.reply(
        retry.id,
        200,
        r#"{"retCode":0,"retMsg":"","result":{"id":"13770661","note":"","apiKey":"primary-key","readOnly":0,"secret":"","permissions":{},"ips":["*"],"type":1,"deadlineDay":83,"expiredAt":"2024-01-01T00:00:00Z","createdAt":"2023-01-01T00:00:00Z","unified":0,"uta":0,"userID":24617703,"inviterID":0,"vipLevel":"No VIP","mktMakerLevel":"0","affiliateID":0,"rsaPublicKey":"","isMaster":true,"parentUid":"0","kycLevel":"LEVEL_DEFAULT","kycRegion":""}}"#,
    );
    assert!(matches!(h.result(45), Some(Ok(_))), "{:?}", h.result(45));
    assert!(!h.has_request("/v5/user/query-api"));
}
