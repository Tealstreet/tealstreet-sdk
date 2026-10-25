//! Slice 1c: startup discovery (instrument lists and the public ticker seed)
//! runs on Background Read contexts for every alias, Hyro included. Native
//! retry, pause, PreLaunch and publication behaviour stays pinned by
//! startup_resilience.rs, public.rs, public_variants.rs and products.rs.
mod support;
use safe_cex_engine::Event;
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::{Harness, INSTRUMENTS, Options, PRELAUNCH, TICKERS};

const WINDOW: Option<u32> = Some(20_000);
const DISCOVERY: [&str; 2] = ["/v5/market/instruments-info", "/v5/market/tickers"];

fn public(variant: Variant, rest: &str) -> Options {
    Options {
        variant,
        public_data_only: true,
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: rest.into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    }
}

/// Every outstanding discovery request carries a managed read window.
fn assert_managed(h: &Harness) {
    let pending: Vec<_> = h
        .requests
        .iter()
        .filter(|request| DISCOVERY.iter().any(|path| request.url.contains(path)))
        .collect();
    assert!(!pending.is_empty(), "discovery is outstanding");
    for request in pending {
        assert_eq!(request.timeout_ms, WINDOW, "{} is managed", request.url);
        assert!(
            Harness::header(request, "X-Referer").is_some(),
            "{} keeps the broker header",
            request.url
        );
    }
}

/// C1: instrument lists (Trading and PreLaunch) and the ticker seed go out on
/// managed read windows with the broker header, for every alias.
#[test]
fn discovery_lists_go_out_on_managed_read_windows() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Spot,
        Variant::Demo,
        Variant::Hyrotrader,
    ] {
        let h = Harness::started(public(variant, "https://hyro.example"));
        assert_managed(&h);
    }
}

/// C2: a discovery read refused at admission feeds the category's retry
/// backoff; it neither stalls startup nor fails it.
#[test]
fn an_admission_refused_discovery_read_retries_through_the_same_backoff() {
    let mut h = Harness::started(Options {
        variant: Variant::Linear,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    let tickers = h.take("/v5/market/tickers");
    h.event(Event::HttpFailed {
        request: tickers.id,
        error: "scripted outage",
    });
    support::park_owners(&mut h);
    h.advance(1_000.);
    assert!(
        !h.has_request("/v5/market/tickers"),
        "no unmanaged read while admission is full"
    );
    assert!(h.startup_failures.is_empty());
    h.advance(60_000.);
    let retried = h.take("/v5/market/tickers");
    assert_eq!(retried.timeout_ms, WINDOW);
}

/// C3: a discovery window that expires retries exactly like transport loss.
#[test]
fn an_expired_discovery_window_retries_like_transport_loss() {
    let mut h = Harness::started(public(Variant::Linear, ""));
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    let read = h.take("/v5/market/instruments-info");
    assert_eq!(read.timeout_ms, WINDOW);
    h.advance(20_001.);
    assert!(!h.has_request("/v5/market/instruments-info"));
    h.advance(1_000.);
    assert!(h.has_request("/v5/market/instruments-info"));
    assert!(h.startup_failures.is_empty());
}

/// C4: a 403 on a managed ticker seed keeps its ten-minute pause.
#[test]
fn a_managed_ticker_ban_keeps_its_pause_evidence() {
    let mut h = Harness::started(public(Variant::Linear, ""));
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    let ticker = h.take("/v5/market/tickers");
    assert_eq!(ticker.timeout_ms, WINDOW);
    h.reply(ticker.id, 403, "Forbidden");
    h.advance(599_000.);
    assert!(!h.has_request("/v5/market/tickers"));
    h.advance(2_000.);
    assert_eq!(h.take("/v5/market/tickers").timeout_ms, WINDOW);
}

/// C5: an answer delivered after its window closed publishes no rows.
#[test]
fn a_late_discovery_answer_publishes_no_rows() {
    let mut h = Harness::started(public(Variant::Linear, ""));
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    let read = h.take("/v5/market/instruments-info");
    h.advance(20_001.);
    h.reply(read.id, 200, INSTRUMENTS);
    assert!(!h.engine.store().loaded().markets);
    assert!(h.engine.store().markets().is_empty());
}

/// C6: Hyro discovery goes to its configured REST host on managed windows.
#[test]
fn hyro_discovery_uses_its_configured_host_on_managed_windows() {
    let host = "https://hyro-sim.example";
    let h = Harness::started(public(Variant::Hyrotrader, host));
    assert_managed(&h);
    for request in &h.requests {
        assert!(
            request.url.starts_with(host),
            "{} uses the configured host",
            request.url
        );
    }
}

/// C7: Hyro's USDT-perpetual filter and Trading+PreLaunch symbol dedup survive
/// the managed path, page by page.
#[test]
fn hyro_filter_and_dedup_survive_managed_discovery() {
    let mut h = Harness::started(public(Variant::Hyrotrader, "https://hyro.example"));
    assert_managed(&h);
    h.respond(
        "/v5/market/instruments-info?category=linear&limit=1000",
        200,
        INSTRUMENTS,
    );
    h.respond("/v5/market/tickers", 200, TICKERS);
    let prelaunch = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"NEWUSDT","status":"PreLaunch","baseCoin":"NEW","quoteCoin":"USDT","contractType":"LinearPerpetual","lotSizeFilter":{"qtyStep":"1","minOrderQty":"1","maxOrderQty":"1000"},"priceFilter":{"tickSize":"0.01"}},{"symbol":"NEWUSDC","status":"PreLaunch","baseCoin":"NEW","quoteCoin":"USDC"}],"nextPageCursor":"second"}}"#;
    h.respond("status=PreLaunch", 200, prelaunch);
    let page = h.take("cursor=second");
    assert_eq!(page.timeout_ms, WINDOW, "the next page is managed too");
    let duplicate = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","status":"PreLaunch","baseCoin":"BTC","quoteCoin":"USDT","contractType":"LinearPerpetual","lotSizeFilter":{"qtyStep":"0.01","minOrderQty":"0.01","maxOrderQty":"10"},"priceFilter":{"tickSize":"0.1"}}],"nextPageCursor":""}}"#;
    h.reply(page.id, 200, duplicate);
    let symbols: Vec<_> = h
        .engine
        .store()
        .markets()
        .iter()
        .map(|m| m.symbol.clone())
        .collect();
    assert_eq!(symbols, ["BTCUSDT", "ETHUSDT", "NEWUSDT"]);
    assert_eq!(
        h.engine.store().market("BTCUSDT").unwrap().precision.amount,
        0.01
    );
}

/// C8: a reply that lands after the deadline but before its
/// expiry timer fires must still leave discovery retrying.
#[test]
fn a_reply_past_its_deadline_before_the_timer_still_retries() {
    for body in [INSTRUMENTS, "{\"retCode\":0,\"result\":{}}"] {
        let mut h = Harness::started(public(Variant::Linear, ""));
        h.respond("status=PreLaunch", 200, PRELAUNCH);
        let read = h.take("/v5/market/instruments-info");
        h.now += 20_001.;
        h.reply(read.id, 200, body);
        assert!(h.engine.store().markets().is_empty(), "no rows published");
        h.advance(120_000.);
        assert!(
            h.has_request("/v5/market/instruments-info"),
            "discovery stalled after a past-deadline reply ({body})"
        );
    }
}
