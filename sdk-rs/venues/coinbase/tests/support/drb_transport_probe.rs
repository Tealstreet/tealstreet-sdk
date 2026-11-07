use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue, book::Book, test_support::ProbeVenue};
struct Noop;
impl Venue for Noop {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> { None }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> { Box::new(std::iter::empty()) }
}
fn run(now: f64, mut f: impl FnMut(&mut Cx<'_, ()>)) -> Vec<Action> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(ProbeVenue::new(Noop, |_: &mut Noop, cx: &mut Cx<'_, ()>| f(cx)));
    engine.start(now);
    safe_cex_engine::test_support::prepared_actions(&mut engine, now)
}
fn chart_context(cx: &mut Cx<'_, ()>) -> safe_cex_engine::request_context::Context {
    use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Scope};
    cx.http_context(Scope::Background, Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: safe_cex_venue_coinbase::drb::REQUEST_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }).unwrap()
}
