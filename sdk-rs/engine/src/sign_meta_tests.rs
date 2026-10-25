// Original controls for RUST_ENGINE_SIGN_META.md. Every task comes from real
// selection, admission, Prepare and a host-visible Action::Sign.
mod sign_meta {
    use super::*;
    use crate::credential_selection::{CredentialRole, PrimaryReason, Selection, SlotPolicy, Slots};
    use crate::request_context::{Credential, Session, SessionPolicy};
    use crate::sign_meta::{SignMeta, SignOwner};
    use crate::signing::{Identities, Identity, KeyRef};

    fn identity(name: &str) -> Identity {
        Identity::new(
            KeyRef(format!("{name}-key")),
            Some(KeyRef(format!("{name}-sign"))),
        )
    }

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
            _: &mut Action,
            _: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            Ok(())
        }
        fn freshness_ms(&self) -> f64 {
            3_000.
        }
    }

    #[derive(Clone, Copy, Default, PartialEq)]
    enum Mode {
        /// `request_selected_managed` under a command Mutation root.
        #[default]
        Mutation,
        /// `request_selected_managed` for a Background Read.
        Read,
        /// `request_selected_managed` for a Session login.
        Session,
        /// Plain `request_signed_managed`: no role was captured.
        Plain,
        /// A Secondary selection over a Primary-credentialed Context.
        Mismatch,
        /// Non-managed `Selection::sign`.
        Unmanaged,
    }

    #[derive(Default)]
    struct Signer {
        mode: Mode,
        role: Option<CredentialRole>,
        secondary: bool,
        context: Option<Context>,
        session: Option<Session>,
        queued: Option<Result<(), Error>>,
        actions_before: usize,
        actions_after: usize,
    }
    impl Signer {
        fn select(&self) -> (Selection, Credential) {
            let slots = Slots::new(
                Some(identity("primary")),
                self.secondary.then(|| identity("secondary")),
                SlotPolicy::default(),
            );
            let (selection, chosen) = slots
                .select(self.role.unwrap_or(CredentialRole::Mutation))
                .unwrap();
            let credential = selection.credential(chosen.credential.clone());
            (selection, credential)
        }
        fn policy(operation: Operation, credential: Credential) -> Policy {
            Policy {
                operation,
                credential: Some(credential),
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }
        }
        fn queue(&mut self, cx: &mut Cx<'_, Option<Context>>, context: Context, selection: &Selection) {
            self.context = Some(context.clone());
            let key = context.credential().unwrap().reference.clone();
            self.actions_before = cx.io.core.actions.len();
            let result = if self.mode == Mode::Plain {
                cx.request_signed_managed(
                    context.clone(),
                    None,
                    Method::Post,
                    "https://venue.test/sign".into(),
                    Some("body".into()),
                    Box::new(Present(key)),
                    Some(context),
                )
            } else {
                cx.request_selected_managed(
                    context.clone(),
                    selection,
                    None,
                    Method::Post,
                    "https://venue.test/sign".into(),
                    Some("body".into()),
                    Box::new(Present(key)),
                    Some(context),
                )
            };
            self.queued = Some(result.map(drop).map_err(|(_, error)| error));
            self.actions_after = cx.io.core.actions.len();
        }
    }
    impl Venue for Signer {
        type Pending = Option<Context>;
        fn start(&mut self, cx: &mut Cx<'_, Option<Context>>) {
            match self.mode {
                Mode::Read => {
                    let (selection, credential) = self.select();
                    let context = cx
                        .http_context(Scope::Background, Self::policy(Operation::Read, credential))
                        .unwrap();
                    self.queue(cx, context, &selection);
                }
                Mode::Session => {
                    let (selection, credential) = self.select();
                    let session = cx
                        .open_session(SessionPolicy {
                            credential: Some(credential.clone()),
                            repeat_safe: true,
                            private_egress: None,
                        })
                        .unwrap();
                    let context = cx
                        .session_context(&session, Self::policy(Operation::Mutation, credential))
                        .unwrap();
                    self.session = Some(session);
                    self.queue(cx, context, &selection);
                }
                Mode::Unmanaged => {
                    let (selection, credential) = self.select();
                    let request = cx.request(
                        Method::Post,
                        "https://venue.test/legacy".into(),
                        Vec::new(),
                        Some("body".into()),
                        None,
                    );
                    selection.sign(cx, request, Box::new(Present(credential.reference)));
                }
                _ => {}
            }
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Option<Context>>, _: Command) {
            let (selection, credential) = self.select();
            let credential = if self.mode == Mode::Mismatch {
                Credential {
                    slot: crate::CredentialSlot::Primary,
                    reference: KeyRef("primary-key".into()),
                }
            } else {
                credential
            };
            let context = cx
                .admit_http_command(Self::policy(Operation::Mutation, credential))
                .unwrap();
            self.queue(cx, context, &selection);
        }
        fn on_response(&mut self, _: &mut Cx<'_, Option<Context>>, _: Option<Context>, _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, Option<Context>>, _: Option<Context>, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Option<Context>>, _: TimerId) {}
        fn guard_pending_placements(&self) -> bool {
            true
        }
        fn guard_order_mutations(&self) -> bool {
            true
        }
        fn capture_http_reconciliation(
            &self,
            _: &Option<Context>,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("sign-meta-fixture".into())
        }
        fn on_open(&mut self, _: &mut Cx<'_, Option<Context>>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Option<Context>>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Option<Context>>, _: SocketId, _: &str) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn engine(signer: Signer) -> Engine<Signer> {
        let mut e = crate::test_support::seeded_engine(signer);
        e.set_identities(Identities {
            primary: Some(identity("primary")),
            secondary: Some(identity("secondary")),
        })
        .unwrap();
        e.start(0.);
        e
    }
    fn place(e: &mut Engine<Signer>) {
        e.handle_command(
            1.,
            Command::PlaceOrders {
                command: 7,
                orders: vec![
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .unwrap();
    }
    fn drain(e: &mut Engine<Signer>) -> Vec<Action> {
        e.drain_actions().collect()
    }
    fn preparation(actions: &[Action]) -> Preparation {
        actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("real Prepare")
    }
    /// Prepare, then return the host-visible signing task.
    fn sign_task(e: &mut Engine<Signer>, token: Preparation, now: f64) -> RequestId {
        e.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        sign_action(&drain(e)).expect("host-visible Action::Sign")
    }
    fn sign_action(actions: &[Action]) -> Option<RequestId> {
        actions.iter().find_map(|a| match a {
            // Exhaustive: the host-facing Sign shape is pinned unchanged.
            Action::Sign {
                attempt,
                work,
                timeout_ms,
            } if !work.operations.is_empty() && *timeout_ms > 0 => Some(*attempt),
            _ => None,
        })
    }
    fn signed(e: &mut Engine<Signer>, task: RequestId, now: f64) -> Vec<Action> {
        e.handle(
            now,
            Event::Signed {
                attempt: task,
                result: Ok(crate::signing::SignResult::new(
                    vec![b"value".to_vec()],
                    crate::signing::SignReceipt {
                        nonce: None,
                        stamp_ms: Some(now),
                        expiry_ms: Some(now + 3_000.),
                    },
                )),
            },
        );
        drain(e)
    }
    fn mutation() -> (Engine<Signer>, Preparation, RequestId) {
        let mut e = engine(Signer::default());
        drain(&mut e);
        place(&mut e);
        let token = preparation(&drain(&mut e));
        let task = sign_task(&mut e, token, 1.5);
        (e, token, task)
    }

    #[test]
    fn a_selected_managed_mutation_reports_its_attempt_deadline_slot_and_role() {
        let (e, token, task) = mutation();
        let context = e.venue.context.clone().unwrap();
        assert_eq!(
            e.sign_meta(task),
            Some(SignMeta {
                task,
                owner: SignOwner::Managed {
                    request: token.attempt().request,
                    generation: token.attempt().generation,
                },
                deadline_ms: context.deadline(),
                slot: crate::CredentialSlot::Primary,
                role: CredentialRole::Mutation,
            })
        );
    }
    #[test]
    fn a_primary_read_keeps_its_exact_reason_and_scope() {
        let role = CredentialRole::PrimaryRead {
            reason: PrimaryReason::PostUnknownOrderRead,
            scope: None,
        };
        let mut e = engine(Signer {
            mode: Mode::Read,
            role: Some(role),
            secondary: true,
            ..Default::default()
        });
        let token = preparation(&drain(&mut e));
        let task = sign_task(&mut e, token, 0.5);
        let meta = e.sign_meta(task);
        assert!(meta.is_some(), "selected read metadata");
        let meta = meta.unwrap();
        assert_eq!(meta.role, role);
        assert_eq!(meta.slot, crate::CredentialSlot::Primary);
    }
    #[test]
    fn a_secondary_read_reports_the_secondary_slot() {
        let role = CredentialRole::Read { scope: None };
        let mut e = engine(Signer {
            mode: Mode::Read,
            role: Some(role),
            secondary: true,
            ..Default::default()
        });
        let token = preparation(&drain(&mut e));
        let task = sign_task(&mut e, token, 0.5);
        let meta = e.sign_meta(task);
        assert!(meta.is_some(), "selected read metadata");
        let meta = meta.unwrap();
        assert_eq!((meta.slot, meta.role), (crate::CredentialSlot::Secondary, role));
    }
    #[test]
    fn a_session_login_reports_its_session_generation_and_session_auth() {
        let mut e = engine(Signer {
            mode: Mode::Session,
            role: Some(CredentialRole::SessionAuth),
            ..Default::default()
        });
        let token = preparation(&drain(&mut e));
        let task = sign_task(&mut e, token, 0.5);
        let generation = match e.venue.session.as_ref().unwrap().owner() {
            crate::request_context::Owner::Session { generation } => generation,
            other => panic!("fixture session owner {other:?}"),
        };
        let meta = e.sign_meta(task);
        assert!(meta.is_some(), "session login metadata");
        let meta = meta.unwrap();
        assert_eq!(meta.owner, SignOwner::Session { generation });
        assert_eq!(meta.role, CredentialRole::SessionAuth);
    }
    #[test]
    fn a_stale_re_sign_moves_metadata_to_the_new_task_with_the_same_owner_and_deadline() {
        let (mut e, _, first) = mutation();
        let before = e.sign_meta(first);
        assert!(before.is_some(), "first task metadata");
        let before = before.unwrap();
        // Past the step's 3 s freshness: the engine re-prepares a new task.
        let actions = signed(&mut e, first, 5.);
        let token = preparation(&actions);
        let second = sign_task(&mut e, token, 5.5);
        assert_ne!(first, second);
        assert_eq!(e.sign_meta(first), None, "a superseded task has no metadata");
        let after = e.sign_meta(second);
        assert!(after.is_some(), "current task metadata");
        let after = after.unwrap();
        assert_eq!((after.owner, after.deadline_ms), (before.owner, before.deadline_ms));
    }
    #[test]
    fn a_completed_task_has_no_metadata() {
        let (mut e, _, task) = mutation();
        assert!(e.sign_meta(task).is_some(), "prepared task metadata");
        signed(&mut e, task, 2.);
        assert_eq!(e.sign_meta(task), None);
    }
    #[test]
    fn plain_unmanaged_and_unknown_tasks_have_no_metadata() {
        let mut plain = engine(Signer {
            mode: Mode::Plain,
            ..Default::default()
        });
        drain(&mut plain);
        place(&mut plain);
        let token = preparation(&drain(&mut plain));
        let task = sign_task(&mut plain, token, 1.5);
        assert_eq!(plain.sign_meta(task), None, "no captured role");
        assert_eq!(plain.sign_meta(RequestId(u64::MAX >> 12)), None);
        let mut unmanaged = engine(Signer {
            mode: Mode::Unmanaged,
            role: Some(CredentialRole::Mutation),
            ..Default::default()
        });
        let task = sign_action(&drain(&mut unmanaged)).expect("non-managed Sign");
        assert_eq!(unmanaged.sign_meta(task), None, "no owner: v1 only");
    }
    #[test]
    fn a_selection_slot_that_differs_from_the_context_credential_is_refused() {
        let mut e = engine(Signer {
            mode: Mode::Mismatch,
            role: Some(CredentialRole::Read { scope: None }),
            secondary: true,
            ..Default::default()
        });
        drain(&mut e);
        place(&mut e);
        assert_eq!(e.venue.queued, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.actions_before, e.venue.actions_after, "nothing queued");
    }
}
