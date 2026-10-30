//! Phemex late cancels spend only authority minted by the original cancel-all.
use super::*;
use safe_cex_engine::classify::BatchItem;
use safe_cex_engine::repair::{
    Binding, Effect, Entitlement, RepairNotice, RepairRoot, Retire, SnapshotReceipt, Subject,
};

#[derive(Default)]
pub(super) struct State {
    grants: Vec<Grant>,
    active: Option<Active>,
}
struct Grant {
    symbol: String,
    entitlement: Entitlement,
    subjects: Vec<Subject>,
}
struct Active {
    pending: RepairRequest,
    native: Request,
    error: Option<VenueError>,
}
#[derive(Clone)]
pub struct RepairRequest {
    root: RepairRoot,
    context: Context,
}
impl PhemexPerpetuals {
    pub(crate) fn mint_late_subjects(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
        symbol: &str,
        lost: &[(String, u64)],
    ) -> Vec<(String, u64)> {
        let mut covered = Vec::new();
        let mut subjects = Vec::new();
        for (key, generation) in lost {
            if self.trading.repair.grants.iter().any(|grant| {
                grant.symbol == symbol
                    && grant
                        .subjects
                        .iter()
                        .any(|subject| subject.key == *key && subject.generation == *generation)
            }) {
                covered.push((key.clone(), *generation));
            } else if !subjects.iter().any(|s: &Subject| s.key == *key) {
                subjects.push(Subject {
                    key: key.clone(),
                    generation: *generation,
                });
            }
        }
        if !subjects.is_empty() {
            match cx.mint_command_repair(preflight, subjects.clone()) {
                Ok(entitlement) => {
                    covered.extend(subjects.iter().map(|s| (s.key.clone(), s.generation)));
                    self.trading.repair.grants.push(Grant {
                        symbol: symbol.into(),
                        entitlement,
                        subjects,
                    });
                }
                // Capacity already emits the core CommandError plus one Warn.
                Err(safe_cex_engine::repair::RepairRefusal::Capacity) => {}
                Err(reason) => cx.notice(
                    Level::Error,
                    code::CHILD_LEG_FAILED,
                    format!("Phemex could not authorize late-landing cancels on {symbol}: {reason:?}; cancel them manually"),
                ),
            }
        }
        covered
    }
    /// Frame-fed: an ORDERS frame that covers a symbol's signed expiries ends
    /// its lost placements; one it does not show never landed, and its late
    /// cancel authority retires.
    pub(crate) fn expire_lost_from_frames(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(cover) = cx
            .source_orders_cover()
            .filter(|_| cx.store.loaded().orders)
        else {
            return;
        };
        for (symbol, since) in self.account.take_proven_lost(cover) {
            for grant in &mut self.trading.repair.grants {
                if grant.symbol == symbol {
                    grant.subjects.retain(|subject| {
                        let proof = Retire::SourceOrders { since };
                        cx.retire_repair_subject(&grant.entitlement, &subject.key, proof)
                            .is_err()
                    });
                }
            }
        }
        self.trading
            .repair
            .grants
            .retain(|grant| !grant.subjects.is_empty());
    }
    pub(crate) fn has_late_subjects(&self, symbol: &str) -> bool {
        self.trading
            .repair
            .grants
            .iter()
            .any(|g| g.symbol == symbol && !g.subjects.is_empty())
    }
    pub(crate) fn retire_late_subjects(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        covered: Option<(&str, u64, &SnapshotReceipt)>,
    ) {
        for grant in &mut self.trading.repair.grants {
            grant.subjects.retain(|subject| {
                if cx
                    .retire_repair_subject(&grant.entitlement, &subject.key, Retire::Terminal)
                    .is_ok()
                {
                    return false;
                }
                if let Some((symbol, generation, receipt)) = covered
                    && grant.symbol == symbol
                    && subject.generation <= generation
                {
                    return cx
                        .retire_repair_subject(
                            &grant.entitlement,
                            &subject.key,
                            Retire::CompleteRead(receipt),
                        )
                        .is_err();
                }
                true
            });
        }
        self.trading
            .repair
            .grants
            .retain(|grant| !grant.subjects.is_empty());
    }
    fn late_cancel_error(cx: &mut Cx<'_, Pending>, ids: &[String], error: &str) {
        cx.notice(Level::Error,code::CHILD_LEG_FAILED,format!(
            "Phemex could not cancel late-landing order {} after cancel-all: {error}; cancel it manually",ids.join(", ")));
    }
    pub(super) fn late_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.repair.active.is_some() {
            return;
        }
        while let Some(step) = self.trading.late.pop_front() {
            let Call::Cancel { before, ids } = &step.call else {
                unreachable!("late queue contains only native cancels")
            };
            let side = step
                .query
                .split('&')
                .find_map(|part| part.strip_prefix("posSide="))
                .unwrap_or("");
            let orders: Vec<_> = ids
                .iter()
                .filter_map(|id| cx.store.order(id).cloned())
                .collect();
            let mut members = Vec::new();
            for id in ids {
                let Some(order) = orders.iter().find(|order| order.id == *id) else {
                    break;
                };
                let Some((grant, subject)) = self.trading.repair.grants.iter().find_map(|grant| {
                    (grant.symbol == order.symbol)
                        .then(|| {
                            grant
                                .subjects
                                .iter()
                                .find(|subject| {
                                    order.client_order_id.as_deref() == Some(subject.key.as_str())
                                })
                                .map(|subject| (grant, subject))
                        })
                        .flatten()
                }) else {
                    break;
                };
                members.push((
                    &grant.entitlement,
                    subject,
                    Binding {
                        order_id: id,
                        symbol: &order.symbol,
                        side,
                    },
                ));
            }
            let admission = if members.len() == ids.len() {
                cx.admit_repair_cancel(&members)
            } else {
                Err(safe_cex_engine::repair::RepairRefusal::StoreMissing)
            };
            let (root, context) = match admission {
                Ok(pair) => pair,
                Err(error) => {
                    Self::late_cancel_error(cx, ids, &format!("{error:?}"));
                    self.account_refresh(cx);
                    continue;
                }
            };
            let identity = self
                .credentials
                .as_ref()
                .filter(|identity| {
                    context
                        .credential()
                        .is_some_and(|credential| identity.owns(&credential.reference))
                })
                .cloned();
            if identity.is_some() {
                for order in before {
                    cx.store.set_order_status(&order.id, OrderStatus::Canceling);
                }
                self.account_write(cx);
            }
            let pending = RepairRequest {
                root,
                context: context.clone(),
            };
            self.trading.repair.active = Some(Active {
                pending: pending.clone(),
                native: Request {
                    command: None,
                    call: step.call,
                    revision: self.account.order_revision(),
                    signed_until: None,
                    released_request: None,
                },
                error: None,
            });
            let Some(identity) = identity else {
                self.repair_failed(
                    cx,
                    pending,
                    VenueError::new("Phemex repair Primary identity mismatch")
                        .kind(ErrorKind::AuthInvalid)
                        .outcome(safe_cex_engine::Outcome::NotSent),
                );
                return;
            };
            let recipe = crate::dispatch_signing::Http::new(
                identity,
                self.options.testnet,
                step.method,
                step.path.into(),
                step.query.clone(),
                None,
            );
            let url = format!("{}{}?{}", self.rest(), step.path, step.query);
            self.queue_late_repair(cx, pending, step.method, url, Box::new(recipe));
            return;
        }
    }
    fn queue_late_repair(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: RepairRequest,
        method: Method,
        url: String,
        recipe: Box<dyn safe_cex_engine::SignStep>,
    ) {
        if let Err((Pending::Repair(pending), error)) = cx.request_signed_managed(
            pending.context.clone(),
            None,
            method,
            url,
            None,
            recipe,
            Pending::Repair(pending),
        ) {
            self.repair_failed(cx, pending, error.venue_error());
        }
    }
    pub(crate) fn repair_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: RepairRequest,
        response: Response<'_>,
    ) {
        let Some(active) = self.trading.repair.active.as_ref().filter(|active| {
            active.pending.root == pending.root && active.pending.context == pending.context
        }) else {
            return;
        };
        if cx.validate_http_context(&pending.context).is_err() {
            return;
        }
        let Call::Cancel { before, ids } = &active.native.call else {
            unreachable!()
        };
        let mut envelope_refusal = None;
        let decoded = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!("HTTP {}: {}", response.status, response.body));
            }
            let env: Envelope = serde_json::from_str(response.body)
                .map_err(|_| "Phemex malformed write acknowledgement".to_string())?;
            if env.error.is_some() || env.code.is_some_and(|code| code != 0) {
                envelope_refusal = env.code.filter(|code| *code != 0).map(NativeCode::Int);
                return Err(format!("Phemex {} {}", env.code.unwrap_or(-1), env.msg));
            }
            if env.code.is_none() {
                return Err("Phemex malformed write acknowledgement".into());
            }
            if self.account.order_revision() != active.native.revision {
                return Err(
                    "Phemex order changed during cancel acknowledgement; await reconciliation"
                        .into(),
                );
            }
            let raw = env
                .data
                .filter(|raw| raw.get() != "null")
                .ok_or("Phemex missing cancel acknowledgement")?;
            decode_cancel_ack(raw.get(), ids)
        })();
        let unknown = Native {
            code: NativeCode::Int(-1),
            message: "",
        };
        let items: Vec<_> = ids
            .iter()
            .map(|id| {
                if decoded.as_ref().is_ok_and(|ack| ack.accepted.contains(id)) {
                    BatchItem::Accepted(id)
                } else {
                    let code = decoded
                        .as_ref()
                        .ok()
                        .and_then(|ack| ack.refusals.get(id).copied())
                        .or(envelope_refusal)
                        .unwrap_or(unknown.code);
                    BatchItem::Native(Native { code, message: "" })
                }
            })
            .collect();
        let message = decoded
            .as_ref()
            .err()
            .cloned()
            .or_else(|| decoded.as_ref().ok().and_then(|ack| ack.problem.clone()));
        let result = match cx.http_write_batch(
            &pending.context,
            &REFUSALS,
            ids.len(),
            &items,
            message
                .as_deref()
                .unwrap_or("Phemex unproven cancel acknowledgement"),
            None,
        ) {
            Ok(result) => result,
            Err(_) => return,
        };
        let before = before.clone();
        if let Ok(ack) = &decoded
            && !ack.accepted.is_empty()
            && cx.validate_http_write_effects(&pending.context).is_ok()
        {
            self.apply_cancel_ack(cx, &before, ack);
        }
        if let Some(active) = &mut self.trading.repair.active {
            active.error = match result {
                safe_cex_engine::request_context::WriteResult::Accepted { .. } => None,
                safe_cex_engine::request_context::WriteResult::Rejected(error)
                | safe_cex_engine::request_context::WriteResult::KnownPartial(error)
                | safe_cex_engine::request_context::WriteResult::PartialUnknown(error)
                | safe_cex_engine::request_context::WriteResult::Failed(error) => Some(error),
            };
        }
        let _ = cx.finish_http_context(&pending.context);
    }
    pub(crate) fn repair_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: RepairRequest,
        error: VenueError,
    ) {
        let Some(active) = self.trading.repair.active.as_mut().filter(|active| {
            active.pending.root == pending.root && active.pending.context == pending.context
        }) else {
            return;
        };
        active.error = Some(error);
        let _ = cx.finish_http_context(&pending.context);
    }
    pub(crate) fn late_repair_notice(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        notice: RepairNotice<'_>,
    ) {
        let RepairNotice::Settled { root, subjects } = notice else {
            return;
        };
        if self
            .trading
            .repair
            .active
            .as_ref()
            .is_none_or(|active| active.pending.root != *root)
        {
            return;
        }
        let active = self.trading.repair.active.take().unwrap();
        let Call::Cancel { ids, .. } = &active.native.call else {
            unreachable!()
        };
        let success = active.error.is_none()
            && subjects
                .iter()
                .all(|subject| subject.cancel == Effect::Accepted);
        if success {
            cx.notice(
                Level::Warn,
                code::ORDERS_REPLACED,
                format!(
                    "Phemex canceled late-landing order {} after cancel-all",
                    ids.join(", ")
                ),
            );
            self.account_write(cx);
        } else {
            self.restore_failed_trade(cx, &active.native);
            Self::late_cancel_error(
                cx,
                ids,
                active
                    .error
                    .as_ref()
                    .map_or("Phemex unproven cancel acknowledgement", |error| {
                        error.message.as_str()
                    }),
            );
        }
        self.retire_late_subjects(cx, None);
        self.account_refresh(cx);
        self.late_next(cx);
    }
}

