//! Read covers on a real OKX source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! venue pair 3): the open-order census (a regular page and four algo pages,
//! each its own read) commits its rows with the cover of its earliest page.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::log::code;
use safe_cex_engine::wire::section;
use safe_cex_engine::{Action, Engine, Event, Notice};
use safe_cex_venue_okx::Okx;

const POSITION: &str = r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","liqPx":"30000","mgnMode":"cross","imr":"200","mmr":"20"}"#;
const STREAM_ORDER: &str = r#"{"instId":"BTC-USDT-SWAP","ordId":"777","ordType":"limit","state":"live","side":"buy","sz":"4","accFillSz":"0","px":"38000","lever":"10","reduceOnly":"false"}"#;
const OCO: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"9","ordType":"oco","state":"live","side":"sell","sz":"1","slTriggerPx":"30000","slTriggerPxType":"last","tpTriggerPx":"50000","tpTriggerPxType":"last","reduceOnly":"true"}"#;
/// An OCO that decodes but carries no trigger: it maps to no order.
const OCO_WITHOUT_TRIGGERS: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"9","ordType":"oco","state":"live","side":"sell","sz":"1","reduceOnly":"true"}"#;

/// A started account at 4 ms: the startup census went out at 3 ms and the
/// stream subscribed while it ran, so a second census follows it.
fn startup() -> (Engine<Okx>, safe_cex_engine::SocketId, Vec<(Action, f64)>) {
    startup_on(false, false)
}

/// `startup` on the demo (`testnet`) or EEA alias.
fn startup_on(
    testnet: bool,
    eea: bool,
) -> (Engine<Okx>, safe_cex_engine::SocketId, Vec<(Action, f64)>) {
    let mut e = crate::support::engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        testnet,
        eea,
        ..Options::default()
    });
    e.start(0.0);
    let mut initial = prepared(&mut e, 0.0);
    // Instruments answer at 1 ms (EEA also lists futures), tickers at 2 ms.
    for (path, now) in [("/instruments?", 1.0), ("/market/tickers", 2.0)] {
        while let Some(at) = initial.iter().position(|a| url(a).contains(path)) {
            let a = initial.remove(at);
            let body = if url(&a).contains("instType=FUTURES") {
                envelope(&MARKET.replace("BTC-USDT-SWAP", "BTC-USDT-261225").replace(
                    "\"instType\":\"SWAP\"",
                    "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\",\"uly\":\"BTC-USDT\"",
                ))
            } else if path == "/market/tickers" {
                envelope(TICKER)
            } else {
                envelope(MARKET)
            };
            reply(&mut e, &a, &body, now);
            initial.extend(prepared(&mut e, now));
        }
    }
    let business = initial
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/business") => Some(*socket),
            _ => None,
        })
        .unwrap();
    let config = initial
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let pending = prepared(&mut e, 3.0);
    let private = pending
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    for socket in [private, business] {
        e.handle(4.0, Event::WsOpened { socket });
        prepared(&mut e, 4.0);
        ws(&mut e, socket, r#"{"event":"login","code":"0","msg":""}"#);
        let out = prepared(&mut e, 4.0);
        for timer in out.iter().filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 4.0 => Some(*timer),
            _ => None,
        }) {
            e.handle(4.0, Event::TimerFired { timer });
            prepared(&mut e, 4.0);
        }
    }
    let pending = pending
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .map(|a| (a, 3.0))
        .collect();
    (e, private, pending)
}

fn url(action: &Action) -> &str {
    match action {
        Action::Http(r) => &r.url,
        _ => "",
    }
}

fn census_start(action: &Action) -> bool {
    url(action).contains("/trade/orders-pending?instType=SWAP") && !url(action).contains("after=")
}

fn census_end(action: &Action) -> bool {
    url(action).contains("ordType=move_order_stop")
}

/// Answers every read, each `step` ms after the last, until `censuses` order
/// censuses have answered; `orders(n, url)` is census n's page body. Returns
/// the reads still out with their send times, the clock, census start
/// times, and how many rows were reported skipped.
fn drive(
    e: &mut Engine<Okx>,
    mut out: Vec<(Action, f64)>,
    mut now: f64,
    step: f64,
    censuses: usize,
    orders: impl Fn(usize, &str) -> String,
) -> (Vec<(Action, f64)>, f64, Vec<f64>, usize) {
    let mut done = 0;
    let mut skipped = 0;
    let mut starts = Vec::new();
    for (action, sent) in &out {
        if census_start(action) {
            starts.push(*sent);
        }
    }
    while done < censuses && !out.is_empty() {
        let (action, _) = out.remove(0);
        let url = url(&action).to_owned();
        let body = if url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else if url.contains("/positions?") {
            envelope(POSITION)
        } else if url.contains("/trade/orders") {
            orders(done, &url)
        } else {
            envelope("")
        };
        now += step;
        reply(e, &action, &body, now);
        if census_end(&action) {
            done += 1;
        }
        for action in prepared(e, now) {
            if matches!(&action, Action::Notice(Notice::Event(n)) if n.code == code::ROW_SKIPPED) {
                skipped += 1;
            }
            if matches!(action, Action::Http(_)) {
                if census_start(&action) {
                    starts.push(now);
                }
                out.push((action, now));
            }
        }
    }
    (out, now, starts, skipped)
}

