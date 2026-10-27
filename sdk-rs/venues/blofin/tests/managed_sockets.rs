//! Production binding with genuine installed-provider signing and captured IDs.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Event, SocketId, Venue};
use safe_cex_venue_blofin::Blofin;
use support::Harness;

fn sends(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: own, text } if *own == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
fn public(h: &Harness) -> SocketId {
    h.actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .expect("actual public connection")
}
#[test]
fn blofin_declares_managed_sockets_for_the_production_binding() {
    assert!(<Blofin as Venue>::managed_sockets());
}
#[test]
fn duplicate_public_open_has_no_subscriptions_or_supervision_effects() {
    let mut h = Harness::new();
    let socket = public(&h);
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert!(
        sends(&h.actions, socket)
            .iter()
            .any(|s| s.contains("tickers"))
    );
    assert!(sends(&h.actions, socket).contains(&"ping"));
    h.actions.clear();
    h.handle(2., Event::WsOpened { socket });
    h.collect();
    assert!(
        h.actions.is_empty(),
        "duplicate Opened must own no work: {:?}",
        h.actions
    );
}
#[test]
fn duplicate_private_open_cannot_resign_or_repeat_ping() {
    let mut h = Harness::new();
    let socket = h.private;
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert_eq!(
        sends(&h.actions, socket)
            .iter()
            .filter(|s| s.contains("\"op\":\"login\""))
            .count(),
        1
    );
    assert!(sends(&h.actions, socket).contains(&"ping"));
    h.actions.clear();
    h.handle(2., Event::WsOpened { socket });
    h.collect();
    assert!(
        h.actions.is_empty(),
        "duplicate Opened cannot sign or reschedule: {:?}",
        h.actions
    );
}
#[test]
fn private_subscriptions_need_the_real_ack_and_duplicate_ack_is_inert() {
    let mut h = Harness::new();
    let socket = h.private;
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert!(
        sends(&h.actions, socket)
            .iter()
            .all(|s| !s.contains("subscribe"))
    );
    h.actions.clear();
    h.handle(
        2.,
        Event::WsMessage {
            socket,
            text: "pong",
        },
    );
    h.collect();
    assert!(sends(&h.actions, socket).is_empty());
    h.handle(
        3.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    h.collect();
    assert_eq!(
        sends(&h.actions, socket)
            .iter()
            .filter(|s| s.contains("subscribe"))
            .count(),
        4
    );
    h.actions.clear();
    h.handle(
        4.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    h.collect();
    assert!(h.actions.is_empty());
}

fn event(h: &mut Harness, now: f64, value: Event<'_>) -> Vec<Action> {
    h.actions.clear();
    h.handle(now, value);
    h.collect();
    std::mem::take(&mut h.actions)
}
fn topics(actions: &[Action], socket: SocketId) -> Vec<(String, String, String)> {
    sends(actions, socket)
        .into_iter()
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .flat_map(|v| {
            let op = v["op"].as_str().unwrap_or_default().to_owned();
            v["args"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |a| {
                    (
                        op.clone(),
                        a["channel"].as_str().unwrap_or_default().to_owned(),
                        a["instId"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
#[test]
fn books5_is_shared_until_the_last_book_or_candle_reference_leaves() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new();
    let socket = public(&h);
    event(&mut h, 0., Event::WsOpened { socket });
    let add = event(
        &mut h,
        1.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&add, socket)
            .iter()
            .filter(|(_, c, _)| c == "books5")
            .count(),
        1
    );
    assert!(
        event(
            &mut h,
            2.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into()
            })
        )
        .is_empty()
    );
    let candle = event(
        &mut h,
        3.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    assert_eq!(
        topics(&candle, socket),
        vec![("subscribe".into(), "candle1m".into(), "BTC-USDT".into())]
    );
    assert!(
        event(
            &mut h,
            4.,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: "BTCUSDT".into()
            })
        )
        .is_empty()
    );
    let remove = event(
        &mut h,
        5.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&remove, socket),
        vec![("unsubscribe".into(), "books".into(), "BTC-USDT".into())]
    );
    let last = event(
        &mut h,
        6.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    let last = topics(&last, socket);
    assert_eq!(last.len(), 2);
    assert!(last.contains(&("unsubscribe".into(), "books5".into(), "BTC-USDT".into())));
    assert!(last.contains(&("unsubscribe".into(), "candle1m".into(), "BTC-USDT".into())));
}
#[test]
fn public_reconnect_does_not_retire_private_or_accept_old_public_ingress() {
    let mut h = Harness::new();
    let old = public(&h);
    let private = h.private;
    event(&mut h, 0., Event::WsOpened { socket: old });
    event(&mut h, 0., Event::WsOpened { socket: private });
    event(
        &mut h,
        0.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let retired = event(
        &mut h,
        1.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    assert!(
        retired
            .iter()
            .all(|a| !matches!(a, Action::WsClose { socket } if *socket == private))
    );
    let retry: Vec<_> = retired
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (751. ..=1001.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retry.len(), 1);
    assert_eq!(
        sends(
            &event(
                &mut h,
                2.,
                Event::WsMessage {
                    socket: private,
                    text: "ping"
                }
            ),
            private
        ),
        vec!["pong"]
    );
    let before = h.e.snapshot_frame(100.).to_vec();
    for ev in [
        Event::WsOpened { socket: old },
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
        Event::WsMessage {
            socket: old,
            text: "malformed",
        },
    ] {
        assert!(event(&mut h, 3., ev).is_empty());
        assert_eq!(h.e.snapshot_frame(100.), before);
    }
    let connected = event(&mut h, retry[0].1, Event::TimerFired { timer: retry[0].0 });
    let current = connected
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(current, old);
    let opened = event(&mut h, retry[0].1, Event::WsOpened { socket: current });
    assert_eq!(topics(&opened, current).len(), 2);
    assert!(sends(&opened, current).contains(&"ping"));
    assert!(event(&mut h, retry[0].1, Event::TimerFired { timer: retry[0].0 }).is_empty());
}
#[test]
fn nonfinite_public_and_private_ingress_leave_full_store_and_actions_unchanged() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::new();
        h.seed();
        let public = public(&h);
        let private = h.private;
        event(&mut h, 0., Event::WsOpened { socket: public });
        event(&mut h, 0., Event::WsOpened { socket: private });
        event(
            &mut h,
            0.,
            Event::WsMessage {
                socket: private,
                text: r#"{"event":"login","code":"0"}"#,
            },
        );
        let before = h.e.snapshot_frame(100.).to_vec();
        for socket in [public, private] {
            for ev in [
                Event::WsOpened { socket },
                Event::WsClosed { socket, code: 1006 },
                Event::WsMessage {
                    socket,
                    text: "invalid",
                },
            ] {
                h.e.handle(invalid, ev);
                let actions: Vec<_> = h.e.drain_actions().collect();
                assert!(actions.is_empty(), "invalid time owns no work: {actions:?}");
                assert_eq!(h.e.snapshot_frame(100.), before);
            }
            assert_eq!(
                sends(
                    &event(
                        &mut h,
                        1.,
                        Event::WsMessage {
                            socket,
                            text: "ping"
                        }
                    ),
                    socket
                ),
                vec!["pong"]
            );
        }
    }
}

#[test]
fn unauthenticated_financial_frames_cannot_publish_and_current_ack_enables_them() {
    let mut h = Harness::new();
    h.seed();
    let socket = h.private;
    event(&mut h, 0., Event::WsOpened { socket });
    let before = h.e.snapshot_frame(100.).to_vec();
    let row = r#"{"arg":{"channel":"account"},"data":{"totalEquity":"777"}}"#;
    assert!(event(&mut h, 1., Event::WsMessage { socket, text: row }).is_empty());
    assert_eq!(h.e.snapshot_frame(100.), before);
    event(
        &mut h,
        2.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    event(&mut h, 3., Event::WsMessage { socket, text: row });
    assert_eq!(h.e.store().balance().total, 776.5);
    assert_ne!(h.e.snapshot_frame(100.), before);
}
#[test]
fn genuine_interest_count_refusal_rolls_back_new_refs_and_preserves_the_peer() {
    use safe_cex_engine::Command;
    let mut h = Harness::new();
    let public = public(&h);
    let private = h.private;
    event(&mut h, 0., Event::WsOpened { socket: public });
    event(&mut h, 0., Event::WsOpened { socket: private });
    event(
        &mut h,
        0.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let original = h.e.store().markets().to_vec();
    let mut markets = original.clone();
    for n in 1..4096 {
        let mut m = original[0].clone();
        m.symbol = format!("S{n}USDT");
        m.id = format!("S{n}-USDT");
        markets.push(m);
    }
    h.e.store_mut().set_markets(markets);
    let refused = event(
        &mut h,
        1.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(sends(&refused, public).is_empty());
    assert!(refused.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(e)) if e.message.contains("Capacity"))));
    assert!(
        refused
            .iter()
            .all(|a| !matches!(a, Action::WsClose { .. } | Action::WsConnect { .. }))
    );
    assert_eq!(
        sends(
            &event(
                &mut h,
                2.,
                Event::WsMessage {
                    socket: private,
                    text: "ping"
                }
            ),
            private
        ),
        vec!["pong"]
    );
    h.e.store_mut().set_markets(original);
    let recovered = event(
        &mut h,
        3.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&recovered, public).len(),
        2,
        "refusal must not leave phantom refs"
    );
}
#[test]
fn real_queued_byte_pressure_invalidates_books_and_recovers_demand_without_retiring_private() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new();
    let public = public(&h);
    let private = h.private;
    event(&mut h, 0., Event::WsOpened { socket: public });
    event(&mut h, 0., Event::WsOpened { socket: private });
    event(
        &mut h,
        0.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let original = h.e.store().markets()[0].clone();
    let mut markets = vec![original.clone()];
    for n in 1..3999 {
        let mut m = original.clone();
        m.symbol = format!("S{n}USDT");
        m.id = format!("{}-{n}-USDT", "A".repeat(215));
        markets.push(m);
    }
    h.e.store_mut().set_markets(markets.clone());
    let established = event(
        &mut h,
        1.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&established, public).len(),
        7998,
        "two original ticker interests were already active"
    );
    assert!(
        sends(&established, public)
            .iter()
            .all(|frame| frame.len() <= 4096)
    );
    event(
        &mut h,
        2.,
        Event::WsMessage {
            socket: public,
            text: r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"snapshot","data":[{"seqId":"1","bids":[["99","1"]],"asks":[["101","1"]]}]}"#,
        },
    );
    assert!(!h.e.book("BTCUSDT").unwrap().is_empty());
    for m in &mut markets[1..] {
        m.id.replace_range(..1, "B");
    }
    h.e.store_mut().set_markets(markets);
    let pressure = event(
        &mut h,
        3.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    assert!(
        pressure
            .iter()
            .any(|a| matches!(a, Action::WsClose {socket} if *socket == public))
    );
    assert!(
        pressure
            .iter()
            .all(|a| !matches!(a, Action::WsClose {socket} if *socket == private))
    );
    assert!(h.e.book("BTCUSDT").is_none_or(|book| book.is_empty()));
    assert!(pressure.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(e)) if e.message.contains("Capacity"))));
    let retry: Vec<_> = pressure
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (753. ..=1003.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retry.len(), 1);
    assert_eq!(
        sends(
            &event(
                &mut h,
                4.,
                Event::WsMessage {
                    socket: private,
                    text: "ping"
                }
            ),
            private
        ),
        vec!["pong"]
    );
    let connected = event(&mut h, retry[0].1, Event::TimerFired { timer: retry[0].0 });
    let current = connected
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(current, public);
    let replay = event(&mut h, retry[0].1, Event::WsOpened { socket: current });
    let active = topics(&replay, current);
    assert_eq!(active.len(), 8001);
    assert_eq!(
        active
            .iter()
            .filter(|(_, channel, _)| channel == "candle1m")
            .count(),
        1
    );
    assert!(
        sends(&replay, current)
            .iter()
            .all(|frame| frame.len() <= 4096)
    );
    assert!(event(&mut h, retry[0].1, Event::WsOpened { socket: public }).is_empty());
}

