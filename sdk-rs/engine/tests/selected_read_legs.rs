//! NEW-CONTRACT specifications for selected original roots and physical page legs.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{PrimaryReason, SlotPolicy, Slots};
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, HttpAction, Owner, Prepared, RoutingIntent,
};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot,
    SelectedRead,
};
use safe_cex_engine::signing::{
    Identity, KeyRef, SignError, SignReceipt, SignRequest, SignResult, hmac_sha256_work,
};
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, Command, CredentialSlot, Cx, DispatchContext, Engine,
    ErrorKind, Event, HttpRequest, Method, Notice, RequestId, Response, SignStep, SocketId, Store,
    TimerId, Venue, VenueError,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
const NOW: f64 = 1_000.25;
const HEADERS: &[(&str, &str)] = &[("X-Demo", "1"), ("Content-Type", "application/json")];
fn identity(slot: &str) -> Identity {
    Identity::new(
        KeyRef(format!("ref:{slot}:credential")),
        Some(KeyRef(format!("ref:{slot}:signing"))),
    )
    .with_passphrase(Some(KeyRef(format!("ref:{slot}:passphrase"))))
    .with_stable_id(Some("account".into()))
}
struct Recipe {
    identity: Identity,
    invalid: bool,
}
impl SignStep for Recipe {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        if self.invalid {
            &[("Bad\nName", "value")]
        } else {
            HEADERS
        }
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &self.identity.credential,
            self.identity.signing_ref()?,
            Vec::new(),
            Default::default(),
            SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: None,
            },
        ))
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        request.headers.push((
            "X-Selected",
            String::from_utf8(result.values[0].clone()).unwrap(),
        ));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