fn empty(_: usize, _: &str) -> String {
    envelope("")
}

fn account_cover(e: &mut Engine<Okx>, now: f64) -> (Vec<u16>, Option<f64>) {
    let frame = e.snapshot_frame(now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

/// Each page answers 1 s after the last: the cover is the first page's send.
#[test]
fn a_census_cover_is_its_first_pages_send_beside_orders() {
    let (mut e, _, out) = startup();
    let (_, now, starts, _) = drive(&mut e, out, 4.0, 1_000.0, 2, empty);
    assert_eq!(
        starts.len(),
        2,
        "the startup census and the one after the subscribe"
    );
    let (kinds, cover) = account_cover(&mut e, now);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    assert_eq!(cover, Some(starts[1]), "the earliest page, not the last");
    assert!(starts[1] + 4_000.0 <= now, "the last page went out later");
}

/// H10: the stream opens an order after a census went out; the census lacks
/// it, keeps the row, and the cover stays at the census before it.
#[test]
fn a_stream_order_after_the_census_went_out_leaves_no_cover() {
    let (mut e, private, out) = startup();
    let (out, now, starts, _) = drive(&mut e, out, 4.0, 1.0, 1, empty);
    assert_eq!(account_cover(&mut e, now).1, Some(starts[0]));
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{STREAM_ORDER}]}}"#),
    );
    let (_, now, _, _) = drive(&mut e, out, now, 1.0, 1, empty);
    assert!(e.store().order("777").is_some(), "the stream row stays");
    assert_eq!(
        account_cover(&mut e, now).1,
        Some(starts[0]),
        "not the outrun census's"
    );
}

/// A working OCO that decodes but maps to no order keeps its stored legs and
/// proves no cover; the skip is reported.
#[test]
fn a_working_row_that_maps_to_nothing_keeps_its_legs_and_proves_no_cover() {
    let (mut e, _, out) = startup();
    let page = |row: &'static str| {
        move |_: usize, url: &str| envelope(if url.contains("ordType=oco") { row } else { "" })
    };
    let (out, now, starts, _) = drive(&mut e, out, 4.0, 1.0, 1, page(OCO));
    assert!(e.store().order("9_sl").is_some() && e.store().order("9_tp").is_some());
    assert_eq!(
        account_cover(&mut e, now).1,
        Some(starts[0]),
        "the control proves one"
    );
    let (_, now, _, _) = drive(&mut e, out, now, 1.0, 1, page(OCO_WITHOUT_TRIGGERS));
    assert!(e.store().order("9_sl").is_some(), "the stored legs stay");
    assert!(e.store().order("9_tp").is_some(), "the stored legs stay");
    assert_eq!(
        account_cover(&mut e, now).1,
        Some(starts[0]),
        "the census proves nothing newer"
    );
}

/// A new working row that maps to nothing is reported and proves no cover.
#[test]
fn a_new_working_row_that_maps_to_nothing_is_skipped_visibly() {
    let (mut e, _, out) = startup();
    let page = |_: usize, url: &str| {
        envelope(if url.contains("ordType=oco") {
            OCO_WITHOUT_TRIGGERS
        } else {
            ""
        })
    };
    let (_, now, _, skipped) = drive(&mut e, out, 4.0, 1.0, 2, page);
    assert!(!e.store().orders().iter().any(|o| o.id.starts_with('9')));
    assert_eq!(skipped, 1, "reported once, not per census");
    assert_eq!(account_cover(&mut e, now).1, None);
}

/// The follow-up a refused census owes waits at least 2 s from that census's
/// start, so a busy symbol cannot drive back-to-back census reads; it reads once.
#[test]
fn the_follow_up_to_a_refused_census_is_rate_bounded() {
    let (mut e, private, out) = startup();
    let (out, now, starts, _) = drive(&mut e, out, 4.0, 1.0, 1, empty);
    let started = *starts.last().unwrap();
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{STREAM_ORDER}]}}"#),
    );
    let mut out = out;
    let mut now = now;
    let mut armed = Vec::new();
    while let Some((action, _)) = (!out.is_empty()).then(|| out.remove(0)) {
        now += 1.0;
        reply(&mut e, &action, &envelope(""), now);
        for action in prepared(&mut e, now) {
            match action {
                Action::SetTimer { timer, at_ms } if at_ms > now => armed.push((timer, at_ms)),
                action @ Action::Http(_) => {
                    assert!(
                        !census_start(&action),
                        "no census straight after the refused one"
                    );
                    out.push((action, now));
                }
                _ => {}
            }
        }
    }
    let follow_ups: Vec<_> = armed
        .iter()
        .filter(|(_, at)| *at >= started + 2_000.0 - 0.01 && *at < started + 5_000.0)
        .copied()
        .collect();
    assert_eq!(follow_ups.len(), 1, "one follow-up: {armed:?}");
    let (timer, at) = follow_ups[0];
    e.handle(at, Event::TimerFired { timer });
    let reads = prepared(&mut e, at);
    assert_eq!(reads.iter().filter(|a| census_start(a)).count(), 1);
    e.handle(at, Event::TimerFired { timer });
    assert!(!prepared(&mut e, at).iter().any(census_start), "fires once");
}