use safe_cex_engine::{Command, Cx, Engine, Response, TimerId, VenueError};
use safe_cex_venue_blofin::Options;
#[derive(Default)]
struct PressureTrace {
    old: Option<SocketId>,
    public: Option<safe_cex_engine::socket_lifecycle::FeedId>,
    retired: Vec<SocketId>,
    armed: bool,
    changes: u8,
}
struct ProposalPressure(Blofin, std::rc::Rc<std::cell::RefCell<PressureTrace>>);
impl Venue for ProposalPressure {
    fn managed_sockets() -> bool {
        <Blofin as Venue>::managed_sockets()
    }
    fn socket_capabilities() -> safe_cex_engine::socket_lifecycle::SocketCapabilities {
        <Blofin as Venue>::socket_capabilities()
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        use safe_cex_engine::socket_lifecycle::LifecycleEvent;
        {
            let mut trace = self.1.borrow_mut();
            if let LifecycleEvent::Retired { epoch, .. } = &event
                && (trace.public == Some(epoch.feed()) || trace.old == Some(epoch.socket()))
            {
                trace.public = Some(epoch.feed());
                trace.retired.push(epoch.socket());
            }
            if let LifecycleEvent::Reconnect(feed) = &event
                && trace.public == Some(*feed)
                && trace.armed
            {
                trace.armed = false;
                for _ in 0..safe_cex_engine::socket_lifecycle::MAX_FEEDS {
                    cx.set_socket_interests(*feed, vec![])
                        .expect("actual proposal credit");
                }
            }
        }
        let injection = match &event {
            LifecycleEvent::Opened(epoch)
                if self.1.borrow().old == Some(epoch.socket()) && self.1.borrow().changes != 3 =>
            {
                self.1.borrow().changes
            }
            LifecycleEvent::Opened(epoch)
                if self.1.borrow().old != Some(epoch.socket()) && self.1.borrow().changes == 3 =>
            {
                3
            }
            _ => 0,
        };
        let private_feed = match &event {
            LifecycleEvent::Opened(epoch) => Some(epoch.feed()),
            _ => None,
        };
        self.0.on_socket_event(cx, event)?;
        if injection == 3 {
            let feed = private_feed.unwrap();
            cx.set_socket_interests(feed, vec![])
                .expect("real private proposal before retirement");
            self.0.on_command(
                cx,
                Command::SubscribeOrderBook {
                    symbol: "BTCUSDT".into(),
                },
            );
            cx.stop_socket_feed(feed);
        } else if injection == 2 || injection == 4 {
            self.0.on_command(
                cx,
                Command::SubscribeOrderBook {
                    symbol: "BTCUSDT".into(),
                },
            );
            if injection == 2 {
                self.0.on_command(
                    cx,
                    Command::SubscribeCandles {
                        symbol: "BTCUSDT".into(),
                        interval: safe_cex_engine::model::Interval::M1,
                    },
                );
                self.0.on_command(
                    cx,
                    Command::SubscribeOrderBook {
                        symbol: "BTCUSDT".into(),
                    },
                );
                self.0.on_command(
                    cx,
                    Command::SubscribeCandles {
                        symbol: "BTCUSDT".into(),
                        interval: safe_cex_engine::model::Interval::M1,
                    },
                );
            } else {
                self.0.on_command(
                    cx,
                    Command::UnsubscribeOrderBook {
                        symbol: "BTCUSDT".into(),
                    },
                );
            }
        }
        Ok(())
    }
    fn socket_control(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.0.socket_control(store, feed, authenticated, text)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        values: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.0
            .encode_socket_interests(store, feed, subscribe, values, emit)
    }

