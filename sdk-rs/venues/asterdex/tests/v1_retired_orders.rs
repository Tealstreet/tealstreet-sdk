//! Aster removed the V1 order routes (`/fapi/v1/openOrders`, `order`, ...), which now
//! answer an HTML 404. A V1 account can then never load its orders, so startup ends
//! at once with the cause instead of waiting out the host's start deadline.
mod support;
use safe_cex_engine::{Action, Engine, ErrorKind, Event, HttpRequest, Notice, Response};
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const NOW: f64 = 1_700_000_000_000.;
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000"}]}]}"#;
const BALANCE: &str = r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5","assets":[]}"#;
const NOT_FOUND: &str =
    "<!DOCTYPE html>\n<html><head><title>404</title></head><body>Not Found</body></html>";

fn path(request: &HttpRequest) -> &str {
    request.url.split('?').next().unwrap()
}

fn reply(
    engine: &mut Engine<AsterDex>,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    support::drain(engine, NOW)
}

/// Runs startup, answering every read but open orders, and returns that pending read.
fn starting(options: Options) -> (Engine<AsterDex>, HttpRequest) {
    let mut engine = support::engine(options);
    engine.start(NOW);
    let mut pending = support::drain(&mut engine, NOW);
    let mut orders = None;
    for _ in 0..16 {
        let mut next = Vec::new();
        for action in pending {
            if let Action::Http(request) = action {
                let at = path(&request);
                if at.ends_with("/openOrders") {
                    orders = Some(request);
                    continue;
                }
                let body = if at.ends_with("/time") {
                    r#"{"serverTime":1700000000000}"#
                } else if at.ends_with("/exchangeInfo") {
                    MARKETS
                } else if at.ends_with("/account") {
                    BALANCE
                } else if at.ends_with("/positionSide/dual") {
                    r#"{"dualSidePosition":false}"#
                } else if at.ends_with("/listenKey") {
                    r#"{"listenKey":"synthetic"}"#
                } else {
                    "[]"
                };
                next.extend(reply(&mut engine, &request, 200, body));
            }
        }
        pending = next;
    }
    let orders = orders.expect("native startup must read open orders");
    assert!(engine.store().loaded().balance && engine.store().loaded().positions);
    assert!(!engine.store().loaded().orders);
    (engine, orders)
}

fn failures(actions: &[Action]) -> Vec<&safe_cex_engine::VenueError> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::StartupFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}

#[test]
fn retired_v1_order_route_ends_startup_with_its_cause() {
    let (mut engine, orders) = starting(Options {
        key: Some("synthetic-key".into()),
        secret: Some("synthetic-material".into()),
        ..Default::default()
    });
    assert!(orders.url.contains("/fapi/v1/openOrders"));
    let done = reply(&mut engine, &orders, 404, NOT_FOUND);
    let failures = failures(&done);
    assert_eq!(
        failures.len(),
        1,
        "a removed route can never load: end startup now"
    );
    assert_eq!(failures[0].kind, Some(ErrorKind::AuthInvalid));
    assert!(
        failures[0].message.contains("V1") && failures[0].message.contains("V3"),
        "the cause names the retired key type and its replacement: {}",
        failures[0].message
    );
}

#[test]
fn v3_order_read_404_keeps_its_existing_retry() {
    let (mut engine, orders) = starting(Options::v3_wallet());
    assert!(orders.url.contains("/fapi/v3/openOrders"));
    let done = reply(&mut engine, &orders, 404, NOT_FOUND);
    assert!(
        failures(&done).is_empty(),
        "V3 still serves its order routes"
    );
}
