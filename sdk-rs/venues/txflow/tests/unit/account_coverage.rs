//! Native exhaustion branches, with real engine event delivery and synthetic
//! accepted continuations. This does not claim streamed-response admission proof.
use super::*;
use crate::Options;
use safe_cex_engine::{Action, Command, Notice, SocketId, Value, Venue};

#[derive(Clone, Copy)]
enum Limit {
    Offset,
    Rows,
    Dedup,
}
struct Fixture {
    venue: TxFlow,
    limit: Limit,
}
impl Venue for Fixture {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut fills = Vec::new();
        let mut offset = 0;
        let has_more = !matches!(self.limit, Limit::Dedup);
        match self.limit {
            Limit::Offset => offset = 200_000,
            Limit::Rows => {
                for tid in 0..=FILL_CAP {
                    fills.push(execution(tid as u64));
                }
            }
            Limit::Dedup => {
                self.venue.account.seen_fills.extend(0..FILL_CAP as u64);
                fills.push(execution(FILL_CAP as u64));
            }
        }
        self.venue.account.inflight.insert("fills".into());
        let body = format!(r#"{{"success":true,"code":0,"data":{{"hasMore":{has_more}}}}}"#);
        self.venue.account_response(
            cx,
            Request::Fills {
                generation: self.venue.account.generation,
                offset,
                fills,
                ids: HashSet::new(),
                valid: true,
            },
            Response {
                status: 200,
                headers: "",
                body: &body,
            },
        );
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("fixture must not issue network requests");
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn execution(tid: u64) -> Execution {
    Execution {
        tid,
        oid: 1,
        instrument_id: 1,
        px: N(1.0),
        sz: N(1.0),
        side: "B".into(),
        time: N(1_000.0),
    }
}
fn assert_exhausted(limit: Limit, message: &str) {
    let venue = TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    });
    let mut engine = safe_cex_engine::test_support::seeded_engine(Fixture { venue, limit });
    engine.set_log_level(Level::Error);
    engine.start(1_000.0);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions.len(),
        1,
        "no fill, request, retry or command completion"
    );
    let Action::Notice(Notice::Event(event)) = &actions[0] else {
        panic!("coverage event")
    };
    assert_eq!(event.code, code::HISTORY_INCOMPLETE);
    assert_eq!(event.level, Level::Error);
    assert_eq!(event.message, message);
    assert_eq!(
        event.fields,
        vec![("scope", Value::Str("account/background".into()))]
    );
    assert!(engine.venue().venue.account.fills_exhausted);
    assert!(!engine.venue().venue.account.inflight.contains("fills"));
    assert!(engine.take_frame(1_000.0).is_none());
}
#[test]
fn offset_exhaustion_is_account_scoped_without_a_query_owner() {
    assert_exhausted(
        Limit::Offset,
        "TxFlow execution coverage exhausted; restart account reconciliation",
    );
}
#[test]
fn row_exhaustion_is_account_scoped_without_a_query_owner() {
    assert_exhausted(
        Limit::Rows,
        "TxFlow execution coverage exhausted; restart account reconciliation",
    );
}
#[test]
fn dedup_exhaustion_is_account_scoped_without_a_query_owner() {
    assert_exhausted(
        Limit::Dedup,
        "TxFlow execution deduplication capacity exhausted; restart account reconciliation",
    );
}