#[derive(Default)]
struct Observed {
    roots: Vec<Context>,
    legs: Vec<Context>,
    identities: Vec<Identity>,
    entries: Vec<CommandEntryError>,
    expired: Vec<Result<(), CommandEntryError>>,
    local: Vec<Result<(), Error>>,
    queued_refusals: Vec<(Context, u8, Error)>,
    delivery: Vec<Result<(), Error>>,
    wrong: Vec<Result<(), Error>>,
    retired: Vec<(Owner, usize)>,
    failures: usize,
    staged: Vec<String>,
    finalizers: usize,
    sign_errors: Vec<VenueError>,
}
#[derive(Default)]
struct Controls {
    missing: bool,
    primary: bool,
    hold: bool,
    hold_cancel: bool,
    invalid: bool,
    stream: bool,
    delay: f64,
    from_start: bool,
    terminal: Option<bool>,
    foreign_leg: Option<ReadLeg>,
}
struct Walk {
    root: ReadRoot,
    history: ReadCursorHistory<String>,
    pages: usize,
}
struct Pending {
    leg: ReadLeg,
    command: u32,
    marker: u8,
    items: usize,
}
struct Probe {
    controls: Arc<Mutex<Controls>>,
    observed: Arc<Mutex<Observed>>,
    walks: HashMap<u32, Walk>,
    wake: HashMap<TimerId, u32>,
    old: Vec<ReadLeg>,
}
fn encode(rows: &[String]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string()))
}
impl Probe {
    fn admit(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        let controls = self.controls.lock().unwrap();
        let slots = Slots::new(
            (!controls.missing).then(|| identity("primary")),
            (!controls.missing).then(|| identity("secondary")),
            SlotPolicy::default(),
        );
        let role = if controls.primary {
            ReadRole::Primary {
                reason: PrimaryReason::DegradedRead,
                scope: Some(AccountReadScope::Balance),
            }
        } else {
            ReadRole::Read {
                scope: Some(AccountReadScope::Balance),
            }
        };
        let selected = SelectedRead::select(&slots, role);
        let hold = controls.hold;
        drop(controls);
        let root = match SelectedRead::command_root(
            cx,
            selected,
            LocalRefusal {
                diagnostic: Some("native missing key".into()),
                error: VenueError::new("credential missing").kind(ErrorKind::AuthInvalid),
            },
        ) {
            Ok(root) => root,
            Err(CommandReadError::Entry(e)) => {
                self.observed.lock().unwrap().entries.push(e);
                if e == CommandEntryError::Revoked
                    && cx.command_wait() == Some(safe_cex_engine::CommandWait::Expired)
                {
                    let result = cx
                        .reject_expired_command(safe_cex_engine::request_context::Operation::Read);
                    self.observed.lock().unwrap().expired.push(result);
                }
                return;
            }
            Err(CommandReadError::LocalRefusal { terminal, .. }) => {
                self.observed.lock().unwrap().local.push(terminal);
                return;
            }
        };
        self.observed
            .lock()
            .unwrap()
            .roots
            .push(root.context().clone());
        let completion = ReadCompletion::from_context(cx, command, root.context().clone()).unwrap();
        self.walks.insert(
            command,
            Walk {
                root,
                history: ReadCursorHistory::new(
                    completion,
                    CursorWalk::new(10),
                    |_| "stall".into(),
                    encode,
                ),
                pages: 0,
            },
        );
        if !hold {
            self.queue(cx, command);
            if self.controls.lock().unwrap().terminal.is_some() {
                self.park(cx, command);
            }
        }
    }
    fn queue(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        let Some(walk) = self.walks.get(&command) else {
            return;
        };
        let leg = match walk.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(e) => {
                self.observed.lock().unwrap().queued_refusals.push((
                    walk.root.context().clone(),
                    73,
                    e,
                ));
                return;
            }
        };
        self.observed
            .lock()
            .unwrap()
            .legs
            .push(leg.context().clone());
        let observed = self.observed.clone();
        let controls = self.controls.lock().unwrap();
        let invalid = controls.invalid;
        let stream = controls.stream;
        drop(controls);
        let result = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url: "https://fixture/read?page=1".into(),
                body: Some("native body".into()),
                stream: stream.then_some(safe_cex_engine::json::ArrayAt::Field("rows")),
            },
            move |identity| {
                observed.lock().unwrap().identities.push(identity.clone());
                Box::new(Recipe {
                    identity: identity.clone(),
                    invalid,
                })
            },
            |leg| Pending {
                leg,
                command,
                marker: 73,
                items: 0,
            },
        );
        if let Err((p, error)) = result {
            self.observed.lock().unwrap().queued_refusals.push((
                p.leg.context().clone(),
                p.marker,
                error,
            ));
            self.controls.lock().unwrap().foreign_leg = Some(p.leg);
        }
    }
    fn park(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        let delay = self.controls.lock().unwrap().delay;
        let timer = cx.set_timer(delay);
        self.wake.insert(timer, command);
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.controls.lock().unwrap().from_start {
            self.admit(cx, 7);
        }
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.controls.lock().unwrap().hold_cancel
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchAccount { command } | Command::CancelOrders { command, .. } => {
                self.admit(cx, command)
            }
            // This legacy fixture command only creates a native placement-ack hold.
            Command::PlaceOrders { command, .. } => cx.done(command, "[\"7\"]".into()),
            _ => {}
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        let walk = self.walks.get_mut(&pending.command).unwrap();
        if walk.root.validate_delivery(cx, &pending.leg).is_ok() {
            let Ok(mut page) = walk.history.admit(cx, pending.leg.context()) else {
                return;
            };
            page.stage(item.into());
            pending.items += 1;
            self.observed.lock().unwrap().staged.push(item.into());
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, response: Response<'_>) {
        let valid = self.walks[&p.command].root.validate_delivery(cx, &p.leg);
        self.observed.lock().unwrap().delivery.push(valid);
        if valid.is_err() {
            return;
        }
        for old in &self.old {
            self.observed
                .lock()
                .unwrap()
                .wrong
                .push(self.walks[&p.command].root.validate_delivery(cx, old));
        }
        for (&id, other) in &self.walks {
            if id != p.command {
                self.observed
                    .lock()
                    .unwrap()
                    .wrong
                    .push(other.root.validate_delivery(cx, &p.leg));
            }
        }
        if let Some(foreign) = &self.controls.lock().unwrap().foreign_leg {
            self.observed
                .lock()
                .unwrap()
                .wrong
                .push(self.walks[&p.command].root.validate_delivery(cx, foreign));
        }
        let discard = response.body == "discard";
        let finish = response.body == "done" || p.items > 0;
        let walk = self.walks.get_mut(&p.command).unwrap();
        if discard {
            assert!(walk.history.discard_staged_page());
        } else {
            walk.pages += 1;
            let mut page = walk.history.admit(cx, p.leg.context()).unwrap();
            if p.items == 0 {
                page.stage(response.body.into());
            }
            let step = page.advance(CursorPage {
                next: Some(format!("page{}", walk.pages)),
                rows: p.items.max(1),
                fresh: p.items.max(1),
                want_more: !finish,
            });
            assert_eq!(matches!(step, CursorStep::More(_)), !finish);
        }
        cx.finish_http_context(p.leg.context()).unwrap();
        self.old.push(p.leg);
        if finish {
            let walk = self.walks.remove(&p.command).unwrap();
            let observed = self.observed.clone();
            walk.history
                .finish_with(cx, Ok(()), move |rows| {
                    observed.lock().unwrap().finalizers += 1;
                    encode(&rows)
                })
                .unwrap();
        } else {
            self.park(cx, p.command);
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        self.observed.lock().unwrap().failures += 1;
        let walk = self.walks.remove(&p.command).unwrap();
        let observed = self.observed.clone();
        walk.history
            .finish_with(cx, Err(VenueError::new(error)), move |rows| {
                observed.lock().unwrap().finalizers += 1;
                encode(&rows)
            })
            .unwrap();
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
        self.observed
            .lock()
            .unwrap()
            .sign_errors
            .push(error.clone());
        let walk = self.walks.remove(&p.command).unwrap();
        cx.fail_context(walk.root.context(), error).unwrap();
    }
    fn account_auth_outcome(
        &self,
        _: &Pending,
        _: &Response<'_>,
        _: f64,
        _: &Store,
    ) -> Option<AuthOutcome> {
        Some(AuthOutcome::Accepted)
    }
    fn read_root_retired(&mut self, owner: Owner, pending: Vec<Pending>) {
        self.observed
            .lock()
            .unwrap()
            .retired
            .push((owner, pending.len()));
        self.walks.retain(|_, w| w.root.context().owner() != owner);
        self.wake.retain(|_, id| self.walks.contains_key(id));
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(id) = self.wake.remove(&timer) {
            let terminal = self.controls.lock().unwrap().terminal;
            if let Some(success) = terminal {
                let walk = self.walks.remove(&id).unwrap();
                if success {
                    cx.done_context(walk.root.context(), "native local done".into())
                        .unwrap();
                } else {
                    cx.fail_context(walk.root.context(), VenueError::new("native local fail"))
                        .unwrap();
                }
            } else {
                self.queue(cx, id);
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(c: Controls) -> (Engine<Probe>, Arc<Mutex<Observed>>, Arc<Mutex<Controls>>) {
    let observed = Arc::new(Mutex::new(Observed::default()));
    let controls = Arc::new(Mutex::new(c));
    let e = support::seeded_engine(Probe {
        controls: controls.clone(),
        observed: observed.clone(),
        walks: HashMap::new(),
        wake: HashMap::new(),
        old: Vec::new(),
    });
    (e, observed, controls)
}
fn ask(e: &mut Engine<Probe>, id: u32, wait: u32) -> Vec<Action> {
    e.handle_command(NOW, Command::FetchAccount { command: id }, Some(wait))
        .unwrap();
    e.drain_actions().collect()
}
fn preparation(
    actions: Vec<Action>,
) -> (
    safe_cex_engine::request_context::Preparation,
    HttpRequest,
    RoutingIntent,
) {
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => Some((preparation, request, intent)),
                _ => None,
            },
            _ => None,
        })
        .expect("managed Prepare")
}
fn release(e: &mut Engine<Probe>, actions: Vec<Action>, now: f64) -> (HttpRequest, TimerId) {
    let deadline = e
        .venue()
        .observed
        .lock()
        .unwrap()
        .legs
        .last()
        .unwrap()
        .deadline();
    let timer = actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
            _ => None,
        })
        .expect("physical timer emitted at queue");
    let (preparation, _, _) = preparation(actions);
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let (attempt, work) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    assert!(
        work.operations
            .iter()
            .any(|op| matches!(op, safe_cex_engine::signing::Operation::Sign { .. }))
    );
    e.handle(
        now,
        Event::Signed {
            attempt,
            result: Ok(SignResult {
                values: vec![b"remote-key".to_vec(), vec![7; 32]],
                receipt: work.receipt,
                headers: Vec::new(),
            }),
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let (permit, request) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(e.claim_http(now, permit).is_ok());
    (request, timer)
}
fn response(e: &mut Engine<Probe>, request: RequestId, now: f64, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn wake(e: &mut Engine<Probe>, actions: Vec<Action>, now: f64) -> Vec<Action> {
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .expect("native eligible timer");
    e.handle(now, Event::TimerFired { timer });
    e.drain_actions().collect()
}
fn failed(actions: &[Action]) -> Vec<&VenueError> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}

#[test]
fn new_contract_pages_derive_from_original_fractional_root_and_keep_selected_snapshot() {
    let (mut e, o, c) = engine(Controls {
        delay: 15_000.,
        ..Controls::default()
    });
    let first = ask(&mut e, 7, 120_000);
    let (request, _) = release(&mut e, first, NOW + 1.);
    let actions = response(&mut e, request.id, NOW + 2., "page1");
    c.lock().unwrap().primary = true;
    let second = wake(&mut e, actions, NOW + 15_002.);
    let (_, request, intent) = preparation(second);
    assert_eq!(intent, RoutingIntent::Secondary);
    assert_eq!(request.body.as_deref(), Some("native body"));
    assert_eq!(
        request.headers,
        HEADERS
            .iter()
            .map(|&(k, v)| (k, v.into()))
            .collect::<Vec<_>>()
    );
    let s = o.lock().unwrap();
    assert_eq!(s.roots[0].deadline(), NOW + 120_000.);
    assert_eq!(s.legs[0].deadline(), NOW + 20_000.);
    assert_eq!(s.legs[1].deadline(), NOW + 35_002.);
    assert_ne!(s.legs[0].leg(), s.legs[1].leg());
    assert_eq!(s.legs[0].owner(), s.legs[1].owner());
    assert_eq!(
        s.identities,
        vec![identity("secondary"), identity("secondary")]
    );
}
#[test]
fn new_contract_late_page_is_capped_without_rounding_and_root_expires_before_bind() {
    let (mut e, o, _) = engine(Controls {
        delay: 14_998.5,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 20_000);
    let (req, _) = release(&mut e, a, NOW + 1.);
    let a = response(&mut e, req.id, NOW + 2., "page1");
    let a = wake(&mut e, a, NOW + 15_000.5);
    let (prep, request, _) = preparation(a);
    assert_eq!(request.timeout_ms, Some(5_000));
    assert_eq!(o.lock().unwrap().legs[1].deadline(), NOW + 20_000.);
    e.http_prepared(
        NOW + 20_000.,
        prep,
        Prepared {
            id: 2,
            expires_at: None,
        },
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failed(&a).len(), 1);
    assert_eq!(failed(&a)[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(failed(&a)[0].outcome, None);
    let s = o.lock().unwrap();
    assert_eq!(s.finalizers, 0);
    assert_eq!(s.retired.len(), 1);
    assert_eq!(s.retired[0].1, 1);
}
#[test]
fn new_contract_discard_requeue_retires_old_delivery_without_changing_snapshot_or_root() {
    let (mut e, o, c) = engine(Controls {
        delay: 250.,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 120_000);
    let (req, _) = release(&mut e, a, NOW + 1.);
    let a = response(&mut e, req.id, NOW + 2., "discard");
    c.lock().unwrap().primary = true;
    let a = wake(&mut e, a, NOW + 252.);
    let (next, _) = release(&mut e, a, NOW + 253.);
    assert!(response(&mut e, req.id, NOW + 254., "late old rows").is_empty());
    let a = response(&mut e, next.id, NOW + 255., "done");
    assert!(a.iter().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{result,..}) if result=="[\"done\"]")
    ));
    let s = o.lock().unwrap();
    assert_eq!(s.wrong, vec![Err(Error::Revoked)]);
    assert_eq!(
        s.identities,
        vec![identity("secondary"), identity("secondary")]
    );
    assert_eq!(s.roots.len(), 1);
    assert_eq!(s.finalizers, 1);
}
#[test]
fn new_contract_wrong_live_root_cannot_admit_another_roots_page() {
    let (mut e, o, _) = engine(Controls::default());
    let a = ask(&mut e, 7, 120_000);
    let (req, _) = release(&mut e, a, NOW + 1.);
    let _ = ask(&mut e, 8, 120_000);
    response(&mut e, req.id, NOW + 2., "done");
    assert_eq!(o.lock().unwrap().wrong, vec![Err(Error::Revoked)]);
}
#[test]
fn new_contract_child_timeout_recovers_only_accepted_prefix_while_root_is_live() {
    let (mut e, o, _) = engine(Controls {
        delay: 250.,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 120_000);
    let (req, _) = release(&mut e, a, NOW + 1.);
    let a = response(&mut e, req.id, NOW + 2., "page1");
    let a = wake(&mut e, a, NOW + 252.);
    let (_, timer) = release(&mut e, a, NOW + 253.);
    e.handle(NOW + 20_252., Event::TimerFired { timer });
    let a = e.drain_actions().collect::<Vec<_>>();
    assert!(failed(&a).is_empty());
    assert!(a.iter().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{result,..}) if result=="[\"page1\"]")
    ));
    let s = o.lock().unwrap();
    assert_eq!(s.failures, 1);
    assert_eq!(s.finalizers, 1);
    assert!(s.retired.is_empty());
}
#[test]
fn new_contract_root_timeout_discards_staged_stream_and_never_recovers_prefix() {
    let (mut e, o, c) = engine(Controls {
        delay: 250.,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 1_000);
    let (req, _) = release(&mut e, a, NOW + 1.);
    let a = response(&mut e, req.id, NOW + 2., "page1");
    c.lock().unwrap().stream = true;
    let a = wake(&mut e, a, NOW + 252.);
    let (req, _) = release(&mut e, a, NOW + 253.);
    e.handle(
        NOW + 254.,
        Event::HttpStart {
            request: req.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 254.,
        Event::HttpChunk {
            request: req.id,
            data: br#"{"rows":[{"id":9},"#,
        },
    );
    e.drain_actions().for_each(drop);
    assert_eq!(o.lock().unwrap().staged, vec!["{\"id\":9}"]);
    e.handle(NOW + 1_000., Event::PollScaleChanged { scale: 1. });
    let a = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failed(&a).len(), 1);
    assert_eq!(failed(&a)[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(o.lock().unwrap().finalizers, 0);
    assert_eq!(o.lock().unwrap().failures, 0);
    e.handle(
        NOW + 1_001.,
        Event::HttpChunk {
            request: req.id,
            data: b"{\"id\":10}]}",
        },
    );
    e.handle(NOW + 1_001., Event::HttpEnd { request: req.id });
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    assert_eq!(o.lock().unwrap().staged.len(), 1);
}
#[test]
fn new_contract_queue_refusal_preserves_exact_leg_pending_and_error_without_root_terminal() {
    let (mut e, o, _) = engine(Controls {
        invalid: true,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 120_000);
    assert!(failed(&a).is_empty());
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
    );
    let s = o.lock().unwrap();
    assert_eq!(
        s.queued_refusals,
        vec![(s.legs[0].clone(), 73, Error::Invalid)]
    );
    assert_eq!(s.retired.len(), 0);
}
#[test]
fn new_contract_missing_selection_clears_tracked_root_then_diagnostic_and_one_failure() {
    let (mut e, o, _) = engine(Controls {
        missing: true,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 120_000);
    assert_eq!(failed(&a).len(), 1);
    assert_eq!(failed(&a)[0].kind, Some(ErrorKind::AuthInvalid));
    assert!(a.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(a.iter().any(|a| matches!(a, Action::ClearTimer { .. })));
    let n = a
        .iter()
        .filter(|a| matches!(a, Action::Notice(_)))
        .collect::<Vec<_>>();
    assert!(matches!(n[0], Action::Notice(Notice::CommandError { .. })));
    assert!(matches!(n[1], Action::Notice(Notice::CommandFailed { .. })));
    assert_eq!(o.lock().unwrap().local, vec![Ok(())]);
    e.handle(NOW + 120_000., Event::PollScaleChanged { scale: 1. });
    assert!(failed(&e.drain_actions().collect::<Vec<_>>()).is_empty());
}
#[test]
fn new_contract_capacity_precedes_native_missing_selection_refusal() {
    let (mut e, o, c) = engine(Controls {
        hold: true,
        ..Controls::default()
    });
    for id in 1..=256 {
        ask(&mut e, id, 120_000);
    }
    c.lock().unwrap().missing = true;
    let a = ask(&mut e, 257, 120_000);
    let mut expected = Error::Capacity.venue_error();
    expected.outcome = None;
    assert_eq!(failed(&a), vec![&expected]);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandError { .. })))
    );
    assert_eq!(
        o.lock().unwrap().entries,
        vec![CommandEntryError::Rejected(Error::Capacity)]
    );
}
#[test]
fn new_contract_borrowed_startup_and_duplicate_entry_never_issue_or_replace_root() {
    let (mut e, o, _) = engine(Controls {
        from_start: true,
        hold: true,
        ..Controls::default()
    });
    e.start(NOW);
    assert!(e.drain_actions().next().is_none());
    assert_eq!(o.lock().unwrap().entries, vec![CommandEntryError::Revoked]);
    ask(&mut e, 7, 120_000);
    let a = ask(&mut e, 7, 120_000);
    assert!(failed(&a).is_empty());
    assert_eq!(o.lock().unwrap().roots.len(), 1);
}

#[test]
fn new_contract_foreign_engine_equal_numeric_ids_cannot_exchange_legs() {
    let (mut a, oa, ca) = engine(Controls {
        invalid: true,
        ..Controls::default()
    });
    ask(&mut a, 7, 120_000);
    let foreign = ca.lock().unwrap().foreign_leg.take().unwrap();
    let (mut b, ob, cb) = engine(Controls::default());
    let actions = ask(&mut b, 7, 120_000);
    assert_eq!(
        oa.lock().unwrap().legs[0].owner(),
        ob.lock().unwrap().legs[0].owner()
    );
    assert_eq!(foreign.context().leg(), ob.lock().unwrap().legs[0].leg());
    cb.lock().unwrap().foreign_leg = Some(foreign);
    let (request, _) = release(&mut b, actions, NOW + 1.);
    response(&mut b, request.id, NOW + 2., "done");
    assert_eq!(ob.lock().unwrap().wrong, vec![Err(Error::Revoked)]);
}

#[test]
fn new_contract_provider_refusal_keeps_original_error_and_root_only_terminal() {
    let (mut e, o, _) = engine(Controls::default());
    let (prep, _, _) = preparation(ask(&mut e, 7, 120_000));
    e.http_prepared(
        NOW + 1.,
        prep,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let attempt = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, .. } => Some(attempt),
            _ => None,
        })
        .unwrap();
    e.handle(
        NOW + 2.,
        Event::Signed {
            attempt,
            result: Err(SignError::Unavailable),
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failed(&actions).len(), 1);
    assert_eq!(failed(&actions)[0], &o.lock().unwrap().sign_errors[0]);
    assert!(
        !actions.iter().any(
            |a| matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Bind{..}))
        )
    );
    assert_eq!(o.lock().unwrap().finalizers, 0);
}

