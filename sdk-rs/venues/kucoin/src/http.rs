//! The shared Classic API budget; create/cancel orders use the unqueued transport.
use crate::{Kucoin, Pending, REST, encoded_path, sign};
use safe_cex_engine::{Cx, Method, TimerId, json::ArrayAt};
use std::collections::VecDeque;

pub(crate) struct Request {
    pub method: Method,
    pub path: String,
    pub body: Option<String>,
    pub pending: Pending,
    pub stream: Option<ArrayAt>,
    pub authenticated: bool,
}

#[derive(Default)]
pub(crate) struct Budget {
    queue: VecDeque<Request>,
    used: u8,
    timer: Option<TimerId>,
}

fn limited(method: Method, path: &str) -> bool {
    method == Method::Get
        || method == Method::Post
            && matches!(
                path,
                "/api/v1/bullet-public"
                    | "/api/v1/bullet-private"
                    | "/api/v2/changeCrossUserLeverage"
                    | "/api/v2/position/changeMarginMode"
                    | "/api/v2/position/switchPositionMode"
            )
}

impl Kucoin {
    pub(crate) fn http(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if limited(request.method, &request.path) {
            self.http_budget.queue.push_back(request);
            self.drain_http(cx);
        } else {
            self.dispatch_http(cx, request);
        }
    }

