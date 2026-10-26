//! Snapshot repair authority and conservative full-quantity replacement proofs.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{Action, Event, HttpRequest, Method, Notice, Response, SocketId};
use support::{Driver, ORDER};

struct Packet {
    request: HttpRequest,
    preparation: Option<Preparation>,
}
fn packets(actions: Vec<Action>) -> Vec<Packet> {
    actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(Packet {
                request,
                preparation: None,
            }),
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    request,
                    preparation,
                    ..
                } => Some(Packet {
                    request,
                    preparation: Some(preparation),
                }),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
fn drain(e: &mut Driver) -> Vec<Packet> {
    packets(e.drain_actions().collect())
}
fn release(e: &mut Driver, packet: &Packet, now: f64) -> HttpRequest {
    let Some(preparation) = packet.preparation else {
        return packet.request.clone();
    };
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: packet.request.id.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } if request.id == packet.request.id => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("genuine exact Bind");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn respond(e: &mut Driver, request: &HttpRequest, now: f64, body: &str) {
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
fn old(fill: &str) -> String {
    ORDER
        .replace(
            "\"orderType\":\"Limit\"",
            "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\"",
        )
        .replace(
            "\"executedQuantity\":\"0\"",
            &format!("\"executedQuantity\":{fill}"),
        )
}
fn discover(body: &str) -> (Driver, SocketId, Vec<Packet>) {
    let (mut e, socket) = support::ready();
    e.handle(1000., Event::WsOpened { socket });
    let refreshes = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.);
    let orders = refreshes
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/orders") => Some(r),
            _ => None,
        })
        .expect("real private-open Orders observation");
    respond(&mut e, &orders, 1000., body);
    let work = drain(&mut e);
    (e, socket, work)
}
fn cancel(fill: &str) -> (Driver, SocketId, Packet) {
    let (e, socket, all) = discover(&format!("[{}]", old(fill)));
    let p = all
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .expect("native foreign trigger repair cancel");
    (e, socket, p)
}
fn ack(fill: &str) -> String {
    format!("{{\"id\":\"123\",\"status\":\"Cancelled\",\"executedQuantity\":{fill}}}")
}
fn no_place(all: &[Packet]) {
    assert!(
        !all.iter().any(|p| p.request.method == Method::Post),
        "unsafe proof must not place full frozen quantity"
    );
}

