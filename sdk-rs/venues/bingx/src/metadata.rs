//! FetchLeverage metadata: two serial native reads (leverage, then margin type)
//! under one engine-admitted ReadOnly Plan. Each read keeps BingX's one fresh
//! 20s signature retry; only validated DATA settles a call.
use super::{Account, metadata_error, response_error};
use crate::Pending;
use crate::account_wire::{Data, Leverage, MarginMode};
use crate::sign::{Credentials, Params, dispatch::Http};
use safe_cex_engine::engine::Window;
use safe_cex_engine::log::code;
use safe_cex_engine::metadata::{
    CredentialSubject, Fallback, NativeWork, Proposal, Rails, Snapshot, Token, WorkKey,
};
use safe_cex_engine::model::MarginType;
use safe_cex_engine::request_context::plan::{CallBound, RetryWindow, Slot, StepBound};
use safe_cex_engine::request_context::{
    Context, Credential, Error, Operation, Plan, PlanPolicy, Policy, PreflightPolicy,
    PreflightPurpose, RetryEvidence, RetryPolicy, RoutingIntent,
};
use safe_cex_engine::{CredentialSlot, Cx, ErrorKind, Level, Method, Response, VenueError};

const MARGIN_TYPE: &str = "/openApi/swap/v2/trade/marginType";
/// One native 20s attempt plus one fresh re-signed attempt per read: 40s per
/// call, 80s for the serial pair.
const READ: [CallBound<'static>; 1] = [CallBound {
    timeout_ms: 20_000,
    attempts: 2,
    retry_window: RetryWindow::FreshAttempt,
    after_refusal: &[],
}];
const STEPS: [StepBound<'static>; 2] = [
    StepBound {
        wait_ms: 0,
        calls: &READ,
    },
    StepBound {
        wait_ms: 0,
        calls: &READ,
    },
];

const STEP_WAITS: [u32; 2] = [STEPS[0].wait_ms, STEPS[1].wait_ms];
const CALLS_PER_STEP: [usize; 2] = [STEPS[0].calls.len(), STEPS[1].calls.len()];

/// One issued metadata read and the native fences its publication needs.
pub struct Leg {
    plan: Plan,
    slot: Slot,
    context: Context,
    symbol: String,
    native: String,
    token: Token,
    /// Present once the leverage read settled; selects the margin-type read.
    leverage: Option<(f64, f64)>,
    resent: bool,
}

fn credential(credentials: &Credentials) -> Credential {
    Credential {
        slot: if credentials.read.is_some() {
            CredentialSlot::Secondary
        } else {
            CredentialSlot::Primary
        },
        reference: credentials.identity(Method::Get).credential.clone(),
    }
}

fn refused() -> VenueError {
    metadata_error("BingX metadata read lifecycle is unavailable").kind(ErrorKind::Internal)
}