    type Pending = <Blofin as Venue>::Pending;
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
        self.0.start(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str) {
        self.0.on_message(cx, socket, text);
    }
    fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Self::Pending>, pending: &mut Self::Pending, item: &str) {
        self.0.on_item(cx, pending, item);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_response(cx, pending, response);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.0.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.0.on_sign_error(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.0
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_stream_error(cx, pending, response);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId) {
        self.0.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        let renew = self.1.borrow().changes == 6
            && matches!(&command, Command::UnsubscribeOrderBook { .. });
        self.0.on_command(cx, command);
        if renew {
            self.0.on_command(
                cx,
                Command::SubscribeOrderBook {
                    symbol: "BTCUSDT".into(),
                },
            );
        }
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn pressure_actions(e: &mut Engine<ProposalPressure>, now: f64, event: Event<'_>) -> Vec<Action> {
    e.handle(now, event);
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn retry(actions: &[Action], now: f64) -> (TimerId, f64) {
    let retries: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > now && *at_ms <= now + 2000. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retries.len(), 1, "one genuine backoff: {actions:?}");
    retries[0]
}
#[test]
fn reconnect_proposal_capacity_retires_the_replacement_and_recovers_desired_work() {
    let (signer, identities) = safe_cex_crypto::signer::LocalSigner::from_json(
        r#"{"primary":{"key":"key","secret":"secret","passphrase":"pass"}}"#.into(),
    )
    .unwrap();
    let trace = std::rc::Rc::new(std::cell::RefCell::new(PressureTrace::default()));
    let mut e = safe_cex_engine::test_support::seeded_engine(ProposalPressure(
        Blofin::new(Options {
            identities: identities.clone(),
            ..Options::default()
        }),
        trace.clone(),
    ));
    e.set_signer(Some(std::sync::Arc::new(signer))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(0.);
    let mut startup = vec![];
    for body in [support::INSTRUMENT, r#"{"code":"0","data":[]}"#] {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        let request = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.id),
                _ => None,
            })
            .unwrap();
        startup.extend(actions);
        e.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    startup.extend(safe_cex_engine::test_support::prepared_actions(&mut e, 0.));
    let socket = |private: bool| {
        startup
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. }
                    if url.ends_with(if private { "/private" } else { "/public" }) =>
                {
                    Some(*socket)
                }
                _ => None,
            })
            .unwrap()
    };
    let old = socket(false);
    let private = socket(true);
    pressure_actions(&mut e, 0., Event::WsOpened { socket: old });
    pressure_actions(&mut e, 0., Event::WsOpened { socket: private });
    pressure_actions(
        &mut e,
        0.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    trace.borrow_mut().old = Some(old);
    trace.borrow_mut().armed = true;
    let closed = pressure_actions(
        &mut e,
        1.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    let (timer, at) = retry(&closed, 1.);
    let failed = pressure_actions(&mut e, at, Event::TimerFired { timer });
    let retired = trace.borrow().retired.clone();
    assert_eq!(
        retired.len(),
        2,
        "replacement must retire through Closed, not permanently stop: {failed:?}"
    );
    let replacement = retired[1];
    assert_ne!(replacement, old);
    assert!(
        !failed
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::WsSend { .. }))
    );
    assert_eq!(
        failed
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == replacement))
            .count(),
        1
    );
    let (next, next_at) = retry(&failed, at);
    for stale in [old, replacement] {
        assert!(pressure_actions(&mut e, at, Event::WsOpened { socket: stale }).is_empty());
    }
    let peer = pressure_actions(
        &mut e,
        at,
        Event::WsMessage {
            socket: private,
            text: "ping",
        },
    );
    assert_eq!(sends(&peer, private), vec!["pong"]);
    let connected = pressure_actions(&mut e, next_at, Event::TimerFired { timer: next });
    let current = connected
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(current, replacement);
    assert_ne!(current, old);
    let opened = pressure_actions(&mut e, next_at, Event::WsOpened { socket: current });
    assert_eq!(topics(&opened, current).len(), 2);
    assert!(sends(&opened, current).contains(&"ping"));
    assert!(pressure_actions(&mut e, next_at, Event::TimerFired { timer: next }).is_empty());
}