#[test]
fn native_zero_fill_cancel_and_replacement_keep_exact_wire() {
    let (mut e, _, p) = cancel("\"0\"");
    let c = release(&mut e, &p, 1000.);
    assert_eq!(
        c.body.as_deref(),
        Some("{\"orderId\":\"123\",\"symbol\":\"SOL_USDC_PERP\"}")
    );
    respond(&mut e, &c, 1000., &ack("\"0\""));
    let p = drain(&mut e)
        .into_iter()
        .find(|p| p.request.method == Method::Post)
        .expect("one replacement");
    let r = release(&mut e, &p, 1000.);
    assert_eq!(
        r.body.as_deref(),
        Some(
            "{\"symbol\":\"SOL_USDC_PERP\",\"side\":\"Bid\",\"orderType\":\"Market\",\"triggerPrice\":\"130.00\",\"triggerQuantity\":\"5.00\",\"triggerBy\":\"MarkPrice\"}"
        )
    );
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "X-API-Key" && v == "test-public-key")
    );
}
#[test]
fn snapshot_repair_cancel_is_a_genuine_managed_preparation() {
    let (_, _, p) = cancel("\"0\"");
    assert!(
        p.preparation.is_some(),
        "snapshot repair must carry actual engine Mutation custody"
    );
}
#[test]
fn accepted_cancel_successor_is_a_genuine_independent_preparation() {
    let (mut e, _, p) = cancel("\"0\"");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 1000., &ack("\"0\""));
    let work = drain(&mut e);
    let p = work
        .iter()
        .find(|p| p.request.method == Method::Post)
        .expect("replacement");
    assert!(
        p.preparation.is_some(),
        "successor needs its own issued context"
    );
    assert_ne!(p.request.id, c.id);
    assert_eq!(
        work.iter()
            .filter(|p| p.request.method == Method::Post)
            .count(),
        1
    );
}
#[test]
fn initial_partial_fill_without_a_new_delta_never_replaces_full_quantity() {
    let (mut e, _, p) = cancel("\"2\"");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 1000., &ack("\"2\""));
    no_place(&drain(&mut e));
}
#[test]
fn missing_snapshot_fill_is_not_default_zero_proof() {
    let body = old("\"0\"").replace(",\"executedQuantity\":\"0\"", "");
    let (mut e, _, all) = discover(&format!("[{body}]"));
    let p = all
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .expect("cancel still permitted");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 1000., &ack("\"0\""));
    no_place(&drain(&mut e));
}
macro_rules! unsafe_ack {
    ($name:ident, $body:expr) => {
        #[test]
        fn $name() {
            let (mut e, _, p) = cancel("\"0\"");
            let c = release(&mut e, &p, 1000.);
            respond(&mut e, &c, 1000., $body);
            no_place(&drain(&mut e));
        }
    };
}
unsafe_ack!(
    missing_ack_fill_is_not_zero_proof,
    r#"{"id":"123","status":"Cancelled"}"#
);
unsafe_ack!(
    null_ack_fill_is_not_zero_proof,
    r#"{"id":"123","status":"Cancelled","executedQuantity":null}"#
);
unsafe_ack!(
    nonfinite_ack_fill_is_not_zero_proof,
    r#"{"id":"123","status":"Cancelled","executedQuantity":"NaN"}"#
);
unsafe_ack!(
    infinite_ack_fill_is_not_zero_proof,
    r#"{"id":"123","status":"Cancelled","executedQuantity":"Infinity"}"#
);
unsafe_ack!(
    negative_ack_fill_is_not_zero_proof,
    r#"{"id":"123","status":"Cancelled","executedQuantity":"-1"}"#
);
unsafe_ack!(
    increased_ack_fill_suppresses_replacement,
    r#"{"id":"123","status":"Cancelled","executedQuantity":"1"}"#
);
unsafe_ack!(
    filled_ack_suppresses_replacement,
    r#"{"id":"123","status":"Filled","executedQuantity":"5"}"#
);
unsafe_ack!(
    wrong_cancel_id_cannot_authorize_replacement,
    r#"{"id":"other","status":"Cancelled","executedQuantity":"0"}"#
);
unsafe_ack!(
    nondefinitive_cancel_status_cannot_authorize_replacement,
    r#"{"id":"123","status":"New","executedQuantity":"0"}"#
);
unsafe_ack!(empty_cancel_ack_cannot_authorize_replacement, "{}");
#[test]
fn decreasing_ack_does_not_erase_partial_snapshot_evidence() {
    let (mut e, _, p) = cancel("\"2\"");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 1000., &ack("\"0\""));
    no_place(&drain(&mut e));
}
#[test]
fn terminal_snapshot_cannot_start_an_internal_cancel_or_place() {
    let body = old("\"0\"").replace("\"status\":\"New\"", "\"status\":\"Cancelled\"");
    let (_, _, all) = discover(&format!("[{body}]"));
    assert!(
        !all.iter().any(|p| p.request.method != Method::Get),
        "terminal discovery cannot authorize mutation"
    );
}
#[test]
fn repeated_same_order_in_one_snapshot_does_not_issue_two_repairs() {
    let row = old("\"0\"");
    let (_, _, all) = discover(&format!("[{row},{row}]"));
    assert_eq!(
        all.iter()
            .filter(|p| p.request.method == Method::Delete)
            .count(),
        1,
        "exactly one authority per native order"
    );
}
#[test]
fn cancel_response_at_the_twenty_second_boundary_never_places() {
    let (mut e, _, p) = cancel("\"0\"");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 21000., &ack("\"0\""));
    no_place(&drain(&mut e));
}
#[test]
fn held_cancel_preparation_expires_without_a_write() {
    let (mut e, _, p) = cancel("\"0\"");
    assert!(p.preparation.is_some(), "real preparation must be held");
    e.http_prepared(
        21000.,
        p.preparation.unwrap(),
        Prepared {
            id: p.request.id.0,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| match a {
        Action::Http(request) => request.method != Method::Get,
        Action::ManagedHttp(a) => match *a {
            HttpAction::Bind { request, .. } | HttpAction::Prepare { request, .. } => {
                request.method != Method::Get
            }
            HttpAction::BindGroup { .. } => true,
            _ => false,
        },
        _ => false,
    }));
}
#[test]
fn repair_has_no_numeric_command_progress_or_terminal() {
    let (mut e, _, p) = cancel("\"0\"");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 1000., &ack("\"0\""));
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { .. }
                | Notice::CommandFailed { .. }
                | Notice::CommandProgress { .. }
        )
    )));
}
#[test]
fn duplicate_cancel_response_cannot_create_a_second_replacement() {
    let (mut e, _, p) = cancel("\"0\"");
    let c = release(&mut e, &p, 1000.);
    respond(&mut e, &c, 1000., &ack("\"0\""));
    let first = drain(&mut e);
    assert_eq!(
        first
            .iter()
            .filter(|p| p.request.method == Method::Post)
            .count(),
        1
    );
    respond(&mut e, &c, 1000., &ack("\"0\""));
    no_place(&drain(&mut e));
}

