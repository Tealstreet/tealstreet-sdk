//! The shared three-request window; native create/cancel orders bypass it.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::{Cx, Method, TimerId, json::ArrayAt};
use std::collections::VecDeque;

pub(crate) struct Request {
    pub method: Method,
    pub path: String,
    pub query: String,
    pub body: Option<String>,
    pub pending: Pending,
    pub stream: Option<ArrayAt>,
}

#[derive(Default)]
pub(crate) struct Budget {
    queue: VecDeque<Request>,
    used: u8,
    timer: Option<TimerId>,
}

fn limited(method: Method, path: &str) -> bool {
    !matches!(
        (method, path),
        (Method::Put, "/g-orders/create") | (Method::Delete, "/g-orders/cancel" | "/g-orders")
    )
}

impl PhemexPerpetuals {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        query: &str,
        body: Option<String>,
        pending: Pending,
        stream: Option<ArrayAt>,
    ) {
        let request = Request {
            method,
            path: path.into(),
            query: query.into(),
            body,
            pending,
            stream,
        };
        if limited(method, path) {
            self.http_budget.queue.push_back(request);
            self.drain_http(cx);
        } else {
            self.dispatch_http(cx, request);
        }
    }

    fn dispatch_http(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let headers = self
            .credentials
            .as_ref()
            .map(|c| {
                c.headers(
                    &request.path,
                    &request.query,
                    request.body.as_deref().unwrap_or(""),
                    cx.now,
                )
            })
            .unwrap_or_default();
        let url = format!(
            "{}{}{}",
            self.rest(),
            request.path,
            if request.query.is_empty() {
                String::new()
            } else {
                format!("?{}", request.query)
            }
        );
        if let Some(at) = request.stream {
            cx.request_streamed_body(
                request.method,
                url,
                headers,
                request.body,
                at,
                request.pending,
            );
        } else {
            cx.request(request.method, url, headers, request.body, request.pending);
        }
    }

    fn drain_http(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.now < self.paused_until {
            if self.http_budget.timer.is_none() {
                self.http_budget.timer = Some(cx.set_timer(self.paused_until - cx.now));
            }
            return;
        }
        while self.http_budget.used < 3 {
            let Some(request) = self.http_budget.queue.pop_front() else {
                break;
            };
            let current = match &request.pending {
                Pending::Depth { symbol, generation } => self
                    .books
                    .get(symbol)
                    .is_some_and(|book| book.fetching && book.generation == *generation),
                Pending::Account(request) => self.account_request_current(request),
                Pending::Trade(request) => self.trade_request_current(request),
                _ => true,
            };
            if !current {
                continue;
            }
            if self.http_budget.used == 0 {
                // Hosts may rebase pending timer delays between startup and replay.
                // Only this callback resets the window, never absolute clock math.
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
    use crate::{BookState, Options};
    use safe_cex_engine::{
        Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketId, Venue, book::Book,
    };

    struct Probe {
        venue: PhemexPerpetuals,
        requests: Vec<Request>,
    }
    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            for request in self.requests.drain(..) {
                self.venue.request(
                    cx,
                    request.method,
                    &request.path,
                    &request.query,
                    request.body,
                    request.pending,
                    request.stream,
                );
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            self.venue.on_timer(cx, timer);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if let Command::FetchLeverage { command, symbol } = command {
                if symbol == "/start-account" {
                    self.venue.start_account(cx);
                    return;
                }
                let request = request(command, Method::Get, &symbol);
                self.venue.request(
                    cx,
                    request.method,
                    &request.path,
                    &request.query,
                    request.body,
                    request.pending,
                    request.stream,
                );
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {
            self.venue.books.get_mut("BTCUSDT").unwrap().generation = 2;
        }
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
            self.venue.on_item(cx, pending, item);
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            response: Response<'_>,
        ) {
            self.venue.on_response(cx, pending, response);
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
            self.venue.on_request_failed(cx, pending, error);
        }
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
            query: "symbol=BTC%2BUSDT&flag=false".into(),
            body: None,
            pending: Pending::Candles {
                command,
                rows: vec![],
                invalid: false,
            },
            stream: None,
        }
    }
    fn engine(requests: Vec<Request>) -> Engine<Probe> {
        let mut venue = PhemexPerpetuals::new(Options {
            key: Some("fixture-key".into()),
            secret: Some("fixture-secret".into()),
            ..Default::default()
        });
        let mut book = BookState::new(1);
        book.fetching = true;
        venue.books.insert("BTCUSDT".into(), book);
        venue.generation = 1;
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
    fn xhr_reads_and_settings_share_fifo_but_only_native_order_mutations_bypass() {
        for path in [
            "/public/products",
            "/md/v3/ticker/24hr/all",
            "/md/v2/fullbook",
            "/g-accounts/accountPositions",
            "/exchange/public/md/v2/kline/list",
            "/api-data/g-futures/trades",
            "/api-data/g-futures/orders",
            "/md/v2/trade",
        ] {
            assert!(limited(Method::Get, path));
        }
        for path in ["/g-positions/leverage", "/g-positions/switch-pos-mode-sync"] {
            assert!(limited(Method::Put, path));
        }
        for (method, path) in [
            (Method::Put, "/g-orders/create"),
            (Method::Delete, "/g-orders/cancel"),
            (Method::Delete, "/g-orders"),
        ] {
            assert!(!limited(method, path));
        }
        assert!(limited(Method::Get, "/g-orders/create"));
        let mut queued = request(4, Method::Put, "/g-positions/leverage");
        queued.body = Some("{\"enabled\":false,\"leverage\":0}".into());
        let mut e = engine(vec![
            request(1, Method::Get, "/public/products"),
            request(2, Method::Get, "/md/v3/ticker/24hr/all"),
            request(3, Method::Get, "/btc"),
            queued,
            request(5, Method::Get, "/fifth"),
            request(6, Method::Put, "/g-orders/create"),
            request(7, Method::Delete, "/g-orders/cancel"),
            request(8, Method::Delete, "/g-orders"),
        ]);
        let actions: Vec<_> = e.drain_actions().collect();
        let paths = |actions: &[Action]| {
            http(actions)
                .iter()
                .map(|r| {
                    r.url
                        .split('?')
                        .next()
                        .unwrap()
                        .strip_prefix("https://api.phemex.com")
                        .unwrap()
                        .to_owned()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(&actions),
            [
                "/public/products",
                "/md/v3/ticker/24hr/all",
                "/btc",
                "/g-orders/create",
                "/g-orders/cancel",
                "/g-orders"
            ]
        );
        let (reset, at) = timer(&actions);
        assert_eq!(at, 1050.);
        e.handle(
            100.,
            Event::HttpFailed {
                request: http(&actions)[0].id,
                error: "network failure",
            },
        );
        assert!(
            http(&e.drain_actions().collect::<Vec<_>>()).is_empty(),
            "failure never refunds quota"
        );
        e.handle(
            1049.,
            Event::Command(Command::FetchLeverage {
                command: 9,
                symbol: "/later".into(),
            }),
        );
        assert!(http(&e.drain_actions().collect::<Vec<_>>()).is_empty());
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            paths(&actions),
            ["/g-positions/leverage", "/fifth", "/later"]
        );
        let requests = http(&actions);
        let expected = e.venue().venue.credentials.as_ref().unwrap().headers(
            "/g-positions/leverage",
            "symbol=BTC%2BUSDT&flag=false",
            r#"{"enabled":false,"leverage":0}"#,
            at,
        );
        assert_eq!(requests[0].headers, expected);
        assert!(
            requests[0]
                .headers
                .contains(&("X-PHEMEX-REQUEST-EXPIRY", "61".into())),
            "fresh expiry from dispatch time"
        );
        assert_eq!(
            requests[0].body.as_deref(),
            Some(r#"{"enabled":false,"leverage":0}"#)
        );
        assert!(requests[0].url.ends_with("?symbol=BTC%2BUSDT&flag=false"));
        e.handle(2000., Event::TimerFired { timer: reset });
        assert!(
            e.drain_actions().next().is_none(),
            "old timer cannot renew slots"
        );
        let (reset, at) = timer(&actions);
        assert_eq!(at, 2050.);
        e.handle(at, Event::TimerFired { timer: reset });
        assert!(
            e.drain_actions().next().is_none(),
            "idle reset does not rearm itself"
        );
        e.handle(
            2500.,
            Event::Command(Command::FetchLeverage {
                command: 10,
                symbol: "/idle".into(),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(http(&actions).len(), 1);
        assert_eq!(timer(&actions).1, 3500.);
    }

    #[test]
    fn queued_streams_keep_the_selector_pending_command_and_every_candle() {
        let mut requests: Vec<_> = (1..=4)
            .map(|id| request(id, Method::Get, &format!("/read-{id}")))
            .collect();
        requests[3].stream = Some(ArrayAt::Path(&["data", "rows"]));
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
        for data in
            br#"{"code":0,"data":{"rows":[[1,60,0,1,2,0.5,1.5,3,0],[2,60,0,2,3,1,2.5,4,0]]}}"#
                .chunks(3)
        {
            e.handle(at, Event::HttpChunk { request: id, data });
        }
        e.handle(at, Event::HttpEnd { request: id });
        let result = e
            .drain_actions()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: 4, result }) => Some(result.unwrap()),
                _ => None,
            })
            .unwrap();
        #[derive(serde::Deserialize)]
        struct Candle {
            timestamp: f64,
            close: f64,
        }
        let rows: Vec<Candle> = serde_json::from_str(&result).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (
                rows[0].timestamp,
                rows[0].close,
                rows[1].timestamp,
                rows[1].close
            ),
            (1., 1.5, 2., 2.5)
        );
    }

    #[test]
    fn obsolete_snapshot_is_skipped_before_quota_without_clearing_replacement() {
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
        let requests = http(&actions);
        assert_eq!(requests.len(), 3);
        assert!(requests[0].url.contains("/snapshot-2?"));
        assert!(requests[1].url.contains("/sixth?"));
        assert!(requests[2].url.contains("/seventh?"));
        let book = &e.venue().venue.books["BTCUSDT"];
        assert!(book.fetching);
        assert_eq!(book.generation, 2);
    }
    #[test]
    fn host_rebased_startup_timer_is_the_only_window_boundary() {
        let mut e = engine(
            (1..=4)
                .map(|id| request(id, Method::Get, "/read"))
                .collect(),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let (reset, original_at) = timer(&actions);
        assert_eq!(original_at, 1050.);
        assert_eq!(http(&actions).len(), 3);
        // A host carries the remaining startup delay into its replay clock.
        // The venue must honor that callback, even before the old epoch's deadline.
        e.handle(868., Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(http(&actions).len(), 1);
        assert_eq!(timer(&actions).1, 1868.);
    }

    #[test]
    fn failed_depth_retry_reenters_fifo_without_refunding_the_failed_slot() {
        let mut depth = request(3, Method::Get, "/md/v2/fullbook");
        depth.pending = Pending::Depth {
            symbol: "BTCUSDT".into(),
            generation: 1,
        };
        let mut e = engine(vec![
            request(1, Method::Get, "/public/products"),
            request(2, Method::Get, "/md/v3/ticker/24hr/all"),
            depth,
            request(4, Method::Get, "/eth-fourth"),
        ]);
        let actions: Vec<_> = e.drain_actions().collect();
        let (reset, at) = timer(&actions);
        e.handle(
            50.,
            Event::HttpFailed {
                request: http(&actions)[2].id,
                error: "temporary network failure",
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let (retry, retry_at) = timer(&actions);
        assert_eq!(retry_at, at);
        assert!(http(&actions).is_empty());
        // Deliver simultaneous callbacks in the order least favorable to the retry.
        e.handle(retry_at, Event::TimerFired { timer: retry });
        assert!(http(&e.drain_actions().collect::<Vec<_>>()).is_empty());
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        let requests = http(&actions);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].url.contains("/eth-fourth?"));
        assert!(
            requests[1]
                .url
                .ends_with("/md/v2/fullbook?symbol=BTCUSDT&limit=1000")
        );
        assert_eq!(e.venue().venue.books["BTCUSDT"].generation, 2);
        assert!(e.venue().venue.books["BTCUSDT"].fetching);
        assert_eq!(
            requests[1].headers,
            e.venue().venue.credentials.as_ref().unwrap().headers(
                "/md/v2/fullbook",
                "symbol=BTCUSDT&limit=1000",
                "",
                at
            )
        );
    }

    #[test]
    fn expired_account_cycle_is_skipped_without_consuming_dispatch_slots() {
        let mut e = engine(
            (1..=3)
                .map(|id| request(id, Method::Get, "/read"))
                .collect(),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let (reset, _) = timer(&actions);
        e.handle(
            100.,
            Event::Command(Command::FetchLeverage {
                command: 4,
                symbol: "/start-account".into(),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(http(&actions).is_empty());
        let deadline = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 20100. => Some(*timer),
                _ => None,
            })
            .unwrap();
        // A delayed host delivers the reconciliation deadline before the old
        // quota callback. Neither expired currency request may be sent afterward.
        e.handle(20100., Event::TimerFired { timer: deadline });
        e.drain_actions().for_each(drop);
        e.handle(20100., Event::TimerFired { timer: reset });
        assert!(http(&e.drain_actions().collect::<Vec<_>>()).is_empty());
        for id in 5..=7 {
            e.handle(
                20101.,
                Event::Command(Command::FetchLeverage {
                    command: id,
                    symbol: format!("/current-{id}"),
                }),
            );
        }
        let actions: Vec<_> = e.drain_actions().collect();
        let requests = http(&actions);
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|r| r.url.contains("/current-")));
    }

    #[test]
    fn quota_reset_during_shared_429_pause_waits_until_retry_after() {
        let mut e = engine(
            (1..=4)
                .map(|id| request(id, Method::Get, "/read"))
                .collect(),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let (reset, at) = timer(&actions);
        e.handle(
            100.,
            Event::HttpResponse {
                request: http(&actions)[0].id,
                response: Response {
                    status: 429,
                    headers: "x-ratelimit-retry-after-contract: 60",
                    body: "{}",
                },
            },
        );
        e.drain_actions().for_each(drop);
        e.handle(at, Event::TimerFired { timer: reset });
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(http(&actions).is_empty());
        let (resume, at) = timer(&actions);
        assert_eq!(at, 60100.);
        e.handle(at, Event::TimerFired { timer: resume });
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(http(&actions).len(), 1);
        assert_eq!(timer(&actions).1, 61100.);
    }
}