fn result((buy, sell): (f64, f64)) -> String {
    format!(r#"{{"buyLeverage":{buy},"sellLeverage":{sell}}}"#)
}

impl Account {
    /// Entry owns refusals; only equal live work can borrow the shared Plan.
    pub(super) fn start_metadata(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        symbol: String,
        publish: bool,
    ) {
        for (token, handle) in cx.store.metadata_executions() {
            if cx.validate_http_context(handle.context()).is_err() {
                cx.store.retire_metadata_work(&token);
            }
        }
        let native = cx.store.market(&symbol).map(|market| market.id.clone());
        let key = credentials
            .zip(native.as_ref())
            .map(|(keys, native)| work_key(keys, &symbol, native, publish));
        if let Some(key) = key.as_ref() {
            if let Some(handle) = cx.store.metadata_work(key) {
                let Ok(root) = cx.admit_http_command(read_policy(credentials.unwrap())) else {
                    return;
                };
                if let Err(error) = cx.join_execution(&handle, &root, "metadata-settings") {
                    let _ = cx.fail_context(&root, read_error(error));
                }
                return;
            }
            if !cx.store.metadata_work_space(&symbol) {
                if let Ok(root) = cx.admit_http_command(read_policy(credentials.unwrap())) {
                    let _ = cx.fail_context(&root, read_error(Error::Capacity));
                }
                return;
            }
        }
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::ReadOnly,
            read_credential: credentials.map(credential),
            write_credential: None,
        }) else {
            return;
        };
        let Some(credentials) = credentials else {
            let _ = cx.fail_preflight(
                &preflight,
                metadata_error("BingX metadata requires credentials"),
            );
            return;
        };
        let Some(native) = native else {
            let _ = cx.fail_preflight(&preflight, metadata_error("Unknown BingX market"));
            return;
        };
        let (plan, handle, _) = match cx.begin_planned_execution(
            &preflight,
            &STEPS,
            PlanPolicy {
                independent_calls: false,
                operation: Operation::Read,
                credential: Some(credential(credentials)),
                pacing: None,
                retry: RetryPolicy::ReadFailure,
            },
            Window::UntilTerminal,
            "metadata-settings",
        ) {
            Ok(admitted) => admitted,
            Err(error) => {
                let _ = cx.fail_preflight(&preflight, read_error(error));
                return;
            }
        };
        let token = match cx
            .store
            .bind_metadata_work(key.expect("validated native key"), handle)
        {
            Ok(token) => token,
            Err(error) => {
                let _ = cx.fail_plan(&plan, error);
                return;
            }
        };
        issue(
            cx,
            credentials,
            plan,
            Fences {
                symbol,
                native,
                token,
            },
            None,
        );
    }

    pub fn metadata_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: &Credentials,
        leg: Leg,
        response: Response<'_>,
    ) {
        // Released DATA authority precedes every decode, Store or command effect.
        if cx.validate_plan_delivery(&leg.plan, &leg.context).is_err() {
            return;
        }
        if !leg.resent && crate::signature_rejected(response.body) {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                "BingX rejected a request signature; re-signing it once",
            );
            match cx.plan_retry_context(
                &leg.plan,
                &leg.slot,
                &leg.context,
                RetryEvidence::ReadFailure,
            ) {
                Ok(context) => send(
                    cx,
                    credentials,
                    Leg {
                        context,
                        resent: true,
                        ..leg
                    },
                ),
                Err(_) => {
                    let error = response_error(response, false)
                        .unwrap_or_else(|| "BingX metadata signature retry is unavailable".into());
                    fail(cx, &leg, metadata_error(error));
                }
            }
            return;
        }
        if let Some(error) = response_error(response, false) {
            // A failing private read surfaces as an Error at the retried-failure cadence.
            cx.retried_failure(
                "bingx.metadata:private",
                format!("BingX private request failed: {error}"),
            );
            fail(cx, &leg, metadata_error(error));
            return;
        }
        let current = cx.store.metadata_eligible(&leg.token);
        let Some((buy, sell)) = leg.leverage else {
            let rails = serde_json::from_str::<Data<Leverage>>(response.body)
                .ok()
                .map(|p| p.data)
                .filter(|l| l.long_leverage > 0.0 && l.short_leverage > 0.0);
            let Some(rails) = rails else {
                fail(cx, &leg, metadata_error("Invalid BingX leverage metadata"));
                return;
            };
            let read = (rails.long_leverage, rails.short_leverage);
            if !current {
                answer_current(cx, &leg, read);
                return;
            }
            let Some(native) = cx.store.market(&leg.symbol).map(|m| m.id.clone()) else {
                fail(
                    cx,
                    &leg,
                    metadata_error("BingX market disappeared during metadata read"),
                );
                return;
            };
            if cx
                .finish_plan_call(&leg.plan, &leg.slot, &leg.context)
                .is_err()
            {
                return;
            }
            let Leg {
                plan,
                symbol,
                token,
                ..
            } = leg;
            issue(
                cx,
                credentials,
                plan,
                Fences {
                    symbol,
                    native,
                    token,
                },
                Some(read),
            );
            return;
        };
        if !current {
            answer_current(cx, &leg, (buy, sell));
            return;
        }
        let parsed = serde_json::from_str::<Data<MarginMode<'_>>>(response.body);
        let margin = match parsed.as_ref().map(|row| row.data.margin_type.as_ref()) {
            Ok("ISOLATED") => MarginType::Isolated,
            Ok("CROSSED") => MarginType::Cross,
            _ => {
                fail(cx, &leg, metadata_error("Invalid BingX margin metadata"));
                return;
            }
        };
        if cx
            .finish_plan_call(&leg.plan, &leg.slot, &leg.context)
            .is_err()
        {
            return;
        }
        let setting = safe_cex_engine::store::MarketSetting {
            leverage: buy,
            buy_leverage: Some(buy),
            sell_leverage: Some(sell),
            margin_type: Some(margin),
        };
        let proposal = Snapshot::new(setting).expect("validated native metadata");
        let Some(resolution) = cx
            .store
            .finish_metadata(&leg.token, Ok(Proposal::complete(proposal)))
        else {
            fail(cx, &leg, refused());
            return;
        };
        if resolution.published {
            self.bump_positions();
        }
        match resolution.into_result() {
            Ok(value) => {
                let _ = cx.done_plan(&leg.plan, result((value.buy, value.sell)));
            }
            Err(error) => {
                let _ = cx.fail_plan(&leg.plan, error);
            }
        }
    }
}

