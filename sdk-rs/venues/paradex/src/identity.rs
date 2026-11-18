//! Identity reads share JWT admission without entering a financial account cycle.
use crate::{Paradex, Pending};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, Response, VenueError};
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Text(String),
    Integer(u64),
}
impl Id {
    fn value(self) -> Option<String> {
        let value = match self {
            Self::Text(value) => value,
            Self::Integer(0) => return None,
            Self::Integer(value) => value.to_string(),
        };
        (!value.trim().is_empty()).then_some(value)
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
    affiliate_id: Option<String>,
}
fn finish(cx: &mut Cx<'_, Pending>, context: &Context, result: Result<Identity, VenueError>) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity).map_err(|e| VenueError::new(e.to_string()))
    });
    match result {
        Ok(data) => {
            let _ = cx.done_context(context, data);
        }
        Err(error) => {
            let _ = cx.command_error_context(context, error.message.clone());
            let _ = cx.fail_context(context, error);
        }
    }
}

#[derive(Deserialize)]
struct Body {
    account_number: Id,
}
impl Paradex {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>, _command: CommandId) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        if !self.options.private_enabled() {
            finish(
                cx,
                &context,
                Err(VenueError::new(
                    "Paradex account identity requires credentials",
                )),
            );
            return;
        }
        self.account.read(
            cx,
            context.clone(),
            "/account",
            None,
            Pending::Identity(context),
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        response: Response<'_>,
    ) {
        let result = crate::success(response)
            .and_then(|()| {
                let body: Body = serde_json::from_str(response.body)
                    .map_err(|_| "Invalid Paradex account identity response")?;
                let user_id = body
                    .account_number
                    .value()
                    .ok_or("Paradex account identity is missing")?;
                Ok(Identity {
                    user_id,
                    affiliate_id: None,
                })
            })
            .map_err(VenueError::new);
        finish(cx, &context, result);
    }
    /// A typed refusal (credentials, Session) or transport failure.
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        error: VenueError,
    ) {
        finish(cx, &context, Err(self.account.read_error(error)));
    }
}
