//! Account-tier reads share TS's five-minute cache and one-minute retry cooldown.
use crate::{Lighter, Pending, account};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, Method, Response, VenueAction, unsupported_venue_action};
use serde::Deserialize;
use serde_json::value::RawValue;

const CACHE_MS: f64 = 5.0 * 60_000.0;
const RETRY_MS: f64 = 60_000.0;

#[derive(Clone, Copy, PartialEq)]
enum Tier {
    Standard,
    Premium,
    Plus,
}
impl Tier {
    fn label(self) -> &'static str {
        match self {
            Tier::Standard => "standard",
            Tier::Premium => "premium",
            Tier::Plus => "plus",
        }
    }
}
struct TierLabel(Option<Tier>);
impl<'de> Deserialize<'de> for TierLabel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LabelVisitor;
        impl<'de> serde::de::Visitor<'de> for LabelVisitor {
            type Value = TierLabel;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a known Lighter account-tier label or singleton sequence")
            }
            fn visit_str<E: serde::de::Error>(self, label: &str) -> Result<Self::Value, E> {
                let label = label.trim_matches(|c: char| {
                    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
                });
                Ok(TierLabel(match label.to_ascii_lowercase().as_str() {
                    "standard" | "std" => Some(Tier::Standard),
                    "premium" => Some(Tier::Premium),
                    "plus" => Some(Tier::Plus),
                    _ => None,
                }))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let first = sequence
                    .next_element::<TierLabel>()?
                    .unwrap_or(TierLabel(None));
                let mut multiple = false;
                while sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    multiple = true;
                }
                Ok(if multiple { TierLabel(None) } else { first })
            }
        }
        deserializer.deserialize_any(LabelVisitor)
    }
}
#[derive(Default)]
pub(crate) struct Limits {
    tier: Option<Tier>,
    in_flight: bool,
    next_fetch_at: f64,
}
#[derive(Deserialize)]
struct Envelope {
    code: Option<u16>,
    #[serde(default)]
    message: Option<String>,
}
#[derive(Default, Deserialize)]
struct Payload<'a> {
    #[serde(default, borrow)]
    user_tier_name: Option<&'a RawValue>,
    #[serde(default, borrow, rename = "userTierName")]
    user_tier_name_camel: Option<&'a RawValue>,
    #[serde(default, borrow)]
    user_tier: Option<&'a RawValue>,
    #[serde(default, borrow, rename = "userTier")]
    user_tier_camel: Option<&'a RawValue>,
    #[serde(default, borrow)]
    account_type_name: Option<&'a RawValue>,
    #[serde(default, borrow, rename = "accountTypeName")]
    account_type_name_camel: Option<&'a RawValue>,
    #[serde(default, borrow)]
    account_type: Option<&'a RawValue>,
    #[serde(default, borrow, rename = "accountType")]
    account_type_camel: Option<&'a RawValue>,
    #[serde(default, borrow)]
    data: Option<&'a RawValue>,
    #[serde(default, borrow)]
    account: Option<&'a RawValue>,
}
impl Payload<'_> {
    fn own_name(&self) -> Option<&RawValue> {
        self.user_tier_name
            .or(self.user_tier_name_camel)
            .or(self.user_tier)
            .or(self.user_tier_camel)
            .or(self.account_type_name)
            .or(self.account_type_name_camel)
            .or(self.account_type)
            .or(self.account_type_camel)
    }
    fn record(raw: &RawValue) -> Option<Payload<'_>> {
        if raw.get().starts_with('[') {
            Some(Payload::default())
        } else {
            serde_json::from_str(raw.get()).ok()
        }
    }
    fn tier(&self) -> Option<Tier> {
        if let Some(raw) = self.own_name() {
            return Self::normalize(raw);
        }
        let data = self.data.and_then(Self::record);
        if let Some(raw) = data.as_ref().and_then(Payload::own_name) {
            return Self::normalize(raw);
        }
        let account = self.account.and_then(Self::record).or_else(|| {
            data.as_ref()
                .and_then(|data| data.account)
                .and_then(Self::record)
        });
        Self::normalize(account.as_ref()?.own_name()?)
    }
    fn normalize(raw: &RawValue) -> Option<Tier> {
        serde_json::from_str::<TierLabel>(raw.get()).ok()?.0
    }
}
impl Lighter {
    pub(crate) fn refresh_limits(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        // Frame-fed: the tier arrives in VENUE_STATE; the venue refuses the rest.
        if cx.frame_fed() || cx.now < self.limits.next_fetch_at {
            return true;
        }
        if self.limits.in_flight {
            return false;
        }
        let Some(account) = self.account.account_index else {
            return true;
        };
        if self.account.identity.is_none() {
            self.limits.next_fetch_at = cx.now + RETRY_MS;
            return true;
        }
        self.limits.in_flight = true;
        self.account_request(
            cx,
            format!(
                "{}/api/v1/accountLimits?account_index={account}",
                self.host()
            ),
            None,
            account::Request::Limits { started_at: cx.now },
            Some(crate::sign::AuthPlacement::Header),
        );
        !self.limits.in_flight
    }
    pub(crate) fn tier_action(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        action: VenueAction,
    ) {
        match action {
            VenueAction::FetchAccountTier => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                // Cached only: the limits read keeps its own cadence (none at startup).
                let tier = self
                    .limits
                    .tier
                    .map_or_else(|| "null".into(), |t| format!("\"{}\"", t.label()));
                let _ = cx.done_context(&context, tier);
            }
            VenueAction::SetAccountTier { tier } => {
                let Ok(preflight) =
                    cx.begin_preflight(safe_cex_engine::request_context::PreflightPolicy {
                        purpose:
                            safe_cex_engine::request_context::PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: self
                            .account
                            .identity
                            .as_ref()
                            .map(crate::primary_credential),
                    })
                else {
                    return;
                };
                let result = (|| {
                    if !matches!(tier.as_str(), "standard" | "premium" | "plus") {
                        return Err(
                            "Lighter account tier must be standard, premium or plus".to_owned()
                        );
                    }
                    let account = self
                        .account
                        .account_index
                        .ok_or("Lighter account is not loaded yet")?;
                    let identity = self
                        .account
                        .identity
                        .clone()
                        .ok_or("Lighter signer unavailable")?;
                    let recipe = self.auth_recipe(identity, crate::sign::AuthPlacement::Header)?;
                    Ok((account, recipe))
                })();
                let (account, recipe) = match result {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = cx.fail_preflight(&preflight, error);
                        return;
                    }
                };
                let credential = crate::primary_credential(self.account.identity.as_ref().unwrap());
                let (plan, slot) = match cx.admit_serial_plan(
                    &preflight,
                    super::trading::serial_call(Operation::Mutation),
                    safe_cex_engine::request_context::plan::SerialPolicy {
                        credential: Some(credential),
                        pacing: None,
                        refusals_complete: false,
                    },
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = cx.fail_preflight(&preflight, error.venue_error());
                        return;
                    }
                };
                let context = match cx.plan_leg_context_with_intent(
                    &plan,
                    &slot,
                    safe_cex_engine::classify::Intent::Setting,
                ) {
                    Ok(context) => context,
                    Err(error) => {
                        let _ = cx.fail_plan(&plan, error.venue_error());
                        return;
                    }
                };
                let pending = Pending::Tier {
                    plan: plan.clone(),
                    slot,
                    context: context.clone(),
                };
                if let Err((_, error)) = cx.request_signed_managed(
                    context,
                    None,
                    Method::Post,
                    format!("{}/api/v1/changeAccountTier", self.host()),
                    Some(format!("account_index={account}&new_tier={tier}")),
                    recipe,
                    pending,
                ) {
                    let _ = cx.fail_plan(&plan, error.venue_error());
                }
            }
            #[allow(unreachable_patterns)]
            _ => unsupported_venue_action(cx, command),
        }
    }
    pub(crate) fn tier_changed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: safe_cex_engine::request_context::Plan,
        slot: safe_cex_engine::request_context::plan::Slot,
        context: safe_cex_engine::request_context::Context,
        response: Response<'_>,
    ) {
        if cx.validate_plan_delivery(&plan, &context).is_err() {
            return;
        }
        let envelope = serde_json::from_str::<Envelope>(response.body).ok();
        if (200..300).contains(&response.status)
            && envelope.as_ref().is_some_and(|e| e.code == Some(200))
        {
            if cx
                .http_write_fact(
                    &context,
                    safe_cex_engine::classify::WriteFact::Accepted {
                        order_ids: Vec::new(),
                    },
                    None,
                )
                .is_err()
                || cx.validate_http_write_effects(&context).is_err()
            {
                let _ = cx.fail_plan(&plan, "Invalid Lighter tier acknowledgement");
                return;
            }
            // The next read reflects the new tier and its rate limits.
            self.limits.tier = None;
            self.limits.next_fetch_at = 0.0;
            self.refresh_limits(cx);
            let _ = cx.finish_plan_call(&plan, &slot, &context);
            let _ = cx.done_plan(&plan, "null".into());
        } else {
            let message = envelope
                .as_ref()
                .and_then(|e| e.message.as_deref())
                .filter(|m| !m.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("HTTP {}", response.status));
            let message = format!("Lighter account tier change failed: {message}");
            if (200..300).contains(&response.status)
                && let Some(code) = envelope.and_then(|e| e.code)
            {
                if let Err(error) = Self::record_refusal(cx, &context, u64::from(code), &message) {
                    let _ = cx.fail_plan(&plan, error.venue_error());
                    return;
                }
            } else {
                let _ = cx.http_write_unproven(&context, &message);
            }
            let _ = cx.finish_plan_call(&plan, &slot, &context);
            let _ = cx.fail_plan(&plan, message);
        }
    }
    pub(crate) fn limits_standard(&self) -> bool {
        self.limits.tier == Some(Tier::Standard)
    }
    /// VENUE_STATE's tier code: 0 unknown, 1 standard, 2 premium, 3 plus.
    pub(crate) fn limits_tier_code(&self) -> u8 {
        match self.limits.tier {
            None => 0,
            Some(Tier::Standard) => 1,
            Some(Tier::Premium) => 2,
            Some(Tier::Plus) => 3,
        }
    }
    /// A data source's known tier replaces ours; its unknown keeps ours.
    pub(crate) fn apply_limits_tier_code(&mut self, code: u8) {
        self.limits.tier = match code {
            1 => Some(Tier::Standard),
            2 => Some(Tier::Premium),
            3 => Some(Tier::Plus),
            _ => self.limits.tier,
        };
    }
    pub(crate) fn limits_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        started_at: f64,
        response: Response<'_>,
    ) {
        if (200..300).contains(&response.status)
            && let Ok(envelope) = serde_json::from_str::<Envelope>(response.body)
            && envelope.code == Some(200)
            && let Ok(payload) = serde_json::from_str::<Payload<'_>>(response.body)
        {
            self.limits.tier = payload.tier();
            self.limits.next_fetch_at = started_at + CACHE_MS;
        } else {
            self.limits.next_fetch_at = started_at + RETRY_MS;
        }
        self.limits.in_flight = false;
        self.trade_limits_ready(cx);
    }
    pub(crate) fn limits_failed(&mut self, cx: &mut Cx<'_, Pending>, started_at: f64) {
        self.limits.in_flight = false;
        self.limits.next_fetch_at = started_at + RETRY_MS;
        self.trade_limits_ready(cx);
    }
}
