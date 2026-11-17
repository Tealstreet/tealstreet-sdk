mod support;

use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Method, Response, SocketChannel, SocketId,
};
use support::{MARKETS, Options, TICKERS, TestEngine, configured, prepared};

const UTC: f64 = 1_800_000_000_000.;
const LOOKBACK: f64 = 89. * 86_400_000.;
const EMPTY: &str = r#"{"success":true,"code":0,"data":[]}"#;
const POSITION: &str = r#"{"success":true,"code":0,"data":[{"positionId":"position","symbol":"BTC_USDT","positionType":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}]}"#;
const PLAN: &str = r#"{"id":"guarded","symbol":"BTC_USDT","side":4,"orderType":5,"vol":60,"dealVol":0,"price":0,"triggerPrice":55000,"triggerType":2,"trend":2,"state":1,"leverage":7,"openType":1,"executeCycle":2,"positionMode":1,"reduceOnly":true,"priceProtect":0,"stopLossPrice":0,"takeProfitPrice":0,"lossTrend":1,"profitTrend":1}"#;
const STOP: &str = r#"{"id":"guarded","orderId":"0","symbol":"BTC_USDT","positionId":"position","positionType":1,"vol":60,"volType":2,"triggerSide":2,"stopLossPrice":55000,"takeProfitPrice":0,"lossTrend":2,"profitTrend":1,"state":1,"priceProtect":0,"profitLossVolType":"SAME","takeProfitReverse":2,"stopLossReverse":2,"takeProfitType":0,"stopLossType":0,"takeProfitOrderPrice":0,"stopLossOrderPrice":0,"updateTime":1800000000160.25}"#;

fn requests(e: &mut TestEngine, now: f64) -> Vec<HttpRequest> {
    prepared(e, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn reply(e: &mut TestEngine, request: &HttpRequest, now: f64, body: &str) {
    e.handle(
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
}

// All HTTP and socket input is synthetic; prepare/claim uses the real signer.
fn boot(raw: f64, offset: f64) -> (TestEngine, SocketId, Vec<HttpRequest>) {
    let mut e = configured(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    });
    assert!(e.sync_host_time(raw, UTC + offset));
    e.start(raw);
    let market = requests(&mut e, raw).remove(0);
    reply(&mut e, &market, raw + 10., MARKETS);
    let actions = prepared(&mut e, raw + 10.);
    let socket = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let http = |path: &str| {
        actions
            .iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains(path) => Some(request.clone()),
                _ => None,
            })
            .unwrap()
    };
    reply(&mut e, &http("contract/ticker"), raw + 20., TICKERS);
    prepared(&mut e, raw + 20.);
    reply(
        &mut e,
        &http("position_mode"),
        raw + 100.125,
        r#"{"success":true,"code":0,"data":2}"#,
    );
    let reads = requests(&mut e, raw + 100.125);
    assert_eq!(reads.len(), 5);
    (e, socket, reads)
}

fn parameter<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .find_map(|pair| {
            pair.split_once('=')
                .filter(|(key, _)| *key == name)
                .map(|(_, value)| value)
        })
        .unwrap()
}

fn assert_bounds(request: &HttpRequest, start: f64, end: f64) {
    assert_eq!(request.method, Method::Get);
    for (field, expected) in [("start_time", start.floor()), ("end_time", end.floor())] {
        let value = parameter(request, field);
        assert_eq!(
            value.parse::<i64>(),
            Ok(expected as i64),
            "{}: {field} must be integer milliseconds",
            request.url
        );
    }
}

fn assert_stamp(request: &HttpRequest, expected: f64) {
    let value = request
        .headers
        .iter()
        .find(|(name, _)| *name == "Request-Time")
        .unwrap()
        .1
        .as_str();
    assert_eq!(value, (expected.floor() as i64).to_string());
}

#[test]
fn account_plan_snapshot_uses_integer_wire_bounds() {
    for offset in [0., 0.375, -0.375] {
        let (_, _, reads) = boot(0., offset);
        let plan = reads
            .iter()
            .find(|request| request.url.contains("planorder/list/orders"))
            .unwrap();
        let end = UTC + offset + 100.125;
        assert_bounds(plan, end - LOOKBACK, end);
        assert_stamp(plan, end);
    }
}

#[test]
fn account_stop_snapshot_uses_integer_wire_bounds() {
    let (_, _, reads) = boot(UTC, 0.375);
    let stop = reads
        .iter()
        .find(|request| request.url.contains("stoporder/list/orders"))
        .unwrap();
    assert_bounds(stop, UTC + 100.5 - LOOKBACK, UTC + 100.5);
    assert_stamp(stop, UTC + 100.5);
}

fn ready(raw: f64, offset: f64) -> (TestEngine, SocketId) {
    let (mut e, socket, reads) = boot(raw, offset);
    for request in reads {
        let body = if request.url.contains("open_positions") {
            POSITION
        } else {
            EMPTY
        };
        reply(&mut e, &request, raw + 110., body);
    }
    e.handle(raw + 120., Event::WsOpened { socket });
    prepared(&mut e, raw + 120.);
    (e, socket)
}

