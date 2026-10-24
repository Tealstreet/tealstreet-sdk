//! A rare configuration validation must not change account settings or reconciliation state.
use crate::dispatch_signing::Http;
use crate::{Pending, Task, WeexFutures, account_wire::AccountConfig};
use safe_cex_engine::request_context::{Context, Credential, Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    affiliate_id: Option<String>,
}
/// The account ID is the caller-assigned primary stableId from trusted setup.
fn unavailable() -> VenueError {
    VenueError::new("WEEX account identity requires a trusted primary stableId")
        .kind(ErrorKind::Unsupported)
        .outcome(Outcome::NotSent)
}
#[derive(Deserialize)]
struct Status {
    #[serde(default)]
    code: Option<Code>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Code {
    Text(String),
    Integer(u64),
}
impl Code {
    fn valid(&self) -> bool {
        match self {
            Self::Text(s) => matches!(s.as_str(), "0" | "200"),
            Self::Integer(n) => matches!(n, 0 | 200),
        }
    }
}
impl WeexFutures {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, _command: CommandId) {
        if self.invalid_read_credentials {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            Self::identity_failed(cx, &context, crate::READ_CREDENTIALS_INVALID);
            return;
        }
        let (Some(_), Some(signer)) = (
            &self.credentials.primary,
            self.credentials.select(Method::Get),
        ) else {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            Self::identity_failed(
                cx,
                &context,
                "WEEX account identity requires a non-public primary key",
            );
            return;
        };
        if self.identity.is_none() {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            Self::identity_failed(cx, &context, unavailable());
            return;
        }
        if cx.now < self.paused_until {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            Self::identity_failed(cx, &context, "WEEX account reads are rate limited");
            return;
        }
        let (identity, slot) = signer;
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: Some(Credential {
                slot,
                reference: identity.credential.clone(),
            }),
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        let recipe = Http::new(
            identity.clone(),
            Method::Get,
            "/capi/v3/account/accountConfig".into(),
            None,
        );
        let (url, _, body) = recipe.unsigned();
        let pending = Pending {
            task: Task::Identity {
                context: context.clone(),
            },
            invalid: false,
            skipped: Vec::new(),
            seed_generation: self.seed_generation,
        };
        if cx
            .request_signed_managed(
                context.clone(),
                None,
                Method::Get,
                url,
                body,
                Box::new(recipe),
                pending,
            )
            .is_err()
        {
            crate::managed::reject(
                cx,
                &context,
                VenueError::new("WEEX managed read lifecycle is unavailable")
                    .kind(ErrorKind::Internal)
                    .outcome(Outcome::NotSent),
            );
        }
    }

    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        response: Response<'_>,
    ) {
        let validated = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!("WEEX account identity HTTP {}", response.status));
            }
            let status: Status = serde_json::from_str(response.body)
                .map_err(|_| "Invalid WEEX identity response")?;
            if status.code.as_ref().is_some_and(|code| !code.valid()) {
                return Err("WEEX account identity validation rejected".into());
            }
            let config: AccountConfig = serde_json::from_str(response.body)
                .map_err(|_| "Invalid WEEX account identity configuration")?;
            if !config.can_trade {
                return Err("WEEX API key is valid but trading is disabled".into());
            }
            Ok(())
        })();
        let result = validated
            .map_err(VenueError::new)
            .and_then(|()| self.identity.clone().ok_or_else(unavailable))
            .and_then(|user_id| {
                serde_json::to_string(&Identity {
                    user_id,
                    affiliate_id: None,
                })
                .map_err(|e| VenueError::new(e.to_string()))
            });
        match result {
            Ok(data) => crate::managed::complete(cx, context, data),
            Err(error) => Self::identity_failed(cx, context, error),
        }
    }
    pub(crate) fn identity_failed(
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        error: impl Into<VenueError>,
    ) {
        let error = error.into();
        let _ = cx.command_error_context(context, error.message.clone());
        crate::managed::reject(cx, context, error);
    }
}