#[cfg(test)]
#[path = "../../tests/support/repair_signer.rs"]
mod repair_test_signer;

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::repair::RepairRefusal;
    use safe_cex_engine::request_context::{Credential, ReconciliationData};
    use safe_cex_engine::{Action, Engine, Event, SignStep, TimerId, Venue};

    struct RefuseHeaders(crate::dispatch_signing::Http);
    impl SignStep for RefuseHeaders {
        fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
            &[("X-Test", "\r\n")]
        }
        fn prepare(
            &mut self,
            action: &mut Action,
            context: &safe_cex_engine::DispatchContext<'_>,
        ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError>
        {
            self.0.prepare(action, context)
        }
        fn apply(
            &mut self,
            action: &mut Action,
            result: &safe_cex_engine::signing::SignResult,
        ) -> Result<(), safe_cex_engine::signing::SignError> {
            self.0.apply(action, result)
        }
        fn freshness_ms(&self) -> f64 {
            self.0.freshness_ms()
        }
    }
    struct Fixture {
        venue: PhemexPerpetuals,
        timer: Option<TimerId>,
        refuse_send: bool,
        sync_refused: bool,
        settled: Vec<Vec<Effect>>,
        kinds: Vec<Option<ErrorKind>>,
        mint_errors: Vec<RepairRefusal>,
    }
    fn order(id: &str) -> Order {
        Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 1.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: false,
            close: None,
            client_order_id: Some(format!("client-{id}")),
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }
    fn step(order: Order) -> Step {
        Step {
            method: Method::Delete,
            path: "/g-orders/cancel",
            query: format!("symbol=BTCUSDT&posSide=Merged&orderID={}", order.id),
            call: Call::Cancel {
                ids: vec![order.id.clone()],
                before: vec![order],
            },
            replacement: None,
        }
    }
    impl Venue for Fixture {
        type Pending = Pending;
        fn start(&mut self, _: &mut Cx<'_, Pending>) {}
        fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
            self.venue.book(symbol)
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            self.venue.books()
        }

        fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: safe_cex_engine::SocketId) {
            self.venue.on_open(cx, socket);
        }
        fn on_message(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            socket: safe_cex_engine::SocketId,
            text: &str,
        ) {
            self.venue.on_message(cx, socket, text);
        }
        fn on_close(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            socket: safe_cex_engine::SocketId,
            code: u16,
        ) {
            self.venue.on_close(cx, socket, code);
        }

        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
            let primary = self.venue.credentials.as_ref().unwrap();
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(Credential {
                        slot: safe_cex_engine::CredentialSlot::Primary,
                        reference: primary.credential.clone(),
                    }),
                })
                .unwrap();
            let subjects: Vec<_> = ["one", "two"]
                .into_iter()
                .map(|id| Subject {
                    key: format!("client-{id}"),
                    generation: 1,
                })
                .collect();
            let entitlement = cx
                .mint_command_repair(&preflight, subjects.clone())
                .unwrap();
            self.venue.trading.repair.grants.push(Grant {
                symbol: "BTCUSDT".into(),
                entitlement,
                subjects,
            });
            for id in ["one", "two"] {
                cx.store.upsert_order(order(id));
                self.venue.trading.late.push_back(step(order(id)));
            }
            cx.done_empty_plan(&preflight, "[]".into()).unwrap();
            self.timer = Some(cx.set_timer(1.));
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            if self.timer.take() != Some(timer) {
                return;
            }
            if !self.refuse_send {
                self.venue.late_next(cx);
                return;
            }
            let step = self.venue.trading.late.pop_front().unwrap();
            let grant = &self.venue.trading.repair.grants[0];
            let subject = &grant.subjects[0];
            let (root, context) = cx
                .admit_repair_cancel(&[(
                    &grant.entitlement,
                    subject,
                    Binding {
                        order_id: "one",
                        symbol: "BTCUSDT",
                        side: "Merged",
                    },
                )])
                .unwrap();
            cx.store.set_order_status("one", OrderStatus::Canceling);
            self.venue.account_write(cx);
            let pending = RepairRequest { root, context };
            self.venue.trading.repair.active = Some(Active {
                pending: pending.clone(),
                native: Request {
                    command: None,
                    call: step.call,
                    revision: self.venue.account.order_revision(),
                    signed_until: None,
                    released_request: None,
                },
                error: None,
            });
            let recipe = RefuseHeaders(crate::dispatch_signing::Http::new(
                self.venue.credentials.as_ref().unwrap().clone(),
                false,
                Method::Delete,
                step.path.into(),
                step.query.clone(),
                None,
            ));
            self.venue.queue_late_repair(
                cx,
                pending,
                Method::Delete,
                format!("https://api.phemex.com{}?{}", step.path, step.query),
                Box::new(recipe),
            );
            self.sync_refused = self
                .venue
                .trading
                .repair
                .active
                .as_ref()
                .is_some_and(|active| {
                    active.error.as_ref().is_some_and(|error| {
                        error.kind == Some(ErrorKind::InvalidInput)
                            && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                    })
                });
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
        fn capture_http_reconciliation(
            &self,
            pending: &Pending,
        ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
            self.venue.capture_http_reconciliation(pending)
        }
        fn on_repair_notice(&mut self, cx: &mut Cx<'_, Pending>, notice: RepairNotice<'_>) {
            match &notice {
                RepairNotice::Settled { subjects, .. } => {
                    self.settled
                        .push(subjects.iter().map(|subject| subject.cancel).collect());
                    self.kinds.push(
                        self.venue
                            .trading
                            .repair
                            .active
                            .as_ref()
                            .and_then(|active| active.error.as_ref().and_then(|error| error.kind)),
                    );
                }
                RepairNotice::MintRefused { reason, .. } => self.mint_errors.push(*reason),
            }
            self.venue.late_repair_notice(cx, notice);
        }
    }
    fn fixture(refuse_send: bool) -> Engine<Fixture> {
        let (signer, identities) = repair_test_signer::primary();
        let venue = PhemexPerpetuals::new(crate::Options {
            credentials: identities.primary.clone(),
            ..Default::default()
        });
        let mut engine = safe_cex_engine::test_support::seeded_engine(Fixture {
            venue,
            timer: None,
            refuse_send,
            sync_refused: false,
            settled: vec![],
            kinds: vec![],
            mint_errors: vec![],
        });
        engine
            .set_signer(Some(std::sync::Arc::new(signer)))
            .unwrap();
        engine.set_identities(identities).unwrap();
        engine.start(1000.);
        engine.handle(
            1001.,
            Event::Command(Command::CancelSymbolOrders {
                command: 7,
                symbol: "BTCUSDT".into(),
            }),
        );
        let timer = engine.venue().timer.unwrap();
        engine.drain_actions().for_each(drop);
        engine.handle(1002., Event::TimerFired { timer });
        engine
    }
    #[test]
    fn synchronous_repair_send_refusal_settles_restores_and_advances_fifo_once() {
        let mut engine = fixture(true);
        assert!(engine.venue().mint_errors.is_empty());
        assert!(
            engine.venue().sync_refused,
            "the genuine signed submission refused synchronously with InvalidInput/NotSent"
        );
        assert_eq!(
            engine.venue().settled,
            vec![vec![Effect::NotSent]],
            "exactly one core Settled must retire the refused root"
        );
        assert_eq!(
            engine.store().order("one").unwrap().status,
            OrderStatus::Open
        );
        engine.prepare_dispatch(1002.);
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(actions.iter().filter(|action|matches!(action,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.level==Level::Error&&event.message.contains("could not cancel late-landing order one"))).count(),1);
        let next=actions.iter().filter(|action|matches!(action,Action::ManagedHttp(a) if matches!(a.as_ref(),safe_cex_engine::request_context::HttpAction::Prepare{request,..} if request.method==Method::Delete&&request.url.ends_with("orderID=two")))).count();
        assert_eq!(
            next, 1,
            "production FIFO advances to the next genuine repair preparation"
        );
        assert_eq!(
            engine.store().order("two").unwrap().status,
            OrderStatus::Canceling
        );
    }
    #[test]
    fn documented_repair_refusals_keep_envelope_and_row_causes() {
        for body in [
            r#"{"code":11001,"msg":"documented refusal"}"#,
            r#"{"code":0,"data":[{"orderID":"one","code":11001,"ordStatus":"Rejected"}]}"#,
        ] {
            let mut engine = fixture(false);
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1002.);
            let request = actions
                .iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.method == Method::Delete => Some(request.id),
                    _ => None,
                })
                .expect("real prepared and released Primary cancel");
            engine.handle(
                1003.,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            assert_eq!(engine.venue().settled, vec![vec![Effect::Rejected]]);
            assert_eq!(engine.venue().kinds, vec![Some(ErrorKind::Rejected)]);
            assert_eq!(
                engine.store().order("one").unwrap().status,
                OrderStatus::Open
            );
        }
    }
}