/// A page's `RefreshOrders` ask at `now`; the reads it starts, sent at `now`.
fn refresh(
    e: &mut Engine<Okx>,
    command: u32,
    symbols: Option<Vec<String>>,
    now: f64,
) -> Vec<(Action, f64)> {
    e.handle(
        now,
        Event::Command(safe_cex_engine::Command::Venue {
            command,
            action: safe_cex_engine::VenueAction::RefreshOrders { symbols },
        }),
    );
    prepared(e, now)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .map(|a| (a, now))
        .collect()
}

/// OKX advertises read covers in its own snapshot's CAPABILITIES.
#[test]
fn okx_advertises_read_covers() {
    let (mut e, _, _) = startup();
    let frame = e.snapshot_frame(4.0).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    assert_ne!(
        flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT,
        0
    );
}

/// A refresh ask on a loaded source starts a census (regular and algo pages);
/// its answer publishes the account cover at the census's first send.
#[test]
fn a_refresh_ask_reads_the_census_and_publishes_its_send_time() {
    for (testnet, eea, symbols) in [
        (false, false, None),
        (false, false, Some(vec!["BTC-USDT-SWAP".to_owned()])),
        (true, false, None),
        (false, true, None),
    ] {
        // An EEA census reads swaps then futures: two final algo pages.
        let pages = if eea { 2 } else { 1 };
        let (mut e, _, out) = startup_on(testnet, eea);
        let (out, now, _, _) = drive(&mut e, out, 4.0, 1.0, 2 * pages, empty);
        assert!(!out.iter().any(|(a, _)| census_start(a)), "{out:?}");
        let now = now + 5_000.0;
        let reads = refresh(&mut e, 50, symbols.clone(), now);
        assert_eq!(
            reads.iter().filter(|(a, _)| census_start(a)).count(),
            1,
            "{symbols:?}"
        );
        let (_, end, starts, _) = drive(&mut e, reads, now, 100.0, pages, empty);
        assert_eq!(starts, [now], "{symbols:?}");
        assert!(end > now + 400.0, "the census spans its pages");
        assert_eq!(account_cover(&mut e, end).1, Some(now), "{symbols:?}");
    }
}

/// H7/H3: asks while the census is out start nothing; exactly one follow-up
/// census starts after it lands, and nothing after that.
#[test]
fn asks_while_a_census_is_out_get_one_census_after_it() {
    let (mut e, _, out) = startup();
    let (_, now, _, _) = drive(&mut e, out, 4.0, 1.0, 2, empty);
    let mut now = now + 5_000.0;
    let mut reads = refresh(&mut e, 50, None, now);
    assert_eq!(reads.iter().filter(|(a, _)| census_start(a)).count(), 1);
    // The first page answers, so the census is mid-flight on an algo page.
    let at = reads.iter().position(|(a, _)| census_start(a)).unwrap();
    let first = reads.remove(at);
    now += 1.0;
    reply(&mut e, &first.0, &envelope(""), now);
    reads.extend(
        prepared(&mut e, now)
            .into_iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .map(|a| (a, now)),
    );
    for command in 51..150 {
        now += 1.0;
        let more = refresh(&mut e, command, Some(vec!["BTC-USDT-SWAP".into()]), now);
        assert!(
            more.is_empty(),
            "H7: nothing more while it is out: {more:?}"
        );
    }
    let (out, now, starts, _) = drive(&mut e, reads, now, 1.0, 1, empty);
    assert_eq!(starts.len(), 1, "one follow-up census: {starts:?}");
    let follow_up = starts[0];
    let (out, now, starts, _) = drive(&mut e, out, now, 1.0, 1, empty);
    assert_eq!(starts, [follow_up], "nothing after the follow-up");
    assert!(!out.iter().any(|(a, _)| census_start(a)), "{out:?}");
    assert_eq!(account_cover(&mut e, now).1, Some(follow_up));
}

#[test]
fn a_source_ask_joins_an_ordinary_census_and_owes_one_successor() {
    let (mut e, _, mut out) = startup();
    for command in 100..200 {
        let reads = refresh(&mut e, command, None, 5.);
        assert!(
            !reads.iter().any(|(a, _)| census_start(a)),
            "do not replace the ordinary census"
        );
        out.extend(reads);
    }
    let (out, now, starts, _) = drive(&mut e, out, 5., 10., 2, empty);
    assert_eq!(
        starts.len(),
        2,
        "ordinary census plus one coalesced successor"
    );
    assert!(starts[0] < 5. && starts[1] > 5., "{starts:?}");
    assert_eq!(account_cover(&mut e, now).1, Some(starts[1]));
    assert!(!out.iter().any(|(a, _)| census_start(a)), "no third census");
}
