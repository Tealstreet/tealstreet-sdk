//! Native trading declarations and ACK facts; the engine owns settlement.
use super::*;
use safe_cex_engine::classify::{
    BatchItem, CodeTable, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::request_context::{
    Credential, Error, Operation, PlanPolicy, Policy, Preflight, PreflightPolicy, PreflightPurpose,
    RetryPolicy, WriteResult,
    plan::{CallBound, RetryWindow, StepBound},
};

// WEEX common error reference: https://www.weex.com/help/articles/52122274060569
// Only explicit validation/authentication refusals prove pre-execution denial.
const TABLE: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(429),
            verdict: Verdict::Unknown(Some(ErrorKind::RateLimited)),
            only: None,
        },
        Row {
            key: Key::Int(40006),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(40009),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(40012),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(40014),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(40018),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(42002),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
    ],
    success: &[Key::Int(0), Key::Int(200)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

pub(super) enum Entry {
    Preflight(Preflight, Credential),
    Terminal(Context),
}
impl Entry {
    pub(super) fn begin(
        cx: &mut Cx<'_, Pending>,
        identity: Option<&safe_cex_engine::signing::Identity>,
    ) -> Option<Self> {
        if let Some(identity) = identity {
            let credential = Credential {
                slot: safe_cex_engine::CredentialSlot::Primary,
                reference: identity.credential.clone(),
            };
            cx.begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential.clone()),
            })
            .ok()
            .map(|owner| Self::Preflight(owner, credential))
        } else {
            cx.admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .ok()
            .map(Self::Terminal)
        }
    }
    pub(super) fn reject(&self, cx: &mut Cx<'_, Pending>, error: impl Into<VenueError>) {
        let error = error.into();
        match self {
            Self::Preflight(owner, _) => {
                let _ = cx.fail_preflight(owner, error);
            }
            Self::Terminal(owner) => {
                let _ = cx.fail_context(owner, error);
            }
        }
    }
    pub(super) fn empty(self, cx: &mut Cx<'_, Pending>) {
        match self {
            Self::Preflight(owner, _) => {
                let _ = cx.done_empty_plan(&owner, "[]".into());
            }
            Self::Terminal(owner) => {
                let _ = cx.done_context(&owner, "[]".into());
            }
        }
    }
    pub(super) fn admit(self, cx: &mut Cx<'_, Pending>, count: usize) -> Option<Plan> {
        let Self::Preflight(owner, credential) = &self else {
            self.reject(
                cx,
                not_sent("WEEX private credentials required").kind(ErrorKind::AuthInvalid),
            );
            return None;
        };
        let calls = [CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        }];
        let steps = (0..count)
            .map(|_| StepBound {
                wait_ms: 0,
                calls: &calls,
            })
            .collect::<Vec<_>>();
        match cx.admit_plan(
            owner,
            &steps,
            PlanPolicy {
                independent_calls: false,
                operation: Operation::Mutation,
                credential: Some(credential.clone()),
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(plan) => Some(plan),
            Err(error) => {
                self.reject(cx, error.venue_error());
                None
            }
        }
    }
}

