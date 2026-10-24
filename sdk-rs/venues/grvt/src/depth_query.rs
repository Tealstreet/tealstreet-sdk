//! Independent public MDG snapshots do not publish to the streaming book.
use crate::{Grvt, Job, Pending, wire};
use safe_cex_engine::request_context::plan::{CallBound, RetryWindow, Slot, StepBound};
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, Operation, Plan, PlanPolicy, PreflightPolicy,
    PreflightPurpose, RetryEvidence, RetryPolicy,
};
use safe_cex_engine::{Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(d).map(Some)
}
#[derive(Deserialize)]
struct Refusal<'a> {
    #[serde(
        default,
        borrow,
        rename = "c",
        alias = "code",
        deserialize_with = "present"
    )]
    code: Option<&'a RawValue>,
    #[serde(borrow, rename = "m", alias = "message")]
    message: Option<&'a RawValue>,
    #[serde(borrow, rename = "e", alias = "error")]
    error: Option<&'a RawValue>,
}
impl Refusal<'_> {
    fn code(&self) -> Option<wire::Text<'_>> {
        self.code
            .and_then(|raw| serde_json::from_str(raw.get()).ok())
    }
    fn message(&self) -> Option<std::borrow::Cow<'_, str>> {
        self.message
            .and_then(|raw| serde_json::from_str::<std::borrow::Cow<'_, str>>(raw.get()).ok())
            .filter(|s| !s.is_empty())
    }
}
#[derive(Deserialize)]
struct ResultEnvelope<'a> {
    #[serde(borrow, rename = "r", alias = "result")]
    result: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Snapshot<'a> {
    #[serde(borrow, rename = "b", alias = "bids")]
    bids: Option<&'a RawValue>,
    #[serde(borrow, rename = "a", alias = "asks")]
    asks: Option<&'a RawValue>,
}
#[derive(Serialize)]
struct Level {
    price: f64,
    amount: f64,
    total: f64,
}
#[derive(Default, Serialize)]
struct Output {
    bids: Vec<Level>,
    asks: Vec<Level>,
}
pub(crate) fn failure(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
fn invalid() -> VenueError {
    failure("Invalid GRVT order-book snapshot", ErrorKind::Internal)
}
fn object<'a, T: Deserialize<'a>>(raw: &'a str) -> Result<T, VenueError> {
    if !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn levels(raw: Option<&RawValue>) -> Result<Vec<Level>, VenueError> {
    let rows = raw
        .map(|v| serde_json::from_str::<Vec<&RawValue>>(v.get()))
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    let mut total = 0.0;
    rows.into_iter()
        .map(|row| {
            let row = object::<wire::Level>(row.get())?;
            let (price, amount) = (row.price.0, row.size.0);
            total += amount;
            if price <= 0.0 || amount < 0.0 || !total.is_finite() {
                return Err(invalid());
            }
            Ok(Level {
                price,
                amount,
                total,
            })
        })
        .collect()
}
fn decode(response: Response<'_>) -> Result<String, VenueError> {
    let envelope = object::<Refusal<'_>>(response.body);
    let root = envelope.as_ref().ok();
    let nested = root
        .and_then(|e| e.error)
        .and_then(|raw| object::<Refusal<'_>>(raw.get()).ok());
    let root_code = root.and_then(Refusal::code);
    let nested_code = nested.as_ref().and_then(Refusal::code);
    let nonzero = |code: &&wire::Text<'_>| !code.is_empty() && &***code != "0";
    let native = nested_code
        .as_ref()
        .filter(nonzero)
        .or_else(|| root_code.as_ref().filter(nonzero))
        .or(nested_code.as_ref())
        .or(root_code.as_ref())
        .map(|v| &**v)
        .unwrap_or_default();
    let refused = !native.is_empty() && native != "0" || root.is_some_and(|e| e.error.is_some());
    if !(200..300).contains(&response.status) || refused {
        let kind = match response.status {
            429 => ErrorKind::RateLimited,
            408 | 504 => ErrorKind::Timeout,
            200..300 if native == "1006" => ErrorKind::RateLimited,
            _ => ErrorKind::VenueUnavailable,
        };
        let mut error = failure(
            format!("GRVT order-book query HTTP {}", response.status),
            kind,
        );
        if !native.is_empty() {
            error = error.venue_code(native);
        }
        if let Some(message) = nested
            .as_ref()
            .and_then(Refusal::message)
            .or_else(|| root.and_then(Refusal::message))
        {
            error.message.push_str(": ");
            error.message.push_str(&message);
            error = error.venue_message(message.into_owned());
        }
        if kind == ErrorKind::RateLimited
            && let Some(seconds) = response
                .header("retry-after")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n >= 0.0)
        {
            error = error.retry_after_ms(
                (seconds * 1000.0).min(safe_cex_engine::startup::MAX_RATE_LIMIT_PAUSE_MS),
            );
        }
        return Err(error);
    }
    let malformed_code = root.is_some_and(|e| e.code.is_some()) && root_code.is_none();
    envelope
        .and_then(|_| {
            if malformed_code {
                Err(invalid())
            } else {
                object::<ResultEnvelope<'_>>(response.body)
            }
        })
        .and_then(|e| match e.result {
            None => Ok(Output::default()),
            Some(raw) => {
                let book = object::<Snapshot<'_>>(raw.get())?;
                Ok(Output {
                    bids: levels(book.bids)?,
                    asks: levels(book.asks)?,
                })
            }
        })
        .and_then(|book| serde_json::to_string(&book).map_err(|_| invalid()))
}

pub(crate) struct Request {
    pub(crate) plan: Plan,
    slot: Slot,
    leg: Context,
    body: String,
}
impl Grvt {
    pub(crate) fn fetch_book_query(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
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
        let Some(market) = cx.store.market(symbol) else {
            let _ = cx.done_preflight(&preflight, r#"{"bids":[],"asks":[]}"#.into());
            return;
        };
        #[derive(Serialize)]
        struct Body<'a> {
            i: &'a str,
            d: u32,
        }
        let body = serde_json::to_string(&Body {
            i: &market.id,
            d: 50,
        })
        .expect("book request serializes");
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
        self.book_query_send(
            cx,
            Request {
                plan,
                slot,
                leg,
                body,
            },
        );
    }
    pub(crate) fn book_query_send(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let context = request.leg.clone();
        let pending = Pending {
            path: "/lite/v1/book",
            body: request.body.clone(),
            job: Job::BookQuery(Box::new(request)),
            attempts: 0,
            auth_resent: false,
            auth_generation: self.account.generation,
        };
        let body = Some(pending.body.clone());
        if let Err((pending, error)) = cx.request_managed_with_headers(
            context,
            Method::Post,
            format!("{}/lite/v1/book", self.mdg()),
            &[("Content-Type", "application/json")],
            body,
            pending,
        ) && let Job::BookQuery(request) = pending.job
        {
            let _ = cx.fail_plan(&request.plan, refusal(error));
        }
    }
    pub(crate) fn book_query_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if cx
            .validate_plan_delivery(&request.plan, &request.leg)
            .is_err()
        {
            return;
        }
        match decode(response) {
            Ok(result) => {
                let _ = cx.done_read_plan_after_data(
                    &request.plan,
                    &request.slot,
                    &request.leg,
                    result,
                );
            }
            Err(error) => {
                let _ = cx.fail_plan(&request.plan, error);
            }
        }
    }
    pub(crate) fn book_query_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: &str,
    ) {
        match cx.plan_retry_context(
            &request.plan,
            &request.slot,
            &request.leg,
            RetryEvidence::ReadFailure,
        ) {
            Ok(leg) => {
                request.leg = leg;
                self.book_query_send(cx, request);
            }
            Err(Error::Revoked) => {}
            Err(_) => {
                let _ = cx.fail_plan(&request.plan, failure(error, ErrorKind::Network));
            }
        }
    }
}
fn refusal(error: Error) -> VenueError {
    VenueError::new(error.to_string()).kind(error.kind())
}