#[test]
fn new_contract_root_done_or_fail_revokes_preparing_child_and_late_work() {
    for success in [true, false] {
        let (mut e, _, _) = engine(Controls {
            terminal: Some(success),
            delay: 1.,
            ..Controls::default()
        });
        let actions = ask(&mut e, 7, 120_000);
        let root_timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1. => Some(*timer),
                _ => None,
            })
            .unwrap();
        let (prep, request, _) = preparation(actions);
        e.handle(NOW + 1., Event::TimerFired { timer: root_timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
                ))
                .count(),
            1
        );
        assert_eq!(failed(&actions).len(), usize::from(!success));
        e.http_prepared(
            NOW + 2.,
            prep,
            Prepared {
                id: 1,
                expires_at: None,
            },
        );
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::Sign { .. } | Action::ManagedHttp(_)))
        );
        assert!(response(&mut e, request.id, NOW + 3., "late rows").is_empty());
    }
}

#[test]
fn new_contract_streamed_page_preserves_raw_order_and_selected_evidence_on_root_completion() {
    let (mut e, o, _) = engine(Controls {
        stream: true,
        ..Controls::default()
    });
    let a = ask(&mut e, 7, 120_000);
    let (request, _) = release(&mut e, a, NOW + 1.);
    e.handle(
        NOW + 2.,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: request.id,
            data: br#"{"rows":[{"id":2},{"id":1}]}"#,
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let rows = vec!["{\"id\":2}".to_string(), "{\"id\":1}".to_string()];
    assert_eq!(o.lock().unwrap().staged, rows);
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{result,..}) if result==&encode(&rows).unwrap())));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::AccountAuth {
            credential: CredentialSlot::Secondary,
            scope: AccountReadScope::Balance,
            outcome: AuthOutcome::Accepted
        })
    )));
    assert_eq!(o.lock().unwrap().finalizers, 1);
}