fn code(value: &Code) -> NativeCode<'_> {
    match value {
        Code::Number(value) => NativeCode::Int(*value),
        Code::Text(value) => safe_cex_engine::classify::decode_int(value)
            .map(NativeCode::Int)
            .unwrap_or(NativeCode::Text(value)),
    }
}
fn row_native(row: &ResultRow) -> Native<'_> {
    Native {
        code: safe_cex_engine::classify::decode_int(&row.error_code)
            .map(NativeCode::Int)
            .unwrap_or_else(|| {
                if row.error_code.is_empty() {
                    NativeCode::None
                } else {
                    NativeCode::Text(&row.error_code)
                }
            }),
        message: &row.error_message,
    }
}
fn project(result: WriteResult) -> Result<Vec<String>, VenueError> {
    match result {
        WriteResult::Accepted { order_ids } => Ok(order_ids),
        WriteResult::Rejected(error)
        | WriteResult::KnownPartial(error)
        | WriteResult::PartialUnknown(error)
        | WriteResult::Failed(error) => Err(error),
    }
}
fn failure(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    native: Option<&Native<'_>>,
    message: String,
) -> Result<Result<Vec<String>, VenueError>, Error> {
    let fact = cx.classify_write(context, &TABLE, native, message)?;
    let error = fact.error().clone();
    cx.http_write_fact(
        context,
        WriteFact::Failed {
            fact,
            known_effects: false,
        },
        None,
    )?;
    Ok(Err(error))
}
pub(super) struct Recorded {
    pub result: Result<Vec<String>, VenueError>,
    pub publish: bool,
}
pub(super) fn record(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    call: &Call,
    response: Response<'_>,
) -> Result<Recorded, Error> {
    #[derive(Deserialize)]
    struct Envelope {
        code: Option<Code>,
        #[serde(default)]
        msg: String,
        success: Option<bool>,
        #[serde(rename = "orderId")]
        order_id: Option<Id>,
        #[serde(rename = "algoId")]
        algo_id: Option<Id>,
    }
    let envelope = serde_json::from_str::<Envelope>(response.body).ok();
    if response.status >= 400
        || envelope
            .as_ref()
            .and_then(|e| e.code.as_ref())
            .is_some_and(|c| !c.success())
    {
        if envelope
            .as_ref()
            .is_some_and(|e| e.success == Some(true) || e.order_id.is_some() || e.algo_id.is_some())
        {
            return unproven(
                cx,
                context,
                call,
                response.body,
                "WEEX contradictory acknowledgement",
            );
        }
        let native = envelope.as_ref().and_then(|e| {
            e.code.as_ref().map(|value| Native {
                code: code(value),
                message: &e.msg,
            })
        });
        return failure(
            cx,
            context,
            native.as_ref(),
            format!("HTTP {}: {}", response.status, response.body),
        )
        .map(|result| Recorded {
            result,
            publish: false,
        });
    }
    let decoded = match call {
        Call::Place {
            orders,
            batch,
            protective,
            ..
        } => place_rows(orders, *batch, *protective, response.body).map(|rows| {
            let mut ordered = Vec::new();
            for (index, order) in orders.iter().enumerate() {
                let row = if *protective || !*batch {
                    &rows[index]
                } else {
                    rows.iter()
                        .find(|row| row.client() == order.client_order_id.as_deref())
                        .expect("validated native correlation")
                };
                ordered.push(row);
            }
            record_rows(
                cx,
                context,
                orders.len(),
                &ordered,
                true,
                "WEEX order response",
            )
        }),
        Call::Cancel { native, all, .. } => cancel_rows(native, *all, response.body).map(|rows| {
            if rows.is_empty() {
                cx.http_write_fact(
                    context,
                    WriteFact::Accepted {
                        order_ids: Vec::new(),
                    },
                    None,
                )
                .map(|_| Ok(Vec::new()))
            } else {
                record_rows(
                    cx,
                    context,
                    rows.len(),
                    &rows.iter().map(|(_, row)| row).collect::<Vec<_>>(),
                    false,
                    "WEEX cancellation response",
                )
            }
        }),
        Call::Amend { before, .. } => amend_row(before, response.body).map(|row| {
            if row.success && !contradictory(&row) {
                let ids = vec![before.id.clone()];
                cx.http_write_fact(
                    context,
                    WriteFact::Accepted {
                        order_ids: ids.clone(),
                    },
                    None,
                )
                .map(|_| Ok(ids))
            } else {
                failure(cx, context, Some(&row_native(&row)), row.error())
            }
        }),
        Call::Leverage { symbol, .. } => metadata(response.body, Some(symbol)).map(|_| {
            cx.http_write_fact(
                context,
                WriteFact::Accepted {
                    order_ids: Vec::new(),
                },
                None,
            )
            .map(|_| Ok(Vec::new()))
        }),
        Call::Margin { .. } | Call::Mode { .. } => metadata(response.body, None).map(|_| {
            cx.http_write_fact(
                context,
                WriteFact::Accepted {
                    order_ids: Vec::new(),
                },
                None,
            )
            .map(|_| Ok(Vec::new()))
        }),
    };
    match decoded {
        Ok(result) => result.map(|result| Recorded {
            result,
            publish: true,
        }),
        Err(error) => unproven(cx, context, call, response.body, &error.message),
    }
}

