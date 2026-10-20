//! Engine-owned counterparts of Axios' xhr admission and shared weighted pacer.
//! IP/account aggregation across multiple engines belongs to the host; no global
//! state or wall-clock epoch participates in this sans-I/O dispatcher.
use crate::{Hyperliquid, Pending};
use safe_cex_engine::{CommandId, Cx, Method, TimerId, json::ArrayAt};
use serde::Deserialize;
use std::collections::VecDeque;

enum Request {
    Info { body: String, at: Option<ArrayAt>, pending: Pending },
    Trade(CommandId),
}
#[derive(Default)]
pub(crate) struct Budget {
    admission: VecDeque<Request>,
    ready: VecDeque<Request>,
    used: u8,
    admission_timer: Option<TimerId>,
    weighted_timer: Option<TimerId>,
    pumping: bool,
}

fn info_weight(body: &str) -> f64 {
    #[derive(Deserialize)]
    struct Info<'a> {
        #[serde(borrow)]
        r#type: &'a str,
        #[serde(default, deserialize_with = "crate::wire::optional_number")]
        limit: Option<f64>,
    }
    let Ok(info) = serde_json::from_str::<Info<'_>>(body) else { return 20.0; };
    let base = match info.r#type {
        "userRole" => 60.0,
        "allMids" | "l2Book" | "orderStatus" | "clearinghouseState" | "spotClearinghouseState" | "exchangeStatus" => 2.0,
        _ => 20.0,
    };
    let limit = info.limit.filter(|n| *n > 0.0).unwrap_or(0.0);
    base + match info.r#type {
        "candleSnapshot" => (limit / 60.0).floor(),
        "historicalOrders" | "userFills" | "userFillsByTime" | "fundingHistory" | "userFunding" |
        "userNonFundingLedgerUpdates" | "recentTrades" | "twapHistory" | "userTwapSliceFills" |
        "userTwapSliceFillsByTime" | "delegatorHistory" | "delegatorRewards" | "validatorStats" => (limit / 20.0).floor(),
        _ => 0.0,
    }
}

