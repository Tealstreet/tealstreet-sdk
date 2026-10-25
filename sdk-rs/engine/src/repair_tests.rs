// Original controls for RUST_REPAIR_MUTATION_AUTHORITY.md. Every positive starts
// at real command admission or a real Background Read delivery and drives the
// real Prepare/Bind/Claim path; no entitlement, root or delivery is forged.
#[allow(clippy::result_large_err)]
mod repair {
    use super::*;
    use crate::model::{Order, OrderSide, OrderStatus, OrderType};
    use crate::repair::{Binding, Entitlement, RepairNotice, RepairRefusal, RepairRoot, Subject};
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
        BeforeAdmission,
        Secondary,
        Empty,
    }

    #[derive(Default)]
    struct Repairer {
        mint: Mint,
        owed: Vec<&'static str>,
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
        read_slot: Option<Credential>,
        publish: bool,
        read: Option<Context>,
        snapshot_mints: Vec<Result<(), RepairRefusal>>,
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
            let owed: Vec<Subject> = self.owed.iter().map(|k| subject(k)).collect();
            if self.mint == Mint::BeforeAdmission {
                let before = Self::mark(cx);
                let result = cx.mint_command_repair(primary(), owed.clone());
                self.mints.push(result.map(drop));
                self.mint_spent_nothing.push(Self::mark(cx) == before);
            }
            let policy = Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            };
            let context = cx.admit_http_command(policy);
            if self.mint == Mint::BeforeAdmission {
                self.admitted_after_refusal = Some(context.is_ok());
            }
            let context = context.unwrap();
            let minted = match self.mint {
                Mint::AfterAdmission => Some(cx.mint_command_repair(primary(), owed)),
                Mint::Secondary => Some(cx.mint_command_repair(secondary(), owed)),
                Mint::Empty => {
                    let before = Self::mark(cx);
                    let result = cx.mint_command_repair(primary(), Vec::new());
                    self.mint_spent_nothing.push(Self::mark(cx) == before);
                    Some(result)
                }
                Mint::BeforeAdmission => None,
            };
            if let Some(minted) = minted {
                self.mints.push(minted.as_ref().map(drop).map_err(|e| *e));
                if let Ok(entitlement) = minted {
                    self.entitlements.push(entitlement);
                }
            }
            cx.done_context(&context, "[]".into()).unwrap();
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
            if self.read.as_ref() == Some(&context) {
                cx.validate_background_read_http_delivery(&context).unwrap();
                if self.publish {
                    cx.store.upsert_order(order("old-1", "BTCUSD"));
                }
                let minted = cx.mint_snapshot_repair(&context, primary(), subject("old-1"), "BTCUSD");
                self.snapshot_mints
                    .push(minted.as_ref().map(drop).map_err(|e| *e));
                if let Ok(entitlement) = minted {
                    self.entitlements.push(entitlement);
                }
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
                let minted = cx.mint_snapshot_repair(&context, primary(), subject("old-1"), "BTCUSD");
                self.snapshot_mints.push(minted.map(drop));
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
                // A second, later snapshot of the same order.
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
        let e = minted(&["c1", "c2"]);
        assert_eq!(e.venue.mints, vec![Ok(())]);
        let mut e = e;
        assert_eq!(subjects(&mut e, 0), vec![subject("c1"), subject("c2")]);
    }
    #[test]
    fn a_mint_before_admission_allocates_nothing_and_keeps_the_entry() {
        let mut e = engine(Repairer {
            mint: Mint::BeforeAdmission,
            owed: vec!["c1"],
            ..Default::default()
        });
        let actions = cancel_all(&mut e, 7, 1.);
        assert_eq!(e.venue.mints, vec![Err(RepairRefusal::NotInEntry)]);
        assert_eq!(e.venue.mint_spent_nothing, vec![true]);
        assert_eq!(e.venue.admitted_after_refusal, Some(true));
        assert!(done(&actions, 7));
    }
    #[test]
    fn a_mint_outside_any_command_entry_is_refused() {
        let mut e = engine(Repairer::default());
        let before = e.io.core.next_id;
        let mut cx = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert_eq!(
            cx.mint_command_repair(primary(), vec![subject("c1")]),
            Err(RepairRefusal::NotInEntry)
        );
        assert_eq!(e.io.core.next_id, before);
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
    #[test]
    fn a_secondary_credential_cannot_mint_write_authority() {
        let mut e = engine(Repairer {
            mint: Mint::Secondary,
            owed: vec!["c1"],
            ..Default::default()
        });
        cancel_all(&mut e, 7, 1.);
        assert_eq!(e.venue.mints, vec![Err(RepairRefusal::NoPrimary)]);
    }
    #[test]
    fn identities_are_frozen_after_start() {
        // The plan's credential-generation proof rests on this precondition.
        let mut e = engine(Repairer::default());
        assert!(e.set_identities(Identities::default()).is_err());
    }

    // Spending subjects as cancel roots.

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
        let left = subjects(&mut e, 0);
        assert_eq!(left.len(), 5, "spending one root keeps the other subjects");
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
        cancel_all(&mut e, 8, 1.5);
        e.venue.owed = vec!["c2"];
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
        let first = preps(&fire(&mut e, 2.))[0];
        release(&mut e, first, 2.);
        reply(&mut e, first, 3., 503, "gateway");
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        fire(&mut e, 4.);
        assert_eq!(e.venue.admits, vec![Ok(()), Err(RepairRefusal::Spent)]);
    }

    // Classification, journal and notice custody.

    #[test]
    fn a_repair_cancel_signs_with_the_installed_primary_and_classifies_as_cancel() {
        let mut e = minted(&["c1"]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        let token = preps(&fire(&mut e, 2.))[0];
        assert_eq!(
            e.venue.roots[0].1.credential().map(|c| c.slot),
            Some(crate::CredentialSlot::Primary)
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
        let mut e = minted(&["c1"]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        let token = preps(&fire(&mut e, 2.))[0];
        release(&mut e, token, 2.);
        reply(&mut e, token, 3., 503, "gateway");
        assert!(e.venue.notices[0].contains("cancel: Unknown"), "{:?}", e.venue.notices);
        assert!(e.io.core.order_locks.uncertain("BTCUSD"));
    }
    #[test]
    fn a_settled_notice_runs_outside_any_entry_and_may_admit_the_next_group() {
        let mut e = minted(&["c1", "c2"]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        e.venue.next_in_notice = vec![vec![(0, "c2", "c2", "ETHUSD", "buy")]];
        let token = preps(&fire(&mut e, 2.))[0];
        release(&mut e, token, 2.);
        let actions = reply(&mut e, token, 3., 200, r#"{"code":0,"ids":["c1"]}"#);
        assert_eq!(e.venue.notice_outside_entry, vec![true]);
        assert_eq!(e.venue.admits, vec![Ok(()), Ok(())]);
        assert_eq!(preps(&actions).len(), 1, "next FIFO group without a user command");
    }
    #[test]
    fn a_foreground_cancel_of_a_repair_held_order_is_refused() {
        let mut e = minted(&["c1"]);
        e.venue.groups = vec![vec![(0, "c1", "c1", "BTCUSD", "buy")]];
        let token = preps(&fire(&mut e, 2.))[0];
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
        if e.venue.publish {
            assert_eq!(e.venue.snapshot_mints, vec![Ok(())], "published snapshot mints");
        }
        (e, actions)
    }
    #[test]
    fn a_published_snapshot_order_mints_in_its_data_delivery() {
        let (e, _) = snapshot(Repairer {
            publish: true,
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Ok(())]);
    }
    #[test]
    fn a_secondary_signed_snapshot_mints_and_the_repair_signs_with_primary() {
        let (mut e, _) = snapshot(Repairer {
            publish: true,
            read_slot: Some(secondary()),
            replace: true,
            ..Default::default()
        });
        assert_eq!(e.venue.snapshot_mints, vec![Ok(())]);
        fire(&mut e, 2.);
        assert_eq!(e.venue.admits, vec![Ok(())]);
        assert_eq!(
            e.venue.roots[0].1.credential().map(|c| c.slot),
            Some(crate::CredentialSlot::Primary)
        );
    }
    #[test]
    fn a_mint_before_store_publication_is_refused() {
        let (mut e, actions) = snapshot(Repairer {
            publish: false,
            ..Default::default()
        });
        e.handle(1.6, Event::TimerFired { timer: e.venue.timer.unwrap() });
        assert_eq!(e.venue.snapshot_mints, vec![Err(RepairRefusal::StoreMissing)]);
        assert_eq!(warns(&actions, "Automatic repair authority unavailable: StoreMissing"), 1);
        assert!(e.venue.notices.iter().any(|n| n.contains("MintRefused")));
    }
    #[test]
    fn a_failure_callback_cannot_mint() {
        let mut e = engine(Repairer {
            snapshot: true,
            publish: true,
            ..Default::default()
        });
        let token = preps(&drain(&mut e))[0];
        release(&mut e, token, 1.);
        e.handle(
            1.5,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "network unavailable",
            },
        );
        assert_eq!(e.venue.failures, 1);
        assert_eq!(e.venue.snapshot_mints, vec![Err(RepairRefusal::NotData)]);
    }
    #[test]
    fn a_second_snapshot_of_the_same_order_does_not_remint() {
        let (mut e, _) = snapshot(Repairer {
            publish: true,
            ..Default::default()
        });
        let token = preps(&fire(&mut e, 2.))[0];
        release(&mut e, token, 2.);
        reply(&mut e, token, 2.5, 200, "[]");
        assert_eq!(e.venue.snapshot_mints, vec![Ok(()), Err(RepairRefusal::Duplicate)]);
    }
    #[test]
    fn an_accepted_cancel_opens_exactly_one_placement_even_after_the_old_order_shows_cancelled() {
        let (mut e, _) = snapshot(Repairer {
            publish: true,
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
            publish: true,
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
    fn retiring_an_unspent_subject_is_idempotent_and_spares_the_others() {
        let mut e = minted(&["c1", "c2"]);
        let mut cx = Cx {
            now: 2.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let entitlement = &e.venue.entitlements[0];
        let retire = crate::repair::Retire::Terminal;
        assert_eq!(cx.retire_repair_subject(entitlement, "c1", retire), Ok(()));
        assert_eq!(cx.retire_repair_subject(entitlement, "c1", retire), Ok(()));
        assert_eq!(cx.repair_subjects(entitlement), vec![subject("c2")]);
    }
    #[test]
    fn capacity_refusal_keeps_the_command_successful_with_one_warn() {
        let mut e = minted(&["c0"]);
        let mut last = Vec::new();
        for command in 8..300 {
            e.venue.owed = vec!["c"];
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
