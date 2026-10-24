//! Slice 5: history and closed-position reads run on a caller Read root. Each
//! page is a managed leg; the caller's wait bounds the whole walk.
mod support;
use safe_cex_engine::{
    Command, ErrorKind, Event, Notice, Query, ReadCredentials, VenueAction, VenueError,
};
use support::{Harness, Options};

const DAY: f64 = 24.0 * 3_600_000.0;
const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const FILL: &str = r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"","category":"linear","list":[{"symbol":"BTCUSDT","orderType":"Market","side":"Buy","orderId":"o1","execTime":"1672282722429","execFee":"0.07","feeRate":"0.0006","execId":"e1","execPrice":"1190.15","execQty":"0.1","execType":"Trade","isMaker":false}]}}"#;
const TIMESTAMP_REFUSAL: &str = r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[1],server_timestamp[9000],recv_window[5000]","result":{}}"#;

fn ready(read_key: bool) -> Harness {
    let mut h = Harness::started(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read_key.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    });
    for (path, body) in [
        ("/v5/account/fee-rate", support::FEE_RATES),
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("status=PreLaunch", EMPTY),
    ] {
        if h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.requests.clear();
    h
}

fn fills(from_ms: Option<f64>, to_ms: Option<f64>) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms,
        to_ms,
        limit: Some(100),
    }
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

/// Each history page is a managed leg, signed with the read key when configured.
#[test]
fn history_pages_are_managed_legs_signed_with_the_read_key() {
    for read_key in [false, true] {
        let mut h = ready(read_key);
        h.command(Command::Query {
            command: 60,
            query: fills(None, None),
        });
        let page = h.take("/v5/execution/list");
        assert_eq!(page.timeout_ms, Some(20_000), "a managed page window");
        let expected = if read_key { "read-key" } else { "primary-key" };
        assert_eq!(
            Harness::header(&page, "X-BAPI-API-KEY").as_deref(),
            Some(expected)
        );
    }
}

/// A page that never answers fails typed at its own window, not never.
#[test]
fn an_unanswered_history_page_times_out_typed() {
    let mut h = ready(false);
    h.command(Command::Query {
        command: 61,
        query: fills(None, None),
    });
    let _page = h.take("/v5/execution/list");
    h.advance(20_001.0);
    assert_eq!(failure(&h, 61).kind, Some(ErrorKind::Timeout));
}

/// The caller's wait bounds the whole multi-window walk: at its deadline the
/// command times out strictly and no further page is issued.
#[test]
fn the_caller_wait_bounds_a_multi_window_history_walk() {
    let mut h = ready(false);
    let to = 1_672_300_000_000.0;
    h.command_waiting(
        Command::Query {
            command: 62,
            query: fills(Some(to - 13.0 * DAY), Some(to)),
        },
        5_000,
    );
    let first = h.take("/v5/execution/list");
    h.reply(first.id, 200, FILL);
    let _second = h.take("/v5/execution/list");
    h.advance(5_001.0);
    assert_eq!(failure(&h, 62).kind, Some(ErrorKind::Timeout));
    assert!(
        !h.has_request("/v5/execution/list"),
        "no page after the deadline"
    );
}

/// A native refusal keeps its classified kind.
#[test]
fn a_native_history_refusal_keeps_its_kind() {
    let mut h = ready(false);
    h.command(Command::Query {
        command: 63,
        query: fills(None, None),
    });
    let page = h.take("/v5/execution/list");
    h.reply(
        page.id,
        200,
        r#"{"retCode":10003,"retMsg":"API key is invalid.","result":{}}"#,
    );
    assert_eq!(failure(&h, 63).kind, Some(ErrorKind::AuthInvalid));
}

/// A native timestamp refusal re-reads once, as a fresh managed leg with
/// Bybit's clock; host text that resembles one never re-sends.
#[test]
fn history_re_reads_only_on_a_native_timestamp_refusal() {
    let mut h = ready(false);
    h.command(Command::Query {
        command: 64,
        query: fills(None, None),
    });
    let page = h.take("/v5/execution/list");
    h.reply(page.id, 200, TIMESTAMP_REFUSAL);
    let retry = h.take("/v5/execution/list");
    assert_eq!(retry.timeout_ms, Some(20_000));
    assert_eq!(
        Harness::header(&retry, "X-BAPI-TIMESTAMP").as_deref(),
        Some("9000")
    );

    let mut h = ready(false);
    h.command(Command::Query {
        command: 65,
        query: fills(None, None),
    });
    let page = h.take("/v5/execution/list");
    h.event(Event::HttpFailed {
        request: page.id,
        error: "network lost; error sign",
    });
    assert!(!h.has_request("/v5/execution/list"), "text never re-sends");
    assert_eq!(failure(&h, 65).kind, Some(ErrorKind::Network));
}

/// Closed positions page on the same managed authority and caller bound.
#[test]
fn closed_position_pages_are_managed_and_caller_bounded() {
    let mut h = ready(false);
    h.command_waiting(
        Command::Venue {
            command: 66,
            action: VenueAction::FetchClosedPositions {
                symbol: None,
                from_ms: None,
                to_ms: None,
            },
        },
        5_000,
    );
    let page = h.take("/v5/position/closed-pnl");
    // A managed page whose window never outlives the caller's 5 s wait.
    assert_eq!(page.timeout_ms, Some(5_000));
    h.advance(5_001.0);
    assert_eq!(failure(&h, 66).kind, Some(ErrorKind::Timeout));
    assert!(!h.has_request("/v5/position/closed-pnl"));
}

/// Keyless account history is empty without any request (control).
#[test]
fn keyless_account_history_is_empty_without_io() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.requests.clear();
    h.command(Command::Query {
        command: 67,
        query: fills(None, None),
    });
    assert_eq!(h.result(67), Some(&Ok("[]".to_owned())));
    assert!(!h.has_request("/v5/execution/list"));
}