fn inspection(
    raw: f64,
    offset: f64,
    stop: bool,
    rollback: bool,
) -> (TestEngine, HttpRequest, String) {
    let (mut e, socket) = ready(raw, offset);
    let row = if stop { STOP } else { PLAN };
    let channel = if stop {
        "push.personal.stop.order"
    } else {
        "push.personal.plan.order"
    };
    e.handle(
        raw + 160.25,
        Event::WsMessage {
            socket,
            text: &format!(r#"{{"channel":"{channel}","data":{row}}}"#),
        },
    );
    prepared(&mut e, raw + 160.25);
    if rollback {
        assert!(e.sync_host_time(raw + 190., UTC + offset + 190. - 7_200_000.));
    }
    e.handle(
        raw + 200.875,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: if stop {
                "guarded__stop_loss"
            } else {
                "guarded"
            }
            .into(),
            price: Some(54000.),
            amount: None,
        }),
    );
    let request = requests(&mut e, raw + 200.875).remove(0);
    (e, request, row.into())
}

#[test]
fn protective_plan_inspection_uses_integer_calibrated_bounds() {
    let (_, request, _) = inspection(0., 0.375, false, false);
    assert!(request.url.contains("planorder/list/orders"));
    let end = UTC + 201.25;
    assert_bounds(&request, end - LOOKBACK, end);
    assert_stamp(&request, end);
}

#[test]
fn protective_stop_inspection_floors_retained_update_bound() {
    let (_, request, _) = inspection(UTC, 0.375, true, false);
    assert!(request.url.contains("stoporder/list/orders"));
    assert_bounds(&request, UTC + 160.25 - LOOKBACK, UTC + 201.25);
    assert_stamp(&request, UTC + 201.25);
}

#[test]
fn protective_inspection_keeps_backward_clock_correction() {
    let (_, request, _) = inspection(0., 0.375, false, true);
    let end = UTC + 201.25 - 7_200_000.;
    assert_bounds(&request, end - LOOKBACK, end);
    assert_stamp(&request, end);
}

#[test]
fn invalid_clock_observations_preserve_known_time_instead_of_zero() {
    let (mut e, socket) = ready(0., 0.);
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(!e.sync_host_time(150., invalid));
    }
    e.handle(
        160.,
        Event::WsMessage {
            socket,
            text: &format!(r#"{{"channel":"push.personal.plan.order","data":{PLAN}}}"#),
        },
    );
    prepared(&mut e, 160.);
    e.handle(
        200.,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "guarded".into(),
            price: Some(54000.),
            amount: None,
        }),
    );
    let request = requests(&mut e, 200.).remove(0);
    assert_bounds(&request, UTC + 200. - LOOKBACK, UTC + 200.);
    assert_stamp(&request, UTC + 200.);
}

#[test]
fn integer_clock_inspection_and_native_modify_payload_are_unchanged() {
    let (mut e, socket) = ready(0., 0.);
    e.handle(
        160.,
        Event::WsMessage {
            socket,
            text: &format!(r#"{{"channel":"push.personal.plan.order","data":{PLAN}}}"#),
        },
    );
    prepared(&mut e, 160.);
    e.handle(
        200.,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "guarded".into(),
            price: Some(54000.),
            amount: None,
        }),
    );
    let inspect = requests(&mut e, 200.).remove(0);
    assert_bounds(&inspect, UTC + 200. - LOOKBACK, UTC + 200.);
    reply(
        &mut e,
        &inspect,
        210.,
        &format!(r#"{{"success":true,"code":0,"data":[{PLAN}]}}"#),
    );
    let modify = requests(&mut e, 210.).remove(0);
    assert_eq!(modify.method, Method::Post);
    assert!(modify.url.ends_with("/planorder/change_price"));
    assert_eq!(
        modify.body.as_deref(),
        Some(
            r#"{"symbol":"BTC_USDT","orderId":"guarded","triggerPrice":"54000","price":"0","orderType":5.0,"triggerType":2.0,"trend":2,"from":1}"#
        )
    );
    assert_stamp(&modify, UTC + 210.);
}

#[test]
fn ordinary_write_payloads_and_signing_headers_do_not_depend_on_fractional_time() {
    fn writes(offset: f64) -> Vec<(String, Option<String>, Vec<(&'static str, String)>)> {
        let (mut e, _) = ready(0., offset);
        let mut captured = Vec::new();
        let commands = [
            Command::PlaceOrders { command: 1, orders: vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000}"#).unwrap()] },
            Command::CancelOrders { command: 2, ids: vec!["placed".into()] },
            Command::SetLeverage { command: 3, symbol: "BTCUSDT".into(), leverage: 7., side: Some(safe_cex_engine::model::PositionSide::Long) },
        ];
        for (index, command) in commands.into_iter().enumerate() {
            let now = 200.5 + index as f64 * 2000.;
            e.handle(now, Event::Command(command));
            let request = requests(&mut e, now).remove(0);
            assert_eq!(request.method, Method::Post);
            assert_stamp(&request, UTC + now);
            let body = if index == 0 {
                r#"{"success":true,"code":0,"data":"placed"}"#
            } else if index == 1 {
                r#"{"success":true,"code":0,"data":[{"orderId":"placed","errorCode":0}]}"#
            } else {
                r#"{"success":true,"code":0}"#
            };
            reply(&mut e, &request, now + 1., body);
            captured.push((request.url, request.body, request.headers));
            prepared(&mut e, now + 1.);
        }
        captured
    }
    assert_eq!(writes(0.), writes(0.375));
}