struct LoginGate {
    inner: std::sync::Arc<safe_cex_crypto::signer::LocalSigner>,
    parked: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl safe_cex_engine::signing::Signer for LoginGate {
    fn sign(
        &self,
        request: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        if self.parked.load(std::sync::atomic::Ordering::SeqCst) {
            Box::pin(std::future::pending())
        } else {
            self.inner.sign(request)
        }
    }
    fn immediate(&self) -> bool {
        !self.parked.load(std::sync::atomic::Ordering::SeqCst) && self.inner.immediate()
    }
    fn sign_ready(
        &self,
        request: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        (!self.parked.load(std::sync::atomic::Ordering::SeqCst))
            .then(|| self.inner.sign_ready(request))
            .flatten()
    }
}
fn gated_engine(
    policy: safe_cex_engine::PublicFeedPolicy,
) -> (
    Engine<Blofin>,
    Vec<Action>,
    std::sync::Arc<safe_cex_crypto::signer::LocalSigner>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let (provider, identities) = safe_cex_crypto::signer::LocalSigner::from_json(
        r#"{"primary":{"key":"key","secret":"secret","passphrase":"pass"}}"#.into(),
    )
    .unwrap();
    let provider = std::sync::Arc::new(provider);
    let parked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut e = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
        identities: identities.clone(),
        ..Options::default()
    }));
    e.set_signer(Some(std::sync::Arc::new(LoginGate {
        inner: provider.clone(),
        parked: parked.clone(),
    })))
    .unwrap();
    e.set_identities(identities).unwrap();
    e.set_public_feed_policy(policy).unwrap();
    e.start(0.);
    let mut startup = vec![];
    for body in [support::INSTRUMENT, r#"{"code":"0","data":[]}"#] {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        let request = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.id),
                _ => None,
            })
            .unwrap();
        startup.extend(actions);
        e.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    startup.extend(safe_cex_engine::test_support::prepared_actions(&mut e, 0.));
    (e, startup, provider, parked)
}
#[test]
fn suppressed_public_still_opens_authenticates_and_serves_the_real_private_peer() {
    let (mut e, actions, _, _) = gated_engine(safe_cex_engine::PublicFeedPolicy::Shared);
    let sockets: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::WsConnect { socket, url, .. } => Some((*socket, url.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(sockets.len(), 1);
    assert!(sockets[0].1.ends_with("/private"));
    let socket = sockets[0].0;
    e.handle(0., Event::WsOpened { socket });
    let opened = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
    assert_eq!(
        sends(&opened, socket)
            .iter()
            .filter(|s| s.contains("\"op\":\"login\""))
            .count(),
        1
    );
    e.handle(
        0.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let ready = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
    assert_eq!(
        sends(&ready, socket)
            .iter()
            .filter(|s| s.contains("subscribe"))
            .count(),
        4
    );
    e.handle(
        1.,
        Event::WsMessage {
            socket,
            text: "ping",
        },
    );
    assert_eq!(
        sends(
            &safe_cex_engine::test_support::prepared_actions(&mut e, 1.),
            socket
        ),
        vec!["pong"]
    );
}
#[test]
fn a_retired_real_parked_login_cannot_release_into_its_replacement_epoch() {
    use safe_cex_engine::signing::Signer;
    let (mut e, actions, provider, parked) =
        gated_engine(safe_cex_engine::PublicFeedPolicy::Direct);
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    parked.store(true, std::sync::atomic::Ordering::SeqCst);
    e.handle(1., Event::WsOpened { socket });
    let held: Vec<_> = e.drain_actions().collect();
    assert!(sends(&held, socket).is_empty(), "ping must wait for login");
    let (attempt, work) = held
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work)),
            _ => None,
        })
        .expect("actual parked installed-provider login");
    let result = provider.sign_ready(work).unwrap();
    e.handle(2., Event::WsClosed { socket, code: 1006 });
    let closed: Vec<_> = e.drain_actions().collect();
    let (timer, at) = retry(&closed, 2.);
    let before = e.snapshot_frame(100.).to_vec();
    e.handle(3., Event::Signed { attempt, result });
    assert!(e.drain_actions().next().is_none());
    assert_eq!(e.snapshot_frame(100.), before);
    parked.store(false, std::sync::atomic::Ordering::SeqCst);
    e.handle(at, Event::TimerFired { timer });
    let connected = safe_cex_engine::test_support::prepared_actions(&mut e, at);
    let current = connected
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(current, socket);
    e.handle(at, Event::WsOpened { socket: current });
    let opened = safe_cex_engine::test_support::prepared_actions(&mut e, at);
    let sends = sends(&opened, current);
    let login = sends
        .iter()
        .position(|s| s.contains("\"op\":\"login\""))
        .unwrap();
    let ping = sends.iter().position(|s| *s == "ping").unwrap();
    assert!(login < ping);
    let frame: serde_json::Value = serde_json::from_str(sends[login]).unwrap();
    assert_eq!(
        frame["args"][0]["timestamp"].as_u64(),
        Some(at.floor() as u64)
    );
    assert!(sends.iter().all(|s| !s.contains("subscribe")));
    e.handle(
        at,
        Event::WsMessage {
            socket: current,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let ready = safe_cex_engine::test_support::prepared_actions(&mut e, at);
    assert_eq!(topics(&ready, current).len(), 4);
}

fn callback_changes(mode: u8) -> (Engine<ProposalPressure>, SocketId, SocketId) {
    let (provider, identities) = safe_cex_crypto::signer::LocalSigner::from_json(
        r#"{"primary":{"key":"key","secret":"secret","passphrase":"pass"}}"#.into(),
    )
    .unwrap();
    let trace = std::rc::Rc::new(std::cell::RefCell::new(PressureTrace {
        changes: mode,
        ..PressureTrace::default()
    }));
    let mut e = safe_cex_engine::test_support::seeded_engine(ProposalPressure(
        Blofin::new(Options {
            identities: identities.clone(),
            ..Options::default()
        }),
        trace.clone(),
    ));
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(0.);
    let mut actions = vec![];
    for body in [support::INSTRUMENT, r#"{"code":"0","data":[]}"#] {
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        let request = out
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.id),
                _ => None,
            })
            .unwrap();
        actions.extend(out);
        e.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    actions.extend(safe_cex_engine::test_support::prepared_actions(&mut e, 0.));
    let socket = |private: bool| {
        actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. }
                    if url.ends_with(if private { "/private" } else { "/public" }) =>
                {
                    Some(*socket)
                }
                _ => None,
            })
            .unwrap()
    };
    let public = socket(false);
    let private = socket(true);
    trace.borrow_mut().old = Some(public);
    (e, public, private)
}
#[test]
fn a_preopen_book_or_candle_is_present_on_the_initial_physical_open() {
    use safe_cex_engine::model::Interval;
    for (command, channel) in [
        (
            Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            },
            "books",
        ),
        (
            Command::SubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
            },
            "candle1m",
        ),
    ] {
        let mut h = Harness::new();
        let socket = public(&h);
        let pending = event(&mut h, 0., Event::Command(command));
        assert!(sends(&pending, socket).is_empty());
        let opened = event(&mut h, 1., Event::WsOpened { socket });
        let topics = topics(&opened, socket);
        assert!(
            topics
                .iter()
                .any(|(op, c, id)| op == "subscribe" && c == channel && id == "BTC-USDT"),
            "pre-open desired topic must replay: {topics:?}"
        );
        assert_eq!(topics.iter().filter(|(_, c, _)| c == "books5").count(), 1);
    }
}
#[test]
fn a_second_pending_public_change_and_duplicate_addition_keep_their_demand() {
    let (mut e, public, _) = callback_changes(2);
    let opened = pressure_actions(&mut e, 1., Event::WsOpened { socket: public });
    let added = topics(&opened, public);
    assert!(
        added.iter().any(|(_, c, _)| c == "candle1m"),
        "second pending addition lost: {added:?}"
    );
    assert_eq!(added.iter().filter(|(_, c, _)| c == "books5").count(), 1);
    let first = pressure_actions(
        &mut e,
        2.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(
        topics(&first, public).is_empty(),
        "one of two book refs must remain"
    );
    let last = pressure_actions(
        &mut e,
        3.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&last, public),
        vec![("unsubscribe".into(), "books".into(), "BTC-USDT".into())]
    );
    let first = pressure_actions(
        &mut e,
        4.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(
        topics(&first, public).is_empty(),
        "one of two candle refs must remain"
    );
    let last = pressure_actions(
        &mut e,
        5.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let topics = topics(&last, public);
    assert_eq!(topics.len(), 2);
    assert!(topics.contains(&("unsubscribe".into(), "books5".into(), "BTC-USDT".into())));
    assert!(topics.contains(&("unsubscribe".into(), "candle1m".into(), "BTC-USDT".into())));
}
#[test]
fn a_private_proposal_refusal_cannot_discard_the_real_public_pending_change() {
    let (mut e, public, private) = callback_changes(3);
    pressure_actions(&mut e, 0., Event::WsOpened { socket: public });
    let refused = pressure_actions(&mut e, 1., Event::WsOpened { socket: private });
    assert!(
        refused
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == private))
    );
    assert!(
        !refused
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == public))
    );
    assert!(
        topics(&refused, public)
            .iter()
            .any(|(_, c, _)| c == "books")
    );
    let remove = pressure_actions(
        &mut e,
        2.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&remove, public).len(),
        2,
        "public commit must retain local book ref despite private refusal"
    );
}
#[test]
fn a_pending_addition_followed_by_removal_leaves_no_book_subscription() {
    let (mut e, public, _) = callback_changes(4);
    let out = pressure_actions(&mut e, 1., Event::WsOpened { socket: public });
    let all = topics(&out, public);
    let mut live = std::collections::BTreeSet::new();
    for (op, channel, id) in all {
        if op == "subscribe" {
            live.insert((channel, id));
        } else {
            live.remove(&(channel, id));
        }
    }
    assert!(
        !live
            .iter()
            .any(|(channel, _)| channel == "books" || channel == "books5"),
        "pending removal must leave no live book demand: {live:?}"
    );
    let out = pressure_actions(
        &mut e,
        2.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&out, public).len(),
        2,
        "later fresh addition must not be a phantom ref increment"
    );
}
#[test]
fn a_pending_removal_followed_by_renewal_restores_the_book_subscription() {
    let (mut e, public, _) = callback_changes(6);
    pressure_actions(&mut e, 0., Event::WsOpened { socket: public });
    let initial = pressure_actions(
        &mut e,
        1.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let out = pressure_actions(
        &mut e,
        2.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let mut live = std::collections::BTreeSet::new();
    for (op, channel, id) in topics(&initial, public)
        .into_iter()
        .chain(topics(&out, public))
    {
        if op == "subscribe" {
            live.insert((channel, id));
        } else {
            live.remove(&(channel, id));
        }
    }
    assert!(
        live.contains(&("books".into(), "BTC-USDT".into())),
        "renewal must survive the pending removal: {live:?}"
    );
}