impl Hyperliquid {
    pub(crate) fn http_info(&mut self, cx: &mut Cx<'_, Pending>, body: String, at: Option<ArrayAt>, pending: Pending, limited: bool) {
        let request = Request::Info { body, at, pending };
        self.enqueue_http(cx, request, limited);
    }
    pub(crate) fn http_trade(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        self.enqueue_http(cx, Request::Trade(command), true);
    }
    fn enqueue_http(&mut self, cx: &mut Cx<'_, Pending>, request: Request, limited: bool) {
        if limited { self.http.admission.push_back(request); }
        else { self.http.ready.push_back(request); }
        self.pump_http(cx);
    }
    fn request_live(&mut self, request: &Request) -> bool {
        match request {
            Request::Trade(command) => self.trade_queued(*command),
            Request::Info { pending: Pending::Account(request), .. } => self.account_read_live(request),
            Request::Info { .. } => true,
        }
    }
    fn pump_http(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.http.pumping { return; }
        self.http.pumping = true;
        loop {
            while self.http.used < 3 {
                let Some(request) = self.http.admission.pop_front() else { break; };
                if !self.request_live(&request) { continue; }
                if self.http.used == 0 { self.http.admission_timer = Some(cx.set_timer(1000.0)); }
                self.http.used += 1;
                self.http.ready.push_back(request);
            }
            if self.http.weighted_timer.is_some() { break; }
            let Some(request) = self.http.ready.pop_front() else { break; };
            if !self.request_live(&request) { continue; }
            let weight = match request {
                Request::Info { body, at, pending } => {
                    let weight = info_weight(&body);
                    let id = cx.request(Method::Post, format!("{}/info", self.rest()),
                        vec![("Content-Type", "application/json".into())], Some(body), pending);
                    if let Some(at) = at { cx.stream_response(id, at); }
                    Some(weight)
                }
                Request::Trade(command) => self.dispatch_trade(cx, command),
            };
            // Even an otherwise idle queue retains the reservation. Responses
            // do not refund either stage. A delayed callback anchors the next
            // reservation at its actual dispatch time, like the Promise tail.
            if let Some(weight) = weight {
                self.http.weighted_timer = Some(cx.set_timer(weight * 50.0));
            }
        }
        self.http.pumping = false;
    }
    pub(crate) fn http_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.http.admission_timer == Some(timer) {
            self.http.admission_timer = None;
            self.http.used = 0;
            self.pump_http(cx);
        } else if self.http.weighted_timer == Some(timer) {
            self.http.weighted_timer = None;
            self.pump_http(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;
    use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketId, Venue, book::Book};
    struct Probe { venue: Hyperliquid, initial: Vec<(&'static str, bool)> }
    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            for (index, (body, limited)) in self.initial.drain(..).enumerate() {
                self.venue.http_info(cx, body.into(), Some(ArrayAt::Root), Pending::Candles { command: index as u32, candles: vec![], valid: true }, limited);
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) { self.venue.http_timer(cx, timer); }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
            self.venue.http_info(cx, "{\"type\":\"l2Book\",\"tag\":99}".into(), Some(ArrayAt::Root), Pending::Candles { command: 99, candles: vec![], valid: true }, false);
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) { self.venue.on_item(cx, pending, item); }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) { self.venue.on_response(cx, pending, response); }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) { self.venue.on_request_failed(cx, pending, error); }
        fn book(&self, _: &str) -> Option<&Book> { None }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> { Box::new(std::iter::empty()) }
    }
    fn engine(initial: Vec<(&'static str, bool)>) -> Engine<Probe> {
        Engine::new(Probe { venue: Hyperliquid::new(Options::default()), initial })
    }
    fn outputs(engine: &mut Engine<Probe>) -> (Vec<HttpRequest>, Vec<(TimerId, f64)>) {
        let mut requests = vec![]; let mut timers = vec![];
        for action in engine.drain_actions() {
            match action { Action::Http(r) => requests.push(r), Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)), _ => {} }
        }
        (requests, timers)
    }
    fn timer(timers: &[(TimerId, f64)], at: f64) -> TimerId { timers.iter().find(|(_, time)| *time == at).unwrap().0 }
    #[test]
    fn weights_match_outgoing_top_level_limits_and_never_nested_or_response_rows() {
        for (body, weight) in [
            (r#"{"type":"userRole"}"#,60.), (r#"{"type":"clearinghouseState"}"#,2.),
            (r#"{"type":"spotClearinghouseState"}"#,2.), (r#"{"type":"orderStatus"}"#,2.),
            (r#"{"type":"metaAndAssetCtxs"}"#,20.), (r#"{"type":"perpDexs"}"#,20.),
            (r#"{"type":"historicalOrders","limit":41}"#,22.),
            (r#"{"type":"candleSnapshot","limit":"120"}"#,22.),
            (r#"{"type":"candleSnapshot","req":{"limit":120}}"#,20.),
            (r#"{"type":"metaAndAssetCtxs","limit":120}"#,20.),
            (r#"{"type":"userFills","limit":-20}"#,20.),
        ] { assert_eq!(info_weight(body),weight, "{body}"); }
    }
    #[test]
    fn xhr_admission_precedes_weighted_fifo_and_bypass_does_not_jump_admitted_rows() {
        let mut e = engine(vec![(r#"{"type":"l2Book","tag":1}"#,true),(r#"{"type":"l2Book","tag":2}"#,true),(r#"{"type":"l2Book","tag":3}"#,true),(r#"{"type":"l2Book","tag":4}"#,true),(r#"{"type":"l2Book","tag":5}"#,false)]);
        e.start(0.);
        let (first,timers)=outputs(&mut e); assert_eq!(first.len(),1); assert!(first[0].body.as_ref().unwrap().contains(":1}"));
        let reset=timer(&timers,1000.); let mut next=timer(&timers,100.);
        // Failure never refunds the xhr slot or the weighted reservation.
        e.handle(1.,Event::HttpFailed { request:first[0].id, error:"offline" }); outputs(&mut e);
        for (at,tag) in [(100.,2),(200.,3),(300.,5)] {
            e.handle(at,Event::TimerFired { timer:next });
            let (requests,timers)=outputs(&mut e); assert_eq!(requests.len(),1);
            assert!(requests[0].body.as_ref().unwrap().contains(&format!(":{tag}}}")));
            next=timer(&timers,at+100.);
        }
        e.handle(400.,Event::TimerFired {timer:next}); assert!(outputs(&mut e).0.is_empty());
        e.handle(999.,Event::TimerFired {timer:next}); assert!(outputs(&mut e).0.is_empty(),"duplicate cooldown callback has no authority");
        e.handle(1000.,Event::TimerFired {timer:reset});
        let (requests,_)=outputs(&mut e); assert_eq!(requests.len(),1); assert!(requests[0].body.as_ref().unwrap().contains(":4}"));
    }
    #[test]
    fn idle_and_rebased_cooldown_wait_for_owned_callback_and_anchor_late_dispatch() {
        let mut e=engine(vec![(r#"{"type":"metaAndAssetCtxs"}"#,false)]); e.start(10_000.);
        let (_,timers)=outputs(&mut e);let cooldown=timer(&timers,11_000.);
        // Like startup-to-playback rebasing, absolute now can change while the
        // remaining timer delay is transported by the host. No clock comparison
        // may manufacture a fresh token before its owned callback fires.
        e.handle(100.,Event::Command(Command::FetchLeverage {command:1,symbol:"BTC".into()}));
        assert!(outputs(&mut e).0.is_empty());
        e.handle(1200.,Event::TimerFired {timer:cooldown});
        let (requests,timers)=outputs(&mut e);assert_eq!(requests.len(),1);assert_eq!(timers[0].1,1300.);
    }
    #[test]
    fn queued_stream_keeps_pending_identity_and_full_envelope_validation() {
        for (body,valid) in [(r#"[{"t":1,"o":"1","h":"2","l":"1","c":"2","v":"3"}]"#,true),
            (r#"[{"t":1,"o":"1","h":"2","l":"1","c":"2","v":"3"}] trailing"#,false)] {
            let mut e=engine(vec![(r#"{"type":"metaAndAssetCtxs"}"#,false),(r#"{"type":"candleSnapshot"}"#,true)]);
            e.start(0.);let (_,timers)=outputs(&mut e); e.handle(1000.,Event::TimerFired{timer:timer(&timers,1000.)});
            // Admission and weighted timers can share a deadline; only the owned
            // weighted callback starts the queued streamed request.
            let (mut requests,more)=outputs(&mut e);
            if requests.is_empty() { for (id,at) in timers { if at==1000. { e.handle(at,Event::TimerFired{timer:id}); } } let out=outputs(&mut e);requests=out.0; }
            assert_eq!(requests.len(),1);let r=requests.pop().unwrap();
            e.handle(1001.,Event::HttpResponse{request:r.id,response:Response{status:200,headers:"",body}});
            assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:1,result}) if result.is_ok()==valid)));
            let _=more;
        }
    }
}
