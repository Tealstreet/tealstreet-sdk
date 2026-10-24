//! FetchLeverage metadata: two serial native reads (leverage, then margin type)
//! under one engine-admitted ReadOnly Plan. Each read keeps BingX's one fresh
//! 20s signature retry; only validated DATA settles a call.
use super::{Account, metadata_error, response_error};
use crate::Pending;
use crate::account_wire::{Data, Leverage, MarginMode};
use crate::sign::{Credentials, Params, dispatch::Http};
use safe_cex_engine::log::code;
use safe_cex_engine::model::MarginType;
use safe_cex_engine::request_context::plan::{CallBound, RetryWindow, Slot, StepBound};
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
    RetryEvidence, RetryPolicy,
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

/// One issued metadata read and the native fences its publication needs.
pub struct Leg {
    plan: Plan,
    slot: Slot,
    context: Context,
    symbol: String,
    native: String,
    revision: u64,
    generation: u64,
    publish: bool,
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
    /// Admit the paired read once the native entry checks passed.
    pub(super) fn start_metadata(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: &Credentials,
        symbol: String,
        native: String,
        publish: bool,
    ) {
        let generation = self.metadata_generations.entry(symbol.clone()).or_default();
        *generation = generation.wrapping_add(1);
        let generation = *generation;
        let credential = credential(credentials);
        // Entry refusal is engine-owned: a rejection already published this
        // command's failure, and a revoked loan grants no terminal authority.
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::ReadOnly,
            read_credential: Some(credential.clone()),
            write_credential: None,
        }) else {
            return;
        };
        let plan = match cx.admit_plan(
            &preflight,
            &STEPS,
            PlanPolicy {
                operation: Operation::Read,
                credential: Some(credential),
                pacing: None,
                retry: RetryPolicy::ReadFailure,
            },
        ) {
            Ok(plan) => plan,
            Err(_) => {
                let _ = cx.fail_preflight(&preflight, refused());
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
                revision: self.settings_revision,
                generation,
                publish,
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
                    let _ = cx.fail_plan(&leg.plan, metadata_error(error));
                }
            }
            return;
        }
        if let Some(error) = response_error(response, false) {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                format!("BingX private request failed: {error}"),
            );
            let _ = cx.fail_plan(&leg.plan, metadata_error(error));
            return;
        }
        let current = leg.publish
            && leg.revision == self.settings_revision
            && self.metadata_generations.get(&leg.symbol) == Some(&leg.generation);
        let Some((buy, sell)) = leg.leverage else {
            let rails = serde_json::from_str::<Data<Leverage>>(response.body)
                .ok()
                .map(|p| p.data)
                .filter(|l| l.long_leverage > 0.0 && l.short_leverage > 0.0);
            let Some(rails) = rails else {
                let _ = cx.fail_plan(&leg.plan, metadata_error("Invalid BingX leverage metadata"));
                return;
            };
            let read = (rails.long_leverage, rails.short_leverage);
            if !current {
                answer_current(cx, &leg, read);
                return;
            }
            let Some(native) = cx.store.market(&leg.symbol).map(|m| m.id.clone()) else {
                let _ = cx.fail_plan(
                    &leg.plan,
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
                revision,
                generation,
                publish,
                ..
            } = leg;
            issue(
                cx,
                credentials,
                plan,
                Fences {
                    symbol,
                    native,
                    revision,
                    generation,
                    publish,
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
                let _ = cx.fail_plan(&leg.plan, metadata_error("Invalid BingX margin metadata"));
                return;
            }
        };
        if cx
            .finish_plan_call(&leg.plan, &leg.slot, &leg.context)
            .is_err()
        {
            return;
        }
        self.bump_positions();
        cx.store.update_setting(&leg.symbol, |s| {
            s.leverage = buy;
            s.buy_leverage = Some(buy);
            s.sell_leverage = Some(sell);
            s.margin_type = Some(margin);
        });
        let _ = cx.done_plan(&leg.plan, result((buy, sell)));
    }
}

/// Provider, route, transport and timeout failures all settle the one Plan.
pub fn fail(cx: &mut Cx<'_, Pending>, leg: &Leg, error: impl Into<VenueError>) {
    let _ = cx.fail_plan(&leg.plan, error);
}

/// Native publication fences a read carries through both stages.
struct Fences {
    symbol: String,
    native: String,
    revision: u64,
    generation: u64,
    publish: bool,
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
        let _ = cx.fail_plan(&plan, refused());
        return;
    };
    let Ok(context) = cx.plan_leg_context(&plan, &slot) else {
        let _ = cx.fail_plan(&plan, refused());
        return;
    };
    let Fences {
        symbol,
        native,
        revision,
        generation,
        publish,
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
            revision,
            generation,
            publish,
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
    let setting = cx.store.setting(&leg.symbol);
    let valid = |value: &f64| value.is_finite() && *value > 0.0;
    let buy = setting
        .and_then(|s| s.buy_leverage)
        .filter(valid)
        .unwrap_or(read.0);
    let sell = setting
        .and_then(|s| s.sell_leverage)
        .filter(valid)
        .unwrap_or(read.1);
    let _ = cx.done_read_plan_after_data(&leg.plan, &leg.slot, &leg.context, result((buy, sell)));
}
