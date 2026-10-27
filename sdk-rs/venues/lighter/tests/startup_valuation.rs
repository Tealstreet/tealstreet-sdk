//! Scripted native account valuation before public quotes; never contacts accounts.
mod support;
use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId};
use safe_cex_venue_lighter::Lighter;
use support::{ACCOUNT, META, NOW, OPEN, options, reply, reply_at, requests};

fn account(pnl: Option<f64>) -> String {
    let mut body: serde_json::Value = serde_json::from_str(ACCOUNT).unwrap();
    let position = &mut body["accounts"][0]["positions"][0];
    position["position_value"] = "240".into();
    position["realized_pnl"] = "999999".into();
    if let Some(pnl) = pnl {
        position["unrealized_pnl"] = pnl.to_string().into();
    } else {
        position.as_object_mut().unwrap().remove("unrealized_pnl");
    }
    body.to_string()
}
fn before_quotes(
    pnl: Option<f64>,
    price: f64,
) -> (Engine<Lighter>, SocketId, SocketId, Vec<String>) {
    let mut engine = support::engine(options());
    engine.start(NOW);
    let mut private = None;
    let mut public = None;
    let mut logs = vec![];
    for _ in 0..20 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(request) => {
                    if request.url.contains("orderBookDetails")
                        || request.url.contains("orderBooks")
                    {
                        reply(&mut engine, &request, 200, META);
                        engine.store_mut().update_ticker("ETH", |ticker| {
                            ticker.last = price;
                            ticker.mark = price;
                        });
                    } else if request.url.ends_with("/api/v1/exchangeStats") {
                        reply(
                            &mut engine,
                            &request,
                            200,
                            r#"{"code":200,"order_book_stats":[]}"#,
                        );
                    } else if request.url.contains("/account?") {
                        reply(&mut engine, &request, 200, &account(pnl));
                    } else if request.url.contains("accountActiveOrders") {
                        reply(&mut engine, &request, 200, OPEN);
                    } else {
                        panic!("unexpected request {}", request.url);
                    }
                }
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => private = Some(socket),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => public = Some(socket),
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => logs.push(message),
                _ => {}
            }
        }
    }
    (engine, private.unwrap(), public.unwrap(), logs)
}
fn refresh(engine: &mut Engine<Lighter>, socket: SocketId, body: &str) -> Vec<String> {
    engine.handle(NOW + 10., Event::WsOpened { socket });
    let request = requests(engine)
        .into_iter()
        .find(|request| request.url.contains("/account?"))
        .unwrap();
    reply_at(engine, NOW + 11., &request, 200, body);
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message),
            _ => None,
        })
        .collect()
}
#[test]
fn reported_private_pnl_and_notional_do_not_depend_on_zero_nonfinite_or_conflicting_quotes() {
    for price in [0., -1., f64::NAN, f64::INFINITY, 900.] {
        let (engine, _, _, _) = before_quotes(Some(4.), price);
        // The catalogue creates ticker scaffolding; the empty bootstrap stats
        // supply no quotes, so only the injected zero/invalid/conflicting row exists.
        let position = &engine.store().positions()[0];
        assert_eq!(
            (
                position.contracts,
                position.notional,
                position.unrealized_pnl
            ),
            (2., 240., 4.)
        );
        assert_eq!(position.realized_pnl, Some(999999.));
        let balance = engine.store().balance();
        assert_eq!(
            (balance.total, balance.free, balance.used, balance.upnl),
            (996., 900., 100., 4.)
        );
    }
}
#[test]
fn missing_private_pnl_retains_prior_side_valuation_and_warns_without_borrowing_realized_pnl() {
    let (mut engine, private, _, _) = before_quotes(Some(4.), 0.);
    let logs = refresh(&mut engine, private, &account(None));
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 4.);
    assert_eq!(engine.store().balance().upnl, 4.);
    assert!(
        logs.iter()
            .any(|message| message.contains("PnL unavailable"))
    );
    let opposite = account(None).replace("\"sign\":1", "\"sign\":-1");
    refresh(&mut engine, private, &opposite);
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 0.);
    assert_eq!(engine.store().balance().upnl, 0.);
}
#[test]
fn new_unpriced_exposure_is_visible_and_actual_quotes_recover_valuation() {
    let (mut engine, private, public, logs) = before_quotes(None, 0.);
    assert_eq!(engine.store().positions()[0].contracts, 2.);
    assert_eq!(engine.store().balance().upnl, 0.);
    assert_eq!(engine.store().balance().total, 1000.);
    assert!(
        logs.iter()
            .any(|message| message.contains("PnL unavailable"))
    );
    engine.handle(NOW + 1., Event::WsOpened { socket: public });
    engine.drain_actions().for_each(drop);
    engine.handle(NOW + 2., Event::WsMessage { socket: public,
        text: r#"{"channel":"ticker:1","ticker":{"last_trade_price":"110","mark_price":"110"}}"# });
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 20.);
    refresh(&mut engine, private, &account(None));
    assert_eq!(engine.store().balance().upnl, 20.);
    assert_eq!(engine.store().balance().total, 980.);
    refresh(&mut engine, private, &account(Some(5.)));
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 5.);
    assert_eq!(engine.store().balance().upnl, 5.);
    assert_eq!(engine.store().balance().total, 995.);
}
