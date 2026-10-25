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
        if !subjects.is_empty()
            && let Ok(entitlement) = cx.mint_command_repair(preflight, subjects.clone())
        {
            covered.extend(subjects.iter().map(|s| (s.key.clone(), s.generation)));
            self.trading.repair.grants.push(Grant {
                symbol: symbol.into(),
                entitlement,
                subjects,
            });
        }
        covered
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
                    continue;
                }
            };
            let Some(identity) = self
                .credentials
                .as_ref()
                .filter(|identity| {
                    context
                        .credential()
                        .is_some_and(|credential| identity.owns(&credential.reference))
                })
                .cloned()
            else {
                unreachable!("repair core admits only the installed Primary")
            };
            for order in before {
                cx.store.set_order_status(&order.id, OrderStatus::Canceling);
            }
            self.account_write(cx);
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
            let recipe = crate::dispatch_signing::Http::new(
                identity,
                self.options.testnet,
                step.method,
                step.path.into(),
                step.query.clone(),
                None,
            );
            let url = format!("{}{}?{}", self.rest(), step.path, step.query);
            if let Err((Pending::Repair(pending), error)) = cx.request_signed_managed(
                context,
                None,
                step.method,
                url,
                None,
                Box::new(recipe),
                Pending::Repair(pending),
            ) {
                self.repair_failed(cx, pending, error.venue_error());
            }
            return;
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
        let decoded = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!("HTTP {}: {}", response.status, response.body));
            }
            let env: Envelope = serde_json::from_str(response.body)
                .map_err(|_| "Phemex malformed write acknowledgement".to_string())?;
            if env.error.is_some() || env.code.is_some_and(|code| code != 0) {
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
                    BatchItem::Native(unknown)
                }
            })
            .collect();
        let message = decoded
            .as_ref()
            .err()
            .cloned()
            .or_else(|| decoded.as_ref().ok().and_then(|ack| ack.problem.clone()));
        if cx
            .http_write_batch(
                &pending.context,
                &REFUSALS,
                ids.len(),
                &items,
                message
                    .as_deref()
                    .unwrap_or("Phemex unproven cancel acknowledgement"),
                None,
            )
            .is_err()
        {
            return;
        }
        let before = before.clone();
        if let Ok(ack) = &decoded
            && !ack.accepted.is_empty()
            && cx.validate_http_write_effects(&pending.context).is_ok()
        {
            self.apply_cancel_ack(cx, &before, ack);
        }
        if let Some(active) = &mut self.trading.repair.active {
            active.error = message.map(VenueError::new);
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