/// Provider, route, transport and timeout failures all settle the one Plan.
pub fn fail(cx: &mut Cx<'_, Pending>, leg: &Leg, error: impl Into<VenueError>) {
    cx.store.retire_metadata_work(&leg.token);
    let _ = cx.fail_plan(&leg.plan, error);
}

/// Native publication fences a read carries through both stages.
struct Fences {
    symbol: String,
    native: String,
    token: Token,
}

fn issue(
    cx: &mut Cx<'_, Pending>,
    credentials: &Credentials,
    plan: Plan,
    fences: Fences,
    leverage: Option<(f64, f64)>,
) {
    let slot = cx
        .plan_slots(&plan)
        .ok()
        .and_then(|slots| slots.into_iter().next());
    let Some(slot) = slot else {
        cx.store.retire_metadata_work(&fences.token);
        let _ = cx.fail_plan(&plan, refused());
        return;
    };
    let Ok(context) = cx.plan_leg_context(&plan, &slot) else {
        cx.store.retire_metadata_work(&fences.token);
        let _ = cx.fail_plan(&plan, refused());
        return;
    };
    let Fences {
        symbol,
        native,
        token,
    } = fences;
    send(
        cx,
        credentials,
        Leg {
            plan,
            slot,
            context,
            symbol,
            native,
            token,
            leverage,
            resent: false,
        },
    );
}

fn send(cx: &mut Cx<'_, Pending>, credentials: &Credentials, leg: Leg) {
    let path = if leg.leverage.is_none() {
        crate::paths::LEVERAGE
    } else {
        MARGIN_TYPE
    };
    let recipe = Http::new(
        credentials.identity(Method::Get).clone(),
        Method::Get,
        path,
        Params::new().str("symbol", &leg.native),
    );
    let (url, _, body) = recipe.unsigned();
    let context = leg.context.clone();
    if let Err((pending, _)) = cx.request_signed_managed(
        context,
        None,
        Method::Get,
        url,
        body,
        Box::new(recipe),
        Pending::Metadata(Box::new(leg)),
    ) && let Pending::Metadata(leg) = pending
    {
        fail(cx, &leg, refused());
    }
}

/// Native current-settings answer after a superseded read: the newer settings
/// win, with this read's rails only where the store has none. The shared
/// ReadOnly early terminal settles this call and retires the unissued remainder.
fn answer_current(cx: &mut Cx<'_, Pending>, leg: &Leg, read: (f64, f64)) {
    let rails = Rails::new(read.0, read.1).expect("validated native rails");
    let Some(resolution) = cx
        .store
        .finish_metadata(&leg.token, Ok(Proposal::rails(rails)))
    else {
        fail(cx, leg, refused());
        return;
    };
    match resolution.into_result() {
        Ok(value) => {
            let _ = cx.done_read_plan_after_data(
                &leg.plan,
                &leg.slot,
                &leg.context,
                result((value.buy, value.sell)),
            );
        }
        Err(error) => {
            let _ = cx.fail_plan(&leg.plan, error);
        }
    }
}

fn read_error(error: Error) -> VenueError {
    VenueError::new(error.to_string()).kind(error.kind())
}
fn read_policy(credentials: &Credentials) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: Some(credential(credentials)),
        timeout_ms: 80_000,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}
fn work_key(credentials: &Credentials, symbol: &str, native: &str, publish: bool) -> WorkKey {
    let selected = credential(credentials);
    WorkKey {
        symbol: symbol.into(),
        subject: CredentialSubject::ConfiguredPrimary(credentials.primary.clone()),
        slot: selected.slot,
        identity: credentials.identity(Method::Get).clone(),
        intent: if selected.slot == CredentialSlot::Secondary {
            RoutingIntent::Secondary
        } else {
            RoutingIntent::Primary
        },
        operation: Operation::Read,
        native: native.into(),
        work: NativeWork {
            kind: "metadata-settings",
            endpoints: &[crate::paths::LEVERAGE, MARGIN_TYPE],
            step_waits_ms: &STEP_WAITS,
            calls_per_step: &CALLS_PER_STEP,
            root_timeout_ms: 80_000,
            call_timeout_ms: READ[0].timeout_ms,
            attempts: READ[0].attempts,
            retry_window: READ[0].retry_window,
            retry: RetryPolicy::ReadFailure,
        },
        publish,
        fallback: Fallback::IndependentRails,
    }
}
