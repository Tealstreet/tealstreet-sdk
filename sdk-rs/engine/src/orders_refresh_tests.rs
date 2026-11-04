//! A data source's bounded orders refresh (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! §5.2): per account at most one refresh read in flight and one pending
//! follow-up; only a read sent after an ask answers it.
use crate::book::Book;
use crate::engine::{Action, Command, Cx, Engine, Event, Venue, VenueAction};
use crate::order_read_cover::OrderReadScope;
use crate::request_context::{Operation, Policy, RetryPolicy, Scope};
use crate::{Method, RequestId, Response, SocketId, TimerId};

#[derive(Default)]
struct Source {
    refreshes: bool,
    /// Sends its own orders read at start, as a venue's startup census does.
    startup_read: bool,
    /// The account scope keys (one read each); empty: one key, "all".
    keys: Vec<&'static str>,
    /// Symbols the stream changes after each read goes out, so it refuses them.
    refuse: Vec<&'static str>,
    /// The key of each read out.
    out: Vec<&'static str>,
    /// The scope of each read the engine asked for.
    reads: Vec<Option<Vec<String>>>,
}

impl Venue for Source {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if std::mem::take(&mut self.startup_read) {
            self.read(cx);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        let keys = if self.keys.is_empty() {
            vec!["all"]
        } else {
            self.keys.clone()
        };
        let key = if self.out.is_empty() {
            keys[0]
        } else {
            self.out.remove(0)
        };
        for symbol in &self.refuse {
            cx.store.upsert_order(row(symbol));
        }
        let scope = OrderReadScope::Account {
            key,
            account: &keys,
        };
        let rows = cx.store.orders().to_vec();
        cx.commit_orders_read(scope, |_| true, rows, &[]);
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn refreshes_orders(&self) -> bool {
        self.refreshes
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, ()>, symbols: Option<&[String]>) {
        self.reads.push(symbols.map(<[String]>::to_vec));
        let keys = if self.keys.is_empty() {
            vec!["all"]
        } else {
            self.keys.clone()
        };
        for key in keys {
            self.out.push(key);
            self.read(cx);
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

impl Source {
    fn read(&mut self, cx: &mut Cx<'_, ()>) {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .expect("background read");
        let _ = cx.request_managed(
            context,
            None,
            Method::Get,
            "https://source.test/orders".into(),
            None,
            (),
        );
    }
}

/// A resting order on `symbol`, as the stream would insert it.
fn row(symbol: &str) -> crate::model::Order {
    crate::model::Order {
        id: format!("{symbol}-streamed"),
        parent_id: None,
        status: crate::model::OrderStatus::Open,
        symbol: symbol.into(),
        kind: crate::model::OrderType::Limit,
        side: crate::model::OrderSide::Buy,
        price: 1.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

struct Harness {
    engine: Engine<Source>,
    requests: Vec<RequestId>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<u32>,
}

impl Harness {
    fn new(refreshes: bool) -> Self {
        Self::with(Source {
            refreshes,
            ..Source::default()
        })
    }

    fn with_startup_read() -> Self {
        Self::with(Source {
            refreshes: true,
            startup_read: true,
            ..Source::default()
        })
    }

    fn with(source: Source) -> Self {
        let mut engine = crate::test_support::seeded_engine(source);
        engine.start(0.);
        let mut harness = Self {
            engine,
            requests: Vec::new(),
            timers: Vec::new(),
            done: Vec::new(),
        };
        harness.collect(0.);
        harness
    }

    fn collect(&mut self, now: f64) {
        for action in crate::test_support::prepared_actions(&mut self.engine, now) {
            match action {
                Action::Http(request) => self.requests.push(request.id),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                Action::Notice(crate::Notice::CommandDone { command, .. }) => {
                    self.done.push(command)
                }
                _ => {}
            }
        }
    }

    fn ask(&mut self, now: f64, command: u32, symbols: Option<&[&str]>) {
        let symbols = symbols.map(|s| s.iter().map(|&s| s.to_owned()).collect());
        self.engine.handle(
            now,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::RefreshOrders { symbols },
            }),
        );
        self.collect(now);
    }

    fn answer(&mut self, now: f64, request: RequestId) {
        self.engine.handle(
            now,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[]",
                },
            },
        );
        self.collect(now);
    }

    /// Fire every timer due by `until`, in order.
    fn advance(&mut self, until: f64) {
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first().filter(|(_, at)| *at <= until) else {
                return;
            };
            self.timers.remove(0);
            self.engine.handle(at, Event::TimerFired { timer });
            self.collect(at);
        }
    }