fn record_rows(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    expected: usize,
    rows: &[&ResultRow],
    placement: bool,
    label: &str,
) -> Result<Result<Vec<String>, VenueError>, Error> {
    if rows.iter().any(|row| contradictory(row)) {
        return Err(Error::Invalid);
    }
    let ids = rows
        .iter()
        .map(|row| {
            if row.success {
                row.native()
                    .map(Id::id)
                    .transpose()
                    .map(|id| id.unwrap_or_default())
            } else {
                Ok(String::new())
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Invalid)?;
    let items = rows
        .iter()
        .zip(&ids)
        .map(|(row, id)| {
            if row.success {
                BatchItem::Accepted(id)
            } else {
                BatchItem::Native(row_native(row))
            }
        })
        .collect::<Vec<_>>();
    let message = rows
        .iter()
        .filter(|row| !row.success)
        .map(|row| format!("{}: {}", row.client().unwrap_or(label), row.error()))
        .collect::<Vec<_>>()
        .join("; ");
    let mut result =
        project(cx.http_write_batch(context, &TABLE, expected, &items, message, None)?);
    if !placement && let Ok(ids) = &mut result {
        ids.clear();
    }
    Ok(result)
}

fn amend_row(before: &Order, body: &str) -> Result<ResultRow, VenueError> {
    let row: ResultRow = serde_json::from_str(body)
        .map_err(|_| unknown("WEEX malformed TP/SL amendment response"))?;
    if contradictory(&row) {
        return Err(unknown("WEEX contradictory amendment acknowledgement"));
    }
    if let Some(id) = row.native()
        && id.id()? != before.id
    {
        return Err(unknown(
            "WEEX amendment acknowledgement names another order",
        ));
    }
    Ok(row)
}

pub(super) fn contradictory(row: &ResultRow) -> bool {
    !row.aliases_consistent()
        || row.success
            && !row.error_code.is_empty()
            && !matches!(
                safe_cex_engine::classify::decode_int(&row.error_code),
                Some(0 | 200)
            )
}

fn unproven(
    cx: &mut Cx<'_, Pending>,
    context: &Context,
    call: &Call,
    body: &str,
    message: &str,
) -> Result<Recorded, Error> {
    cx.http_write_unproven_result(context, |evidence| {
        evidence.venue_message(message);
        capture_ids(call, body, evidence);
    })?;
    Ok(Recorded {
        result: Err(unknown(message)),
        publish: false,
    })
}

pub(super) fn capture_ids(
    call: &Call,
    body: &str,
    evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
) {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct IdentityRow {
        order_id: Option<Id>,
        algo_id: Option<Id>,
        client_order_id: Option<String>,
        client_algo_id: Option<String>,
    }
    let rows = if body.trim_start().starts_with('[') {
        serde_json::from_str::<Vec<&serde_json::value::RawValue>>(body)
    } else {
        serde_json::from_str::<&serde_json::value::RawValue>(body).map(|row| vec![row])
    };
    let Ok(rows) = rows else {
        return;
    };
    let mut seen = HashSet::new();
    for row in rows {
        let Ok(row) = serde_json::from_str::<IdentityRow>(row.get()) else {
            continue;
        };
        for (id, client) in [
            (
                row.order_id.as_ref(),
                row.client_order_id.as_ref().or(row.client_algo_id.as_ref()),
            ),
            (
                row.algo_id.as_ref(),
                row.client_algo_id.as_ref().or(row.client_order_id.as_ref()),
            ),
        ] {
            let Some(id) = id.and_then(|id| id.id().ok()) else {
                continue;
            };
            let correlated = match call {
                Call::Place {
                    orders,
                    batch,
                    protective,
                    ..
                } => match client {
                    Some(client) => orders
                        .iter()
                        .any(|order| order.client_order_id.as_ref() == Some(client)),
                    None => !batch || *protective,
                },
                Call::Cancel {
                    native,
                    before,
                    all,
                } => {
                    native.contains(&id)
                        || (*all
                            && before
                                .iter()
                                .any(|order| order.id.split("__").next() == Some(id.as_str())))
                }
                Call::Amend { before, .. } => id == before.id,
                _ => false,
            };
            if correlated && seen.insert(id.clone()) && !evidence.order_id(&id) {
                return;
            }
        }
    }
}

pub(super) fn rate_limited(cx: &Cx<'_, Pending>, context: &Context) -> bool {
    cx.classify_write(context, &TABLE, None, "WEEX write rate deferral")
        .is_ok_and(|fact| fact.error().kind == Some(ErrorKind::RateLimited))
}