    fn dispatch_http(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        // Select credentials only when the queued request actually dispatches;
        // both the request signature and broker headers use this dispatch time.
        let read_credentials = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| request.method == Method::Get && request.authenticated)
            .map(|read| sign::Credentials {
                key: read.key.clone(),
                secret: read.secret.clone(),
                passphrase: read
                    .passphrase
                    .clone()
                    .or_else(|| self.options.passphrase.clone())
                    .unwrap_or_default(),
                broker: self.options.broker.clone(),
            });
        let headers = if request.authenticated {
            read_credentials
                .as_ref()
                .or(self.credentials.as_ref())
                .map_or_else(Vec::new, |credentials| {
                    sign::headers(
                        credentials,
                        request.method,
                        &request.path,
                        request.body.as_deref().unwrap_or(""),
                        cx.now,
                    )
                })
        } else {
            vec![]
        };
        let id = cx.request(
            request.method,
            format!("{REST}{}", encoded_path(&request.path)),
            headers,
            request.body,
            request.pending,
        );
        if let Some(at) = request.stream {
            cx.stream_response(id, at);
        }
    }

    fn drain_http(&mut self, cx: &mut Cx<'_, Pending>) {
        while self.http_budget.used < 3 {
            let Some(request) = self.http_budget.queue.pop_front() else {
                break;
            };
            // Unsubscribing or reconnecting invalidates a queued snapshot too.
            if let Pending::Depth { symbol, generation } = &request.pending
                && !self
                    .books
                    .get(symbol)
                    .is_some_and(|book| book.loading && book.generation == *generation)
            {
                continue;
            }
            if self.http_budget.used == 0 {
                // Reset by the timer, not absolute time: hosts may rebase clocks.
                self.http_budget.timer = Some(cx.set_timer(1000.0));
            }
            self.http_budget.used += 1;
            self.dispatch_http(cx, request);
        }
    }

    pub(crate) fn http_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.http_budget.timer != Some(timer) {
            return false;
        }
        self.http_budget.timer = None;
        self.http_budget.used = 0;
        self.drain_http(cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BookState, Options, sign::BrokerOptions};
    use safe_cex_engine::{
        Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketId, Venue, book::Book,
        model::Candle,
    };

    struct Probe {
        venue: Kucoin,
        requests: Vec<Request>,
    }
    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            for request in self.requests.drain(..) {
                self.venue.http(cx, request);
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            self.venue.http_timer(cx, timer);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if let Command::FetchLeverage { command, symbol } = command {
                self.venue.http(cx, request(command, Method::Get, &symbol));
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {
            self.venue.books.get_mut("BTCUSDT").unwrap().generation = 2;
        }
        fn on_item(&mut self, _: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
            if let Pending::Candles { rows, .. } = pending {
                rows.push(Candle {
                    timestamp: item.parse().unwrap(),
                    open: 0.,
                    high: 0.,
                    low: 0.,
                    close: 0.,
                    volume: 0.,
                });
            }
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
            if let Pending::Candles { command, rows, .. } = pending {
                cx.done(
                    command,
                    Ok(rows
                        .iter()
                        .map(|row| row.timestamp.to_string())
                        .collect::<Vec<_>>()
                        .join(",")),
                );
            }
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn request(command: u32, method: Method, path: &str) -> Request {
        Request {
            method,
            path: path.into(),
            body: None,
            pending: Pending::Candles {
                command,
                rows: vec![],
                invalid: false,
            },
            stream: None,
            authenticated: true,
        }
    }
    fn engine(requests: Vec<Request>) -> Engine<Probe> {
        engine_with_options(
            requests,
            Options {
                key: Some("fixture-key".into()),
                secret: Some("fixture-secret".into()),
                passphrase: Some("fixture-passphrase".into()),
                broker: Some(BrokerOptions {
                    partner: "partner".into(),
                    broker_key: "broker-key".into(),
                    broker_name: "broker".into(),
                    verify: None,
                }),
                ..Default::default()
            },
        )
    }

    fn engine_with_options(requests: Vec<Request>, options: Options) -> Engine<Probe> {
        let mut venue = Kucoin::new(options);
        venue.books.insert(
            "BTCUSDT".into(),
            BookState {
                loading: true,
                generation: 1,
                ..Default::default()
            },
        );
        let mut engine = Engine::new(Probe { venue, requests });
        engine.start(50.);
        engine
    }
    fn http(actions: &[Action]) -> Vec<&HttpRequest> {
        actions
            .iter()
            .filter_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .collect()
    }
    fn timer(actions: &[Action]) -> (TimerId, f64) {
        actions
            .iter()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((*timer, *at_ms))
                } else {
                    None
                }
            })
            .unwrap()
    }

    #[test]
    fn queued_secondary_read_signs_at_dispatch_and_streams_all_items() {
        let mut queued = request(4, Method::Get, "/secondary?symbol=BTC+USDT");
        queued.stream = Some(ArrayAt::Path(&["data", "items"]));
        let mut e = engine_with_options(
            vec![
                request(1, Method::Get, "/one"),
                request(2, Method::Get, "/two"),
                request(3, Method::Get, "/three"),
                queued,
                request(5, Method::Post, "/api/v1/orders"),
            ],
            Options {
                key: Some("primary-key".into()),
                secret: Some("primary-secret".into()),
                passphrase: Some("primary-pass".into()),
                read_credentials: Some(safe_cex_engine::ReadCredentials {
                    key: "read-key".into(),
                    secret: "read-secret".into(),
                    passphrase: None,
                }),
                broker: Some(BrokerOptions {
                    partner: "partner".into(),
                    broker_key: "broker-key".into(),
                    broker_name: "broker".into(),
                    verify: None,
                }),
                ..Options::default()
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(http(&actions).len(), 4);
        assert!(
            http(&actions)
                .iter()
                .take(3)
                .all(|r| r.headers.contains(&("KC-API-KEY", "read-key".into())))
        );
        assert!(
            http(&actions)[3]
                .headers
                .contains(&("KC-API-KEY", "primary-key".into()))
        );
        let (reset, at) = timer(&actions);
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        let requests = http(&actions);
        assert_eq!(requests.len(), 1);
        let credentials = sign::Credentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: "primary-pass".into(),
            broker: e.venue().venue.options.broker.clone(),
        };
        assert_eq!(
            requests[0].headers,
            sign::headers(
                &credentials,
                Method::Get,
                "/secondary?symbol=BTC+USDT",
                "",
                at
            )
        );
        assert!(requests[0].url.ends_with("/secondary?symbol=BTC%2BUSDT"));
        let id = requests[0].id;
        e.handle(
            at,
            Event::HttpStart {
                request: id,
                status: 200,
                headers: "",
            },
        );
        for data in br#"{"data":{"items":[1,2]}}"#.chunks(3) {
            e.handle(at, Event::HttpChunk { request: id, data });
        }
        e.handle(at, Event::HttpEnd { request: id });
        assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command:4, result:Ok(value) }) if value=="1,2")));
    }

    #[test]
    fn xhr_routes_share_the_budget_while_order_mutations_bypass_it() {
        for path in [
            "/api/v1/bullet-public",
            "/api/v1/bullet-private",
            "/api/v2/changeCrossUserLeverage",
            "/api/v2/position/changeMarginMode",
            "/api/v2/position/switchPositionMode",
        ] {
            assert!(limited(Method::Post, path));
        }
        assert!(limited(Method::Get, "/api/v1/orders/id"));
        for path in [
            "/api/v1/orders",
            "/api/v1/st-orders",
            "/api/v1/orders/multi",
        ] {
            assert!(!limited(Method::Post, path));
        }
        assert!(!limited(Method::Delete, "/api/v1/orders/id"));
        let mut e = engine(vec![
            request(1, Method::Get, "/first"),
            request(2, Method::Post, "/api/v1/bullet-public"),
            request(3, Method::Get, "/third"),
            request(4, Method::Post, "/api/v2/position/changeMarginMode"),
            request(5, Method::Get, "/fifth"),
            request(6, Method::Post, "/api/v1/orders"),
            request(7, Method::Delete, "/api/v1/orders/id"),
        ]);
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            http(&actions)
                .iter()
                .map(|r| r.url.strip_prefix(REST).unwrap())
                .collect::<Vec<_>>(),
            [
                "/first",
                "/api/v1/bullet-public",
                "/third",
                "/api/v1/orders",
                "/api/v1/orders/id"
            ]
        );
        let (reset, at) = timer(&actions);
        assert_eq!(at, 1050.);
        // A response, including a rejection, does not refund a slot.
        e.handle(
            100.,
            Event::HttpResponse {
                request: http(&actions)[0].id,
                response: Response {
                    status: 429,
                    headers: "",
                    body: "rejected",
                },
            },
        );
        assert!(http(&e.drain_actions().collect::<Vec<_>>()).is_empty());
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        let requests = http(&actions);
        assert_eq!(
            requests
                .iter()
                .map(|r| r.url.strip_prefix(REST).unwrap())
                .collect::<Vec<_>>(),
            ["/api/v2/position/changeMarginMode", "/fifth"]
        );
        let expected = sign::headers(
            e.venue().venue.credentials.as_ref().unwrap(),
            Method::Post,
            "/api/v2/position/changeMarginMode",
            "",
            at,
        );
        assert_eq!(requests[0].headers, expected);
        assert!(
            requests[0]
                .headers
                .contains(&("KC-API-TIMESTAMP", "1050".into()))
        );
        e.handle(2000., Event::TimerFired { timer: reset });
        assert!(
            e.drain_actions().next().is_none(),
            "obsolete reset must not release more work"
        );
        let (next, at) = timer(&actions);
        assert_eq!(at, 2050.);
        e.handle(at, Event::TimerFired { timer: next });
        assert!(
            e.drain_actions().next().is_none(),
            "idle window expires without rearming"
        );
        e.handle(
            2500.,
            Event::Command(Command::FetchLeverage {
                command: 8,
                symbol: "/after-idle".into(),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(http(&actions).len(), 1);
        assert_eq!(timer(&actions).1, 3500.);
    }

    #[test]
    fn queued_streams_keep_pending_identity_and_decode_all_items() {
        let mut requests: Vec<_> = (1..=4)
            .map(|id| request(id, Method::Get, &format!("/read-{id}")))
            .collect();
        requests[3].stream = Some(ArrayAt::Path(&["data", "items"]));
        let mut e = engine(requests);
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(http(&actions).len(), 3);
        let (reset, at) = timer(&actions);
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        let id = http(&actions)[0].id;
        e.handle(
            at,
            Event::HttpStart {
                request: id,
                status: 200,
                headers: "",
            },
        );
        for data in br#"{"data":{"items":[1,2]}}"#.chunks(3) {
            e.handle(at, Event::HttpChunk { request: id, data });
        }
        e.handle(at, Event::HttpEnd { request: id });
        assert!(e.drain_actions().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 4, result: Ok(value) }) if value == "1,2")));
    }

    #[test]
    fn cancelled_snapshot_generation_does_not_use_a_slot_or_clear_the_new_one() {
        let mut requests: Vec<_> = (1..=3)
            .map(|id| request(id, Method::Get, "/read"))
            .collect();
        for generation in [1, 2] {
            let mut r = request(0, Method::Get, &format!("/snapshot-{generation}"));
            r.pending = Pending::Depth {
                symbol: "BTCUSDT".into(),
                generation,
            };
            requests.push(r);
        }
        requests.push(request(6, Method::Get, "/sixth"));
        requests.push(request(7, Method::Get, "/seventh"));
        let mut e = engine(requests);
        let actions: Vec<_> = e.drain_actions().collect();
        let (reset, at) = timer(&actions);
        e.handle(
            100.,
            Event::WsClosed {
                socket: SocketId(1),
                code: 1006,
            },
        );
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            http(&actions)
                .iter()
                .map(|r| r.url.strip_prefix(REST).unwrap())
                .collect::<Vec<_>>(),
            ["/snapshot-2", "/sixth", "/seventh"]
        );
        let book = &e.venue().venue.books["BTCUSDT"];
        assert!(book.loading);
        assert_eq!(book.generation, 2);
    }
}
