//! One unsigned candle call under the original caller receipt and retry budget.
use crate::{Grvt, Job, Pending, interval, wire};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Interval};
use safe_cex_engine::request_context::plan::{CallBound, RetryWindow, Slot, StepBound};
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, Operation, Plan, PlanPolicy, PreflightPolicy,
    PreflightPurpose, RetryEvidence, RetryPolicy,
};
use safe_cex_engine::{Cx, Method, Response, VenueError};
use serde::Serialize;

pub(crate) struct Request {
    pub(crate) plan: Plan,
    slot: Slot,
    pub(crate) leg: Context,
    body: String,
    rows: Vec<Candle>,
    failed: bool,
}
impl Request {
    pub(crate) fn item(&mut self, cx: &mut Cx<'_, Pending>, item: &str) {
        if cx.validate_plan_delivery(&self.plan, &self.leg).is_err() {
            return;
        }
        let candle = serde_json::from_str::<wire::Feed<'_>>(item)
            .ok()
            .and_then(|feed| feed.candle());
        if let Some(candle) = candle {
            self.rows.push(candle);
        } else {
            self.failed = true;
        }
    }
}

impl Grvt {
    pub(crate) fn fetch_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        timeframe: Interval,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        let preflight = match cx.begin_preflight_with_command_wait(PreflightPolicy {
            purpose: PreflightPurpose::ReadOnly,
            read_credential: None,
            write_credential: None,
        }) {
            Ok(preflight) => preflight,
            Err(CommandEntryError::Rejected(_)) => return,
            Err(CommandEntryError::Revoked) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
        };
        let Some(interval) = interval(timeframe) else {
            let _ = cx.fail_preflight(&preflight, timeframe.unsupported("GRVT"));
            return;
        };
        let Some(market) = cx.store.market(&symbol) else {
            let _ = cx.fail_preflight(&preflight, "Unknown GRVT market");
            return;
        };
        #[derive(Serialize)]
        struct Body<'a> {
            i: &'a str,
            t: &'static str,
            #[serde(rename = "i1")]
            interval: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            st: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            et: Option<String>,
            l: u32,
        }
        if from_ms
            .into_iter()
            .chain(to_ms)
            .any(|v| !v.is_finite() || v < 0.)
        {
            let _ = cx.fail_preflight(&preflight, "Invalid GRVT candle bounds");
            return;
        }
        let body = serde_json::to_string(&Body {
            i: &market.id,
            t: "TRADE",
            interval,
            st: from_ms.map(|v| format!("{:.0}", v * 1e6)),
            et: to_ms.map(|v| format!("{:.0}", v * 1e6)),
            l: limit.unwrap_or(500).clamp(1, 1000),
        })
        .expect("candle request");
        let calls = [CallBound {
            timeout_ms: 20_000,
            attempts: 6,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps = [StepBound {
            wait_ms: 0,
            calls: &calls,
        }];
        let plan = match cx.admit_plan(
            &preflight,
            &steps,
            PlanPolicy {
                operation: Operation::Read,
                credential: None,
                pacing: None,
                retry: RetryPolicy::ReadFailure,
            },
        ) {
            Ok(plan) => plan,
            Err(error) => {
                let _ = cx.fail_preflight(&preflight, refusal(error));
                return;
            }
        };
        let slot = match cx
            .plan_slots(&plan)
            .and_then(|mut slots| slots.pop().ok_or(Error::State))
        {
            Ok(slot) => slot,
            Err(error) => {
                let _ = cx.fail_plan(&plan, refusal(error));
                return;
            }
        };
        let leg = match cx.plan_leg_context(&plan, &slot) {
            Ok(leg) => leg,
            Err(error) => {
                let _ = cx.fail_plan(&plan, refusal(error));
                return;
            }
        };
        self.candle_send(
            cx,
            Request {
                plan,
                slot,
                leg,
                body,
                rows: vec![],
                failed: false,
            },
        );
    }

    pub(crate) fn candle_send(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let context = request.leg.clone();
        let pending = Pending {
            path: "/lite/v1/kline",
            body: request.body.clone(),
            job: Job::Candles(Box::new(request)),
            attempts: 0,
            auth_resent: false,
            auth_generation: self.account.generation,
        };
        let body = Some(pending.body.clone());
        match cx.request_managed_with_headers(
            context,
            Method::Post,
            format!("{}/lite/v1/kline", self.mdg()),
            &[("Content-Type", "application/json")],
            body,
            pending,
        ) {
            Ok(id) => cx.stream_response(id, ArrayAt::Envelope(&[&["r"], &["result"]])),
            Err((pending, error)) => {
                if let Job::Candles(request) = pending.job {
                    let _ = cx.fail_plan(&request.plan, refusal(error));
                }
            }
        }
    }

    pub(crate) fn candle_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if cx
            .validate_plan_delivery(&request.plan, &request.leg)
            .is_err()
        {
            return;
        }
        if wire::check(&response).is_err() {
            let _ = cx.fail_plan(&request.plan, "GRVT candle read failed");
        } else if request.failed {
            let _ = cx.fail_plan(&request.plan, "Invalid GRVT candles");
        } else {
            request
                .rows
                .sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
            match serde_json::to_string(&request.rows) {
                Ok(result) => {
                    let _ = cx.done_read_plan_after_data(
                        &request.plan,
                        &request.slot,
                        &request.leg,
                        result,
                    );
                }
                Err(error) => {
                    let _ = cx.fail_plan(&request.plan, error.to_string());
                }
            }
        }
    }

    pub(crate) fn candle_failed(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        match cx.plan_retry_context(
            &request.plan,
            &request.slot,
            &request.leg,
            RetryEvidence::ReadFailure,
        ) {
            Ok(leg) => {
                request.leg = leg;
                request.rows.clear();
                request.failed = false;
                self.candle_send(cx, request);
            }
            Err(Error::Revoked) => {}
            Err(_) => {
                let _ = cx.fail_plan(&request.plan, "GRVT candle read failed");
            }
        }
    }
}

fn refusal(error: Error) -> VenueError {
    VenueError::new(error.to_string()).kind(error.kind())
}
