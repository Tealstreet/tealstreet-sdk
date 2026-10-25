// Original controls for RUST_REPAIR_MUTATION_AUTHORITY.md. Every positive starts
// at real command admission or a real Background Read delivery and drives the
// real Prepare/Bind/Claim path; no entitlement, root or delivery is forged.
#[allow(clippy::result_large_err)]
mod repair {
    use super::*;
    use crate::model::{Order, OrderSide, OrderStatus, OrderType};
    use crate::repair::{
        Binding, Entitlement, RepairNotice, RepairRefusal, RepairRoot, SnapshotReceipt, Subject,
    };
    use crate::request_context::{Credential, WriteResult};
    use crate::signing::{Identities, Identity, KeyRef};

    fn identity(name: &str) -> Identity {
        Identity {
            credential: KeyRef(format!("{name}-key")),
            signing: KeyRef(format!("{name}-sign")),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        }
    }
    fn primary() -> Credential {
        Credential {
            slot: crate::CredentialSlot::Primary,
            reference: KeyRef("primary-key".into()),
        }
    }
    fn secondary() -> Credential {
        Credential {
            slot: crate::CredentialSlot::Secondary,
            reference: KeyRef("secondary-key".into()),
        }
    }
    fn subject(key: &str) -> Subject {
        Subject {
            key: key.into(),
            generation: 1,
        }
    }
    fn order(id: &str, symbol: &str) -> Order {
        Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: symbol.into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
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

    /// Presents the request's own selected key, so primary custody is real.
    struct Present(KeyRef);
    impl crate::SignStep for Present {
        fn prepare(
            &mut self,
            outbound: &mut Action,
            context: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            let Action::Http(_) = outbound else {
                return Err(crate::signing::SignError::InvalidRequest);
            };
            Ok(crate::signing::SignRequest {
                receipt: crate::signing::SignReceipt {
                    nonce: None,
                    stamp_ms: Some(context.stamp_ms()),
                    expiry_ms: Some(context.stamp_ms() + 3_000.),
                },
                context: Default::default(),
                operations: vec![crate::signing::Operation::Present {
                    key: self.0.clone(),
                }],
            })
        }
        fn apply(
            &mut self,
            outbound: &mut Action,
            result: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            let Action::Http(http) = outbound else {
                return Err(crate::signing::SignError::InvalidRequest);
            };
            http.signer_headers.push((
                "x-credential".into(),
                String::from_utf8(result.values[0].clone())
                    .map_err(|_| crate::signing::SignError::InvalidResult)?,
            ));
            Ok(())
        }
        fn freshness_ms(&self) -> f64 {
            3_000.
        }
    }

    /// One planned member of a cancel root: (entitlement index, subject, order, symbol, side).
    type Member = (usize, &'static str, &'static str, &'static str, &'static str);

    #[derive(Clone, Copy, Default, PartialEq)]
    enum Mint {
        #[default]
        AfterAdmission,
        /// A Preflight retained from an earlier command, before this one admits.
        StalePreflight,
        ReadOnly,
        Empty,
        /// Try to spend from inside a command entry.
        AdmitInside,
        /// The native empty write Preflight with no mint.
        Baseline,
        #[cfg(feature = "serial-plans")]
        AfterPlan,
    }

    #[derive(Clone, Copy, Default, PartialEq)]
    enum Snap {
        #[default]
        Publish,
        Omit,
        /// Mint with the first delivery's receipt inside a later delivery.
        ForeignReceipt,
        /// Mutate the Store between the commit and the mint.
        StaleAfterCommit,
        /// Retire command-origin subjects with the committed snapshot.
        Retire,
    }

    #[derive(Default)]
    struct Repairer {
        mint: Mint,
        owed: Vec<&'static str>,
        preflights: Vec<crate::request_context::Preflight>,
        commands: usize,
        entitlements: Vec<Entitlement>,
        mints: Vec<Result<(), RepairRefusal>>,
        mint_spent_nothing: Vec<bool>,
        admitted_after_refusal: Option<bool>,
        groups: Vec<Vec<Member>>,
        next_in_notice: Vec<Vec<Member>>,
        roots: Vec<(RepairRoot, Context)>,
        admits: Vec<Result<(), RepairRefusal>>,
        admit_spent_nothing: Vec<bool>,
        notices: Vec<String>,
        notice_outside_entry: Vec<bool>,
        timer: Option<TimerId>,
        // Snapshot origin.
        snapshot: bool,
        snap: Snap,
        read_slot: Option<Credential>,
        declared_primary: Option<Credential>,
        token: Option<crate::store::OrderSnapshotToken>,
        read: Option<Context>,
        commits: Vec<bool>,
        first_receipt: Option<SnapshotReceipt>,
        snapshot_mints: Vec<Result<(), RepairRefusal>>,
        retires: Vec<Result<(), RepairRefusal>>,
        replace: bool,
        mark_old_cancelled: bool,
        successors: Vec<Result<(), RepairRefusal>>,
        successor_slot: Option<crate::CredentialSlot>,
        failures: usize,
    }
    impl Repairer {
        fn mark(cx: &Cx<'_, Context>) -> (crate::resource_ids::ResourceIds, usize) {
            (cx.io.core.next_id, cx.io.core.actions.len())
        }
        fn read_snapshot(&mut self, cx: &mut Cx<'_, Context>) {
            self.token = cx.store.begin_orders_snapshot();
            let credential = self.read_slot.clone().unwrap_or_else(primary);
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: Some(credential.clone()),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            self.read = Some(context.clone());
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Get,
                "https://venue.test/orders".into(),
                None,
                Box::new(Present(credential.reference)),
                context,
            )
            .map_err(|(_, error)| error)
            .unwrap();
        }
        fn send(&mut self, cx: &mut Cx<'_, Context>, context: Context, method: Method) {
            let key = context.credential().unwrap().reference.clone();
            cx.request_signed_managed(
                context.clone(),
                None,
                method,
                "https://venue.test/repair".into(),
                Some("repair-body".into()),
                Box::new(Present(key)),
                context,
            )
            .map_err(|(_, error)| error)
            .unwrap();
        }
        fn admit(&mut self, cx: &mut Cx<'_, Context>, group: &[Member]) {
            let before = Self::mark(cx);
            let subjects: Vec<Subject> = group.iter().map(|m| subject(m.1)).collect();
            let members: Vec<_> = group
                .iter()
                .zip(&subjects)
                .map(|(m, s)| {
                    (
                        &self.entitlements[m.0],
                        s,
                        Binding {
                            order_id: m.2,
                            symbol: m.3,
                            side: m.4,
                        },
                    )
                })
                .collect();
            let result = cx.admit_repair_cancel(&members);
            self.admits.push(result.as_ref().map(drop).map_err(|e| *e));
            match result {
                Ok((root, context)) => {
                    self.roots.push((root, context.clone()));
                    self.send(cx, context, Method::Delete);
                }
                Err(_) => self.admit_spent_nothing.push(Self::mark(cx) == before),
            }
        }
        fn record_mint(&mut self, minted: Result<Entitlement, RepairRefusal>) {
            self.mints.push(minted.as_ref().map(drop).map_err(|e| *e));
            if let Ok(entitlement) = minted {
                self.entitlements.push(entitlement);
            }
        }
        fn write_preflight(cx: &mut Cx<'_, Context>) -> crate::request_context::Preflight {
            cx.begin_preflight(crate::request_context::PreflightPolicy {
                purpose: crate::request_context::PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(primary()),
            })
            .unwrap()
        }
        fn snapshot_data(&mut self, cx: &mut Cx<'_, Context>, context: &Context) {
            cx.validate_background_read_http_delivery(context).unwrap();
            let token = self.token.take().expect("token begun before the read");
            let publish = matches!(
                self.snap,
                Snap::Publish | Snap::ForeignReceipt | Snap::StaleAfterCommit
            );
            let receipt = cx.commit_repair_snapshot(context, [token], |orders| {
                let mut rows: Vec<Order> = orders
                    .iter()
                    .filter(|o| o.id != "old-1")
                    .cloned()
                    .collect();
                if publish {
                    rows.push(order("old-1", "BTCUSD"));
                }
                rows
            });
            self.commits.push(receipt.is_some());
            let Some(receipt) = receipt else {
                return;
            };
            if self.snap == Snap::Retire {
                for key in ["c1", "c2"] {
                    let retired = cx.retire_repair_subject(
                        &self.entitlements[0],
                        key,
                        crate::repair::Retire::CompleteRead(&receipt),
                    );
                    self.retires.push(retired);
                }
                return;
            }
            if self.snap == Snap::StaleAfterCommit {
                cx.store.upsert_order(order("other", "ETHUSD"));
            }
            let primary = self.declared_primary.clone().unwrap_or_else(primary);
            let foreign = self.snap == Snap::ForeignReceipt && self.first_receipt.is_some();
            let used = if foreign {
                self.first_receipt.as_ref().unwrap()
            } else {
                &receipt
            };
            let minted = cx.mint_snapshot_repair(used, primary, subject("old-1"), "BTCUSD");
            self.snapshot_mints
                .push(minted.as_ref().map(drop).map_err(|e| *e));
            if let Ok(entitlement) = minted {
                self.entitlements.push(entitlement);
            }
            if self.first_receipt.is_none() {
                self.first_receipt = Some(receipt);
            }
        }
    }
    impl Venue for Repairer {
        type Pending = Context;
        fn start(&mut self, cx: &mut Cx<'_, Context>) {
            if self.snapshot {
                self.read_snapshot(cx);
            }
            self.timer = Some(cx.set_timer(100.));
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            self.commands += 1;
            let owed: Vec<Subject> = self.owed.iter().map(|k| subject(k)).collect();
            match self.mint {
                Mint::StalePreflight if !self.preflights.is_empty() => {
                    let before = Self::mark(cx);
                    let stale = self.preflights[0].clone();
                    let minted = cx.mint_command_repair(&stale, owed);
                    self.mints.push(minted.map(drop));
                    self.mint_spent_nothing.push(Self::mark(cx) == before);
                    let preflight = Self::write_preflight(cx);
                    self.admitted_after_refusal = Some(true);
                    cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                }
                Mint::ReadOnly => {
                    let preflight = cx
                        .begin_preflight(crate::request_context::PreflightPolicy {
                            purpose: crate::request_context::PreflightPurpose::ReadOnly,
                            read_credential: None,
                            write_credential: None,
                        })
                        .unwrap();
                    let before = Self::mark(cx);
                    let minted = cx.mint_command_repair(&preflight, owed);
                    self.mint_spent_nothing.push(Self::mark(cx) == before);
                    self.record_mint(minted);
                    cx.done_preflight(&preflight, "[]".into()).unwrap();
                }
                Mint::Empty => {
                    let preflight = Self::write_preflight(cx);
                    let before = Self::mark(cx);
                    let minted = cx.mint_command_repair(&preflight, Vec::new());
                    self.mint_spent_nothing.push(Self::mark(cx) == before);
                    self.record_mint(minted);
                    cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                }
                Mint::Baseline => {
                    let preflight = Self::write_preflight(cx);
                    cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                }
                Mint::AdmitInside if !self.entitlements.is_empty() => {
                    let preflight = Self::write_preflight(cx);
                    self.admit(cx, &[(0, "c1", "c1", "BTCUSD", "buy")]);
                    cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                }
                #[cfg(feature = "serial-plans")]
                Mint::AfterPlan => {
                    let preflight = Self::write_preflight(cx);
                    cx.admit_serial_plan(
                        &preflight,
                        crate::request_context::plan::SerialCall {
                            operation: Operation::Mutation,
                            timeout_ms: 20_000,
                            attempts: 1,
                        },
                        crate::request_context::plan::SerialPolicy {
                            credential: primary(),
                            pacing: None,
                            refusals_complete: true,
                        },
                    )
                    .unwrap();
                    let minted = cx.mint_command_repair(&preflight, owed);
                    self.record_mint(minted);
                }
                _ => {
                    let preflight = Self::write_preflight(cx);
                    let minted = cx.mint_command_repair(&preflight, owed);
                    self.record_mint(minted);
                    self.preflights.push(preflight.clone());
                    cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                }
            }
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
            if self.read.as_ref() == Some(&context) {
                self.snapshot_data(cx, &context);
                cx.finish_http_context(&context).unwrap();
                return;
            }
            #[derive(serde::Deserialize)]
            struct Ack {
                #[serde(default)]
                ids: Vec<String>,
            }
            let ids = serde_json::from_str::<Ack>(response.body)
                .map(|ack| ack.ids)
                .unwrap_or_default();
            cx.http_write_result(&context, WriteResult::Accepted { order_ids: ids })
                .unwrap();
            cx.finish_http_context(&context).unwrap();
            let Some((root, _)) = self
                .roots
                .iter()
                .find(|(_, leg)| *leg == context)
                .cloned()
            else {
                return;
            };
            if self.replace && self.successors.is_empty() {
                if self.mark_old_cancelled {
                    cx.store.set_order_status("old-1", OrderStatus::Canceled);
                }
                let next = cx.repair_successor(&root);
                self.successors.push(next.as_ref().map(drop).map_err(|e| *e));
                let again = cx.repair_successor(&root);
                self.successors.push(again.map(drop));
                if let Ok(next) = next {
                    self.successor_slot = next.credential().map(|c| c.slot);
                    self.roots.push((root, next.clone()));
                    self.send(cx, next, Method::Post);
                }
            }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            self.failures += 1;
            if self.read.as_ref() == Some(&context) {
                let token = self.token.take().expect("token begun before the read");
                let receipt = cx.commit_repair_snapshot(&context, [token], |orders| orders.to_vec());
                self.commits.push(receipt.is_some());
                if let Some(first) = &self.first_receipt {
                    let minted = cx.mint_snapshot_repair(first, primary(), subject("old-1"), "BTCUSD");
                    self.snapshot_mints.push(minted.map(drop));
                }
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Context>, _: TimerId) {
            if self.replace {
                let s = subject("old-1");
                let result = cx.admit_repair_replace(
                    &self.entitlements[0],
                    &s,
                    Binding {
                        order_id: "old-1",
                        symbol: "BTCUSD",
                        side: "buy",
                    },
                );
                self.admits.push(result.as_ref().map(drop).map_err(|e| *e));
                if let Ok((root, context)) = result {
                    self.roots.push((root, context.clone()));
                    self.send(cx, context, Method::Delete);
                }
                return;
            }
            if self.snapshot && self.groups.is_empty() {
                // A later snapshot read.
                self.read_snapshot(cx);
                return;
            }
            for group in std::mem::take(&mut self.groups) {
                self.admit(cx, &group);
            }
        }
        fn on_repair_notice(&mut self, cx: &mut Cx<'_, Context>, notice: RepairNotice<'_>) {
            self.notice_outside_entry
                .push(cx.io.core.command_entry.is_none() && cx.io.core.http_delivery.is_none());
            self.notices.push(format!("{notice:?}"));
            for group in std::mem::take(&mut self.next_in_notice) {
                self.admit(cx, &group);
            }
        }
        fn guard_pending_placements(&self) -> bool {
            true
        }
        fn guard_order_mutations(&self) -> bool {
            true
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("repair-fixture".into())
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn engine(venue: Repairer) -> Engine<Repairer> {
        let mut e = crate::test_support::seeded_engine(venue);
        e.set_identities(Identities {
            primary: Some(identity("primary")),
            secondary: Some(identity("secondary")),
        })
        .unwrap();
        e.start(0.);
        e
    }
    fn cancel_all(e: &mut Engine<Repairer>, command: CommandId, now: f64) -> Vec<Action> {
        e.handle_command(
            now,
            Command::CancelSymbolOrders {
                command,
                symbol: "BTCUSD".into(),
            },
            None,
        )
        .unwrap();
        drain(e)
    }
    fn minted(owed: &[&'static str]) -> Engine<Repairer> {
        let mut e = engine(Repairer {
            owed: owed.to_vec(),
            ..Default::default()
        });
        cancel_all(&mut e, 7, 1.);
        assert_eq!(e.venue.mints, vec![Ok(())], "admitted command mints");
        e
    }
    fn subjects(e: &mut Engine<Repairer>, index: usize) -> Vec<Subject> {
        let cx = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        };
        e.venue
            .entitlements
            .get(index)
            .map(|entitlement| cx.repair_subjects(entitlement))
            .unwrap_or_default()
    }
    fn drain(e: &mut Engine<Repairer>) -> Vec<Action> {
        e.drain_actions().collect()
    }
    fn fire(e: &mut Engine<Repairer>, now: f64) -> Vec<Action> {
        let timer = e.venue.timer.unwrap();
        e.handle(now, Event::TimerFired { timer });
        drain(e)
    }
    fn preps(actions: &[Action]) -> Vec<Preparation> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
    fn release(e: &mut Engine<Repairer>, token: Preparation, now: f64) -> HttpRequest {
        e.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let mut actions = drain(e);
        if let Some((attempt, receipt)) = actions.iter().find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.receipt)),
            _ => None,
        }) {
            e.handle(
                now,
                Event::Signed {
                    attempt,
                    result: Ok(crate::signing::SignResult::new(vec![b"value".to_vec()], receipt)),
                },
            );
            actions = drain(e);
        }
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
            .expect("real Bind");
        assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
        request
    }
    fn reply(e: &mut Engine<Repairer>, token: Preparation, now: f64, status: u16, body: &str) -> Vec<Action> {
        e.handle(
            now,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        drain(e)
    }
    fn warns(actions: &[Action], text: &str) -> usize {
        actions
            .iter()
            .filter(|a| {
                matches!(a, Action::Notice(Notice::Event(event))
                    if event.level == Level::Warn && event.message == text)
            })
            .count()
    }
    fn done(actions: &[Action], command: CommandId) -> bool {
        actions.iter().any(|a| {
            matches!(a, Action::Notice(Notice::CommandDone { command: c, .. }) if *c == command)
        })
    }

    // Command-origin mint custody.

    #[test]
    fn a_command_mints_after_admission_with_every_owed_subject() {
        let mut e = minted(&["c1", "c2"]);
        assert_eq!(subjects(&mut e, 0), vec![subject("c1"), subject("c2")]);
    }
    #[test]
    fn a_stale_preflight_mints_nothing_and_keeps_the_entry() {
        let mut e = minted(&["c1"]);
        e.venue.mint = Mint::StalePreflight;
        let actions = cancel_all(&mut e, 8, 2.);
        assert_eq!(e.venue.mints, vec![Ok(()), Err(RepairRefusal::NotInEntry)]);
        assert_eq!(e.venue.mint_spent_nothing, vec![true]);
        assert_eq!(e.venue.admitted_after_refusal, Some(true));
        assert!(done(&actions, 8));
    }
    #[test]
    fn a_read_only_preflight_cannot_mint_write_authority() {
        let mut e = engine(Repairer {
            mint: Mint::ReadOnly,
            owed: vec!["c1"],
            ..Default::default()
        });
        e.handle_command(1., Command::FetchStpMode { command: 7 }, None)
            .unwrap();
        let actions = drain(&mut e);
        assert_eq!(e.venue.mints, vec![Err(RepairRefusal::NoPrimary)]);
        assert_eq!(e.venue.mint_spent_nothing, vec![true]);
        assert!(done(&actions, 7));
    }
    #[test]
    fn a_mint_outside_any_command_entry_is_refused() {
        let mut e = minted(&["c1"]);
        let before = e.io.core.next_id;
        let stale = e.venue.preflights[0].clone();
        let mut cx = Cx {
            now: 2.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert_eq!(
            cx.mint_command_repair(&stale, vec![subject("c2")]),
            Err(RepairRefusal::NotInEntry)
        );
        assert_eq!(e.io.core.next_id, before);
    }
    #[test]
    fn an_empty_write_preflight_completes_as_the_native_empty_plan() {
        let mut e = engine(Repairer {
            mint: Mint::Baseline,
            ..Default::default()
        });
        assert!(done(&cancel_all(&mut e, 7, 1.), 7));
    }
    #[test]
    fn no_owed_subjects_mint_nothing() {
        let mut e = engine(Repairer {
            mint: Mint::Empty,
            ..Default::default()
        });
        let actions = cancel_all(&mut e, 7, 1.);
        assert_eq!(e.venue.mints, vec![Err(RepairRefusal::Invalid)]);
        assert_eq!(e.venue.mint_spent_nothing, vec![true]);
        assert!(done(&actions, 7));
    }
    #[cfg(feature = "serial-plans")]
    #[test]
    fn a_command_mints_after_its_serial_plan_admission() {
        let mut e = engine(Repairer {
            mint: Mint::AfterPlan,
            owed: vec!["c1"],
            ..Default::default()
        });
        cancel_all(&mut e, 7, 1.);
        assert_eq!(e.venue.mints, vec![Ok(())]);
    }
    #[test]
    fn identities_are_frozen_after_start() {
        // The plan's credential-generation proof rests on this precondition.
        let mut e = engine(Repairer::default());
        assert!(e.set_identities(Identities::default()).is_err());
        assert!(e.set_signer(None).is_err());
    }

    // Spending subjects as cancel roots.

    #[test]
    fn a_root_cannot_be_admitted_inside_a_command_entry() {
        let mut e = minted(&["c1"]);
        e.venue.mint = Mint::AdmitInside;
        let actions = cancel_all(&mut e, 8, 2.);
        assert_eq!(e.venue.admits, vec![Err(RepairRefusal::NotInEntry)]);
        assert_eq!(e.venue.admit_spent_nothing, vec![true]);
        assert!(preps(&actions).is_empty());
    }
    #[test]
    fn twelve_subjects_on_two_symbols_make_two_roots_from_one_entitlement() {
        let owed = [
            "c1", "c2", "c3", "c4", "c5", "c6", "c7", "e1", "e2", "e3", "e4", "e5",
        ];
        let mut e = minted(&owed);
        let btc: Vec<Member> = owed[..7].iter().map(|k| (0, *k, *k, "BTCUSD", "buy")).collect();
        e.venue.groups = vec![btc];
        let actions = fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())]);
        assert_eq!(preps(&actions).len(), 1, "one native request per root");
        assert_eq!(subjects(&mut e, 0).len(), 5, "spending one root keeps the other subjects");
        let eth: Vec<Member> = owed[7..].iter().map(|k| (0, *k, *k, "ETHUSD", "buy")).collect();
        e.venue.groups = vec![eth];
        let actions = fire(&mut e, 3.);
        assert_eq!(e.venue.admits, vec![Ok(()), Ok(())]);
        assert_eq!(preps(&actions).len(), 1);
    }
    #[test]
    fn eleven_subjects_refuse_with_nothing_spent() {
        let owed = [
            "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9", "c10", "c11",
        ];
        let mut e = minted(&owed);
        e.venue.groups = vec![owed.iter().map(|k| (0, *k, *k, "BTCUSD", "buy")).collect()];
        let actions = fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Err(RepairRefusal::TooMany)]);
        assert_eq!(e.venue.admit_spent_nothing, vec![true]);
        assert!(preps(&actions).is_empty());
        assert_eq!(subjects(&mut e, 0).len(), 11);
    }
    #[test]
    fn mixed_sides_refuse_atomically() {
        let mut e = minted(&["c1", "c2"]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy"), (0, "c2", "c2", "BTCUSD", "sell")]];
        fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Err(RepairRefusal::Incompatible)]);
        assert_eq!(e.venue.admit_spent_nothing, vec![true]);
        assert_eq!(subjects(&mut e, 0).len(), 2);
    }
    #[test]
    fn two_cancel_all_origins_share_one_native_request() {
        let mut e = minted(&["c1"]);
        e.venue.owed = vec!["c2"];
        cancel_all(&mut e, 8, 1.5);
        assert_eq!(e.venue.mints, vec![Ok(()), Ok(())]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy"), (1, "c2", "c2", "BTCUSD", "buy")]];
        let actions = fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())]);
        assert_eq!(preps(&actions).len(), 1, "native grouping is preserved");
    }
    #[test]
    fn a_spent_subject_cannot_be_spent_again() {
        let mut e = minted(&["c1"]);
        e.venue.groups = vec![
            vec![(0, "c1", "c1", "BTCUSD", "buy")],
            vec![(0, "c1", "c1", "BTCUSD", "buy")],
        ];
        fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(()), Err(RepairRefusal::Spent)]);
        assert_eq!(e.venue.admit_spent_nothing, vec![true]);
    }
    #[test]
    fn a_failed_root_spends_its_subjects_and_never_replays() {
        let mut e = minted(&["c1"]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        let actions = fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())]);
        let first = preps(&actions)[0];
        release(&mut e, first, 2.);
        reply(&mut e, first, 3., 503, "gateway");
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        fire(&mut e, 4.);
        assert_eq!(e.venue.admits, vec![Ok(()), Err(RepairRefusal::Spent)]);
    }

    // Classification, journal and notice custody.

    fn one_root(owed: &[&'static str]) -> (Engine<Repairer>, Preparation) {
        let mut e = minted(owed);
        e.venue.groups = vec![vec![(0, owed[0], owed[0], "BTCUSD", "buy")]];
        let actions = fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())], "root admitted");
        (e, preps(&actions)[0])
    }
    #[test]
    fn a_repair_cancel_signs_with_the_preflight_primary_and_classifies_as_cancel() {
        let (mut e, token) = one_root(&["c1"]);
        assert_eq!(
            e.venue.roots[0].1.credential(),
            Some(&primary()),
            "the minting Preflight's write credential"
        );
        release(&mut e, token, 2.);
        // An id-less Accepted under a Place intent would seal Unknown.
        reply(&mut e, token, 3., 200, r#"{"code":0}"#);
        assert_eq!(e.venue.notices.len(), 1);
        assert!(e.venue.notices[0].contains("cancel: Accepted"), "{:?}", e.venue.notices);
        assert!(!e.io.core.order_locks.uncertain("BTCUSD"));
    }
    #[test]
    fn a_released_unknown_repair_cancel_fences_its_symbol() {
        let (mut e, token) = one_root(&["c1"]);
        release(&mut e, token, 2.);
        reply(&mut e, token, 3., 503, "gateway");
        assert!(e.venue.notices[0].contains("cancel: Unknown"), "{:?}", e.venue.notices);
        assert!(e.io.core.order_locks.uncertain("BTCUSD"));
    }
    #[test]
    fn a_settled_notice_runs_outside_any_entry_and_may_admit_the_next_group() {
        let (mut e, token) = one_root(&["c1", "c2"]);
        e.venue.next_in_notice = vec![vec![(0, "c2", "c2", "ETHUSD", "buy")]];
        release(&mut e, token, 2.);
        let actions = reply(&mut e, token, 3., 200, r#"{"code":0,"ids":["c1"]}"#);
        assert_eq!(e.venue.notice_outside_entry, vec![true]);
        assert_eq!(e.venue.admits, vec![Ok(()), Ok(())]);
        assert_eq!(preps(&actions).len(), 1, "next FIFO group without a user command");
    }
    #[test]
    fn a_foreground_cancel_of_a_repair_held_order_is_refused() {
        let (mut e, token) = one_root(&["c1"]);
        release(&mut e, token, 2.);
        e.handle_command(
            2.5,
            Command::CancelOrders {
                command: 9,
                ids: vec!["c1".into()],
            },
            None,
        )
        .unwrap();
        let actions = drain(&mut e);
        assert!(
            actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 9, .. }))),
            "lock conflict refuses the foreground write"
        );
        assert!(preps(&actions).is_empty());
    }

    // Snapshot origin (cancel-then-place).

    fn snapshot(venue: Repairer) -> (Engine<Repairer>, Vec<Action>) {
        let mut e = engine(Repairer {
            snapshot: true,
            ..venue
        });
        let token = preps(&drain(&mut e))[0];
        release(&mut e, token, 1.);
        let actions = reply(&mut e, token, 1.5, 200, "[]");
        assert_eq!(e.venue.commits, vec![true], "accepted snapshot commit");
        (e, actions)
    }
    fn second_read(e: &mut Engine<Repairer>) -> Vec<Action> {
        let token = preps(&fire(e, 2.))[0];
        release(e, token, 2.);
        reply(e, token, 2.5, 200, "[]")
    }
    #[test]
    fn a_published_snapshot_order_mints_in_its_data_delivery() {
        let (e, _) = snapshot(Repairer::default());
        assert_eq!(e.venue.snapshot_mints, vec![Ok(())]);
    }
    #[test]
    fn a_secondary_signed_snapshot_mints_and_the_repair_signs_with_primary() {
        let (mut e, _) = snapshot(Repairer {
            read_slot: Some(secondary()),
            replace: true,
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Ok(())]);
        fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())]);
        assert_eq!(e.venue.roots[0].1.credential(), Some(&primary()));
    }
    #[test]
    fn a_declared_secondary_cannot_sign_a_snapshot_repair() {
        let (e, _) = snapshot(Repairer {
            declared_primary: Some(secondary()),
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Err(RepairRefusal::NoPrimary)]);
    }
    #[test]
    fn an_order_the_snapshot_did_not_publish_cannot_mint() {
        let (e, actions) = snapshot(Repairer {
            snap: Snap::Omit,
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Err(RepairRefusal::StoreMissing)]);
        assert_eq!(warns(&actions, "Automatic repair authority unavailable: StoreMissing"), 1);
        assert!(e.venue.notices.iter().any(|n| n.contains("MintRefused")));
    }
    #[test]
    fn a_store_change_after_the_commit_makes_the_receipt_stale() {
        let (e, _) = snapshot(Repairer {
            snap: Snap::StaleAfterCommit,
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Err(RepairRefusal::Stale)]);
    }
    #[test]
    fn an_earlier_deliverys_receipt_cannot_mint_in_a_later_delivery() {
        let (mut e, _) = snapshot(Repairer {
            snap: Snap::ForeignReceipt,
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Ok(())]);
        second_read(&mut e);
        assert_eq!(e.venue.snapshot_mints, vec![Ok(()), Err(RepairRefusal::NotData)]);
    }
    #[test]
    fn a_failure_callback_cannot_commit_or_mint() {
        let (mut e, _) = snapshot(Repairer::default());
        let token = preps(&fire(&mut e, 2.))[0];
        release(&mut e, token, 2.);
        e.handle(
            2.5,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "network unavailable",
            },
        );
        assert_eq!(e.venue.failures, 1);
        assert_eq!(e.venue.commits, vec![true, false]);
        assert_eq!(e.venue.snapshot_mints, vec![Ok(()), Err(RepairRefusal::NotData)]);
    }
    #[test]
    fn a_second_snapshot_of_the_same_order_does_not_remint() {
        let (mut e, _) = snapshot(Repairer::default());
        second_read(&mut e);
        assert_eq!(e.venue.snapshot_mints, vec![Ok(()), Err(RepairRefusal::Duplicate)]);
    }
    #[test]
    fn a_cancel_then_place_root_holds_its_placing_scope_before_the_successor() {
        let (mut e, _) = snapshot(Repairer {
            replace: true,
            ..Default::default()
        });
        fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())]);
        let commands = e.venue.commands;
        cancel_all(&mut e, 9, 2.5);
        assert_eq!(e.venue.commands, commands, "cancel-all waits for the placing scope");
    }
    #[test]
    fn an_accepted_cancel_opens_exactly_one_placement_even_after_the_old_order_shows_cancelled() {
        let (mut e, _) = snapshot(Repairer {
            replace: true,
            mark_old_cancelled: true,
            ..Default::default()
        });
        let cancel = preps(&fire(&mut e, 2.))[0];
        release(&mut e, cancel, 2.);
        let actions = reply(&mut e, cancel, 3., 200, r#"{"code":0,"ids":["old-1"]}"#);
        assert_eq!(e.venue.successors, vec![Ok(()), Err(RepairRefusal::Spent)]);
        assert_eq!(e.venue.successor_slot, Some(crate::CredentialSlot::Primary));
        assert_eq!(preps(&actions).len(), 1, "one placement");
    }
    #[test]
    fn an_unknown_cancel_never_opens_a_placement() {
        let (mut e, _) = snapshot(Repairer {
            replace: true,
            ..Default::default()
        });
        let cancel = preps(&fire(&mut e, 2.))[0];
        release(&mut e, cancel, 2.);
        let actions = reply(&mut e, cancel, 3., 503, "gateway");
        assert!(e.venue.successors.is_empty());
        assert!(preps(&actions).is_empty());
        assert!(e.venue.notices.iter().any(|n| n.contains("cancel: Unknown") && n.contains("place: None")));
    }

    // Lifecycle and capacity.

    #[test]
    fn terminal_retirement_needs_a_terminal_store_order_and_spares_the_others() {
        let mut e = minted(&["c1", "c2"]);
        e.store.upsert_order(order("c1", "BTCUSD"));
        let mut cx = Cx {
            now: 2.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let entitlement = &e.venue.entitlements[0];
        let terminal = crate::repair::Retire::Terminal;
        assert_eq!(cx.retire_repair_subject(entitlement, "c1", terminal), Err(RepairRefusal::Invalid));
        cx.store.set_order_status("c1", OrderStatus::Canceled);
        assert_eq!(cx.retire_repair_subject(entitlement, "c1", terminal), Ok(()));
        assert_eq!(cx.retire_repair_subject(entitlement, "c1", terminal), Ok(()), "idempotent");
        assert_eq!(cx.repair_subjects(entitlement), vec![subject("c2")]);
    }
    #[test]
    fn a_complete_snapshot_begun_after_the_mint_retires_absent_subjects() {
        let mut e = minted(&["c1", "c2"]);
        e.venue.snapshot = true;
        e.venue.snap = Snap::Retire;
        second_read(&mut e);
        assert_eq!(e.venue.commits, vec![true]);
        assert_eq!(e.venue.retires, vec![Ok(()), Ok(())]);
        assert!(subjects(&mut e, 0).is_empty());
    }
    #[test]
    fn a_snapshot_begun_before_the_mint_cannot_retire() {
        let mut e = engine(Repairer {
            snapshot: true,
            snap: Snap::Retire,
            owed: vec!["c1", "c2"],
            ..Default::default()
        });
        let read = preps(&drain(&mut e))[0];
        cancel_all(&mut e, 7, 0.5);
        assert_eq!(e.venue.mints, vec![Ok(())], "admitted command mints");
        release(&mut e, read, 1.);
        reply(&mut e, read, 1.5, 200, "[]");
        assert_eq!(e.venue.commits, vec![true]);
        assert_eq!(e.venue.retires, vec![Err(RepairRefusal::Stale), Err(RepairRefusal::Stale)]);
        assert_eq!(subjects(&mut e, 0).len(), 2);
    }
    #[test]
    fn capacity_refusal_keeps_the_command_successful_with_one_warn() {
        let mut e = minted(&["c0"]);
        let mut last = Vec::new();
        for command in 8..300 {
            e.venue.owed = vec![Box::leak(format!("c{command}").into_boxed_str())];
            last = cancel_all(&mut e, command, 1.);
            assert!(done(&last, command), "the command never fails for repair capacity");
            if e.venue.mints.last() == Some(&Err(RepairRefusal::Capacity)) {
                break;
            }
        }
        assert_eq!(e.venue.mints.last(), Some(&Err(RepairRefusal::Capacity)));
        assert_eq!(warns(&last, "Automatic repair authority unavailable: capacity exhausted"), 1);
    }
}