macro_rules! unsafe_baseline {
    ($name:ident, $fill:expr) => {
        #[test]
        fn $name() {
            let (mut e, _, p) = cancel($fill);
            let c = release(&mut e, &p, 1000.);
            respond(&mut e, &c, 1000., &ack("\"0\""));
            no_place(&drain(&mut e));
        }
    };
}
unsafe_baseline!(nan_snapshot_is_not_zero_fill_proof, "\"NaN\"");
unsafe_baseline!(infinite_snapshot_is_not_zero_fill_proof, "\"Infinity\"");
unsafe_baseline!(
    negative_infinite_snapshot_is_not_zero_fill_proof,
    "\"-Infinity\""
);
unsafe_baseline!(negative_snapshot_is_not_zero_fill_proof, "\"-1\"");

fn stream_fill(e: &mut Driver, socket: SocketId, fill: &str, stamp: u64) {
    let body = format!(
        r#"{{"stream":"account.orderUpdate","data":{{"e":"orderUpdate","i":"123","E":{stamp},"X":"New","s":"SOL_USDC_PERP","o":"Market","S":"Bid","P":"130","Y":"5","q":"5","z":"{fill}"}}}}"#
    );
    e.handle(
        2000.,
        Event::WsMessage {
            socket,
            text: &body,
        },
    );
}
#[test]
fn stream_fill_high_water_survives_a_later_store_rollback() {
    let (mut e, socket, p) = cancel("\"0\"");
    let c = release(&mut e, &p, 1000.);
    stream_fill(&mut e, socket, "1", 2_000_000);
    assert_eq!(e.store().order("123").unwrap().filled, 1.);
    stream_fill(&mut e, socket, "0", 3_000_000);
    assert_eq!(e.store().order("123").unwrap().filled, 0.);
    e.drain_actions().for_each(drop);
    respond(&mut e, &c, 2000., &ack("\"0\""));
    no_place(&drain(&mut e));
}
#[test]
fn nonfinite_cancel_delivery_cannot_create_a_placement() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (mut e, _, p) = cancel("\"0\"");
        let c = release(&mut e, &p, 1000.);
        respond(&mut e, &c, now, &ack("\"0\""));
        no_place(&drain(&mut e));
    }
}