#[test]
fn new_contract_expired_constructor_preserves_fresh_entry_for_shared_rejection() {
    let (mut e, o, _) = engine(Controls {
        hold_cancel: true,
        ..Controls::default()
    });
    e.handle_command(
        NOW,
        Command::PlaceOrders {
            command: 1,
            orders: Vec::new(),
        },
        None,
    )
    .unwrap();
    e.handle_command(
        NOW,
        Command::CancelOrders {
            command: 2,
            ids: vec!["7".into()],
        },
        Some(1_000),
    )
    .unwrap();
    assert!(o.lock().unwrap().entries.is_empty());
    e.drain_actions().for_each(drop);
    e.store_mut().upsert_order(safe_cex_engine::model::Order {
        id: "7".into(),
        parent_id: None,
        status: safe_cex_engine::model::OrderStatus::Open,
        symbol: "BTCUSD".into(),
        kind: safe_cex_engine::model::OrderType::Limit,
        side: safe_cex_engine::model::OrderSide::Buy,
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
    });
    e.handle(NOW + 999.5, Event::PollScaleChanged { scale: 1. });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(o.lock().unwrap().entries, vec![CommandEntryError::Revoked]);
    assert_eq!(o.lock().unwrap().expired, vec![Ok(())]);
    assert!(o.lock().unwrap().roots.is_empty());
    assert_eq!(failed(&actions).len(), 1);
    assert_eq!(failed(&actions)[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(failed(&actions)[0].outcome, None);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
    );
}
#[test]
fn new_contract_unstamped_genuine_receipt_refuses_once_before_native_credentials() {
    let (mut e, o, _) = engine(Controls {
        missing: true,
        ..Controls::default()
    });
    e.handle(
        f64::MAX,
        Event::Command(Command::FetchAccount { command: 7 }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        o.lock().unwrap().entries,
        vec![CommandEntryError::Rejected(Error::Revoked)]
    );
    assert!(o.lock().unwrap().local.is_empty());
    assert!(o.lock().unwrap().roots.is_empty());
    assert_eq!(failed(&actions).len(), 1);
    let mut expected = Error::Revoked.venue_error();
    expected.outcome = None;
    assert_eq!(failed(&actions)[0], &expected);
    assert_eq!(failed(&actions)[0].outcome, None);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_)
            | Action::Sign { .. }
            | Action::SetTimer { .. }
            | Action::Notice(Notice::CommandError { .. })
    )));
}