    fn reads(&self) -> &[Option<Vec<String>>] {
        &self.engine.venue.reads
    }
}

#[test]
fn an_ask_starts_one_read_and_answers_at_once() {
    let mut h = Harness::new(true);
    h.ask(1., 7, Some(&["BTCUSDT"]));
    assert_eq!(h.reads(), [Some(vec!["BTCUSDT".to_owned()])]);
    assert_eq!(h.requests.len(), 1);
    assert_eq!(h.done, [7], "the ask itself answers at once");
}

/// H7: asks while a read is out start nothing; when it lands, one follow-up
/// covers every ask it could not answer, with their symbols merged.
#[test]
fn asks_during_a_read_merge_into_one_follow_up() {
    let mut h = Harness::new(true);
    h.ask(1., 7, Some(&["BTCUSDT"]));
    h.ask(2., 8, Some(&["ETHUSDT"]));
    h.ask(3., 9, Some(&["BTCUSDT"]));
    assert_eq!(h.requests.len(), 1, "H7: no read while one is out");
    let first = h.requests[0];
    h.answer(4., first);
    assert_eq!(
        h.reads()[1..],
        [Some(vec!["ETHUSDT".to_owned(), "BTCUSDT".to_owned()])]
    );
    assert_eq!(h.requests.len(), 2, "exactly one follow-up");
    let follow_up = h.requests[1];
    h.answer(5., follow_up);
    assert_eq!(h.requests.len(), 2, "nothing more is owed");
}

#[test]
fn an_account_wide_ask_absorbs_symbol_asks() {
    let mut h = Harness::new(true);
    h.ask(1., 7, Some(&["BTCUSDT"]));
    h.ask(2., 8, None);
    h.ask(3., 9, Some(&["ETHUSDT"]));
    let first = h.requests[0];
    h.answer(4., first);
    assert_eq!(h.reads()[1..], [None]);
}

/// H3: a read released before an ask can never answer it.
#[test]
fn a_read_released_before_the_ask_cannot_answer_it() {
    let mut h = Harness::with_startup_read();
    let startup = h.requests[0];
    h.ask(1., 7, None);
    assert_eq!(h.requests.len(), 2, "the ask's own read");
    let asked = h.requests[1];
    h.answer(2., startup);
    h.ask(3., 8, None);
    assert_eq!(
        h.requests.len(),
        2,
        "the startup read did not answer ask 7, so its read is still the one out"
    );
    // The startup read committed after the ask's read went out, so the scope
    // refuses that read whole: it answers nothing, and one follow-up for both
    // asks waits its 2 s from that read's send.
    h.answer(4., asked);
    assert_eq!(h.requests.len(), 2, "no follow-up straight away");
    h.advance(1. + crate::frame_fed::REFRESH_FOLLOW_UP_MS);
    assert_eq!(h.requests.len(), 3, "one follow-up, at its time");
    let follow_up = h.requests[2];
    h.answer(3_100., follow_up);
    assert_eq!(h.requests.len(), 3, "it answers both asks");
}

/// An account read lands only once every scope key has: one key alone frees
/// no slot, so a new ask starts nothing until the others land (H7).
#[test]
fn an_account_read_frees_its_slot_only_when_every_key_lands() {
    let mut h = Harness::with(Source {
        refreshes: true,
        keys: vec!["linear", "inverse"],
        ..Source::default()
    });
    h.ask(1., 7, None);
    assert_eq!(h.requests.len(), 2, "one read per key");
    let (linear, inverse) = (h.requests[0], h.requests[1]);
    h.answer(2., linear);
    h.ask(3., 8, None);
    assert_eq!(h.requests.len(), 2, "inverse is still out");
    h.answer(4., inverse);
    assert_eq!(h.requests.len(), 4, "now ask 8's follow-up");
}

/// A read that refuses an asked symbol keeps demand for it, read again no
/// sooner than 2 s after it went out; one it answered asks nothing more.
#[test]
fn a_refused_symbol_is_asked_again_after_its_wait() {
    let mut h = Harness::with(Source {
        refreshes: true,
        refuse: vec!["BTCUSDT"],
        ..Source::default()
    });
    h.ask(1., 7, Some(&["BTCUSDT", "ETHUSDT"]));
    let first = h.requests[0];
    h.answer(2., first);
    assert_eq!(h.requests.len(), 1, "the follow-up waits");
    h.advance(1. + crate::frame_fed::REFRESH_FOLLOW_UP_MS - 1.);
    assert_eq!(h.requests.len(), 1);
    h.advance(1. + crate::frame_fed::REFRESH_FOLLOW_UP_MS);
    assert_eq!(
        h.reads()[1],
        Some(vec!["BTCUSDT".to_owned()]),
        "only what was refused"
    );
    h.engine.venue.refuse.clear();
    let second = h.requests[1];
    h.answer(3_100., second);
    h.advance(10_000.);
    assert_eq!(h.requests.len(), 2, "answered: nothing more");
}

/// A read that never lands frees its slot at its deadline and is retried
/// once; after that only a new ask starts another.
#[test]
fn a_lost_refresh_read_is_retried_once() {
    let mut h = Harness::new(true);
    h.ask(1., 7, Some(&["BTCUSDT"]));
    h.advance(1. + crate::engine::ORDERS_REFRESH_DEADLINE_MS);
    assert_eq!(h.requests.len(), 2, "one retry");
    assert_eq!(
        h.reads()[1],
        Some(vec!["BTCUSDT".to_owned()]),
        "the same symbols"
    );
    h.advance(1. + 3. * crate::engine::ORDERS_REFRESH_DEADLINE_MS);
    assert_eq!(h.requests.len(), 2, "retried once only");
    h.ask(200_000., 8, None);
    assert_eq!(h.requests.len(), 3, "a new ask starts again");
}

#[test]
fn a_source_that_cannot_refresh_answers_and_reads_nothing() {
    let mut h = Harness::new(false);
    h.ask(1., 7, None);
    assert!(h.requests.is_empty());
    assert_eq!(h.done, [7]);
}
