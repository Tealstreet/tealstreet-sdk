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
        if cx.now < self.limits.next_fetch_at {
            return true;
        }
        if self.limits.in_flight {
            return false;
        }
        let Some(account) = self.account.account_index else {
            return true;
        };
        let Ok(auth) = self.account_auth(cx) else {
            self.limits.next_fetch_at = cx.now + RETRY_MS;
            return true;
        };
        self.limits.in_flight = true;
        cx.request(
            Method::Get,
            format!(
                "{}/api/v1/accountLimits?account_index={account}",
                self.host()
            ),
            vec![("authorization", auth)],
            None,
            Pending::Account(account::Request::Limits { started_at: cx.now }),
        );
        false
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
                let result = (|| {
                    if !matches!(tier.as_str(), "standard" | "premium" | "plus") {
                        return Err("Lighter account tier must be standard, premium or plus".into());
                    }
                    let account = self
                        .account
                        .account_index
                        .ok_or("Lighter account is not loaded yet")?;
                    let auth = self.account_auth(cx)?;
                    Ok::<_, String>((account, auth))
                })();
                match result {
                    Ok((account, auth)) => {
                        cx.request(
                            Method::Post,
                            format!("{}/api/v1/changeAccountTier", self.host()),
                            vec![
                                ("Content-Type", "application/x-www-form-urlencoded".into()),
                                ("authorization", auth),
                            ],
                            Some(format!("account_index={account}&new_tier={tier}")),
                            Pending::Account(account::Request::ChangeTier { command }),
                        );
                    }
                    Err(error) => cx.fail(command, error),
                }
            }
            #[allow(unreachable_patterns)]
            _ => unsupported_venue_action(cx, command),
        }
    }
    pub(crate) fn tier_changed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        let envelope = serde_json::from_str::<Envelope>(response.body).ok();
        if (200..300).contains(&response.status)
            && envelope.as_ref().is_some_and(|e| e.code == Some(200))
        {
            // The next read reflects the new tier and its rate limits.
            self.limits.tier = None;
            self.limits.next_fetch_at = 0.0;
            self.refresh_limits(cx);
            cx.done(command, "null".into());
        } else {
            let message = envelope
                .and_then(|e| e.message)
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| format!("HTTP {}", response.status));
            cx.fail(
                command,
                format!("Lighter account tier change failed: {message}"),
            );
        }
    }
    pub(crate) fn limits_standard(&self) -> bool {
        self.limits.tier == Some(Tier::Standard)
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
