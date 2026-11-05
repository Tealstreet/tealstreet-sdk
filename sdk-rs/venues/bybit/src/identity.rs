//! V5 API-key identity: a signed command Read on the engine's managed
//! authority, with the read key when configured.
use crate::{BybitLinear, Pending, Rest, Then, sign::Query};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Cx, ErrorKind, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Text(String),
    Integer(u64),
}
impl Id {
    fn truthy(&self) -> bool {
        match self {
            Self::Text(value) => !value.is_empty(),
            Self::Integer(value) => *value != 0,
        }
    }

    fn value(self) -> Option<String> {
        let value = match self {
            Self::Text(value) => value,
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    ret_code: i64,
    #[serde(default)]
    ret_msg: String,
    result: Option<Data>,
}
#[derive(Deserialize)]
struct Data {
    #[serde(rename = "userID")]
    user_id: Option<Id>,
    #[serde(rename = "inviterID")]
    inviter_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Bybit account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "Bybit account identity response is invalid")?;
    if body.ret_code != 0 {
        return Err(if body.ret_msg.is_empty() {
            format!("Bybit account identity code {}", body.ret_code)
        } else {
            body.ret_msg
        });
    }
    let data = body
        .result
        .ok_or("Bybit account identity response is missing result")?;
    let user_id = data
        .user_id
        .and_then(Id::value)
        .ok_or("Bybit API-key response did not include a user ID")?;
    Ok(Identity {
        user_id,
        affiliate_id: data.inviter_id.filter(Id::truthy).and_then(Id::value),
    })
}
fn finish(cx: &mut Cx<'_, Pending>, context: &Context, result: Result<Identity, String>) {
    let result = result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()));
    match result {
        Ok(data) => {
            let _ = cx.done_context(context, data);
        }
        Err(error) => fail(cx, context, VenueError::new(error)),
    }
}

/// Every failure is reported to its command before the typed terminal.
fn fail(cx: &mut Cx<'_, Pending>, context: &Context, error: VenueError) {
    // Only a live owner gets the diagnostic, so it never outlives its terminal.
    if cx
        .command_error_context(context, error.message.clone())
        .is_ok()
    {
        let _ = cx.fail_context(context, error);
    }
}

fn refuse(cx: &mut Cx<'_, Pending>, context: &Context, message: String, kind: ErrorKind) {
    fail(
        cx,
        context,
        VenueError::new(message)
            .kind(kind)
            .outcome(Outcome::NotSent),
    );
}

impl BybitLinear {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>) {
        let policy = self
            .read_policy(true, crate::policy::read_policy().timeout_ms)
            .unwrap_or_else(crate::policy::read_policy);
        // The engine owns an initial admission refusal.
        let Ok(context) = cx.admit_http_command(policy) else {
            return;
        };
        if let Some((message, kind)) = self.signed_read_refusal("account identity") {
            return refuse(cx, &context, message, kind);
        }
        let rest = Rest::get("/v5/user/query-api", Query::new()).signed();
        self.queue_read(cx, context.clone(), rest, Then::Identity(context));
    }

    /// Why a signed command read cannot be sent with this account's configuration.
    pub(crate) fn signed_read_refusal(&self, read: &str) -> Option<(String, ErrorKind)> {
        if let Some(error) = self.config_error.clone() {
            return Some((error, ErrorKind::InvalidInput));
        }
        if let Some(credentials) = &self.read_credentials
            && let Err(error) = credentials.validate()
        {
            return Some((error.into(), ErrorKind::InvalidInput));
        }
        self.credentials
            .as_ref()
            .is_none_or(|c| c.validate().is_err())
            .then(|| {
                (
                    format!("Bybit {read} requires credentials"),
                    ErrorKind::AuthInvalid,
                )
            })
    }

    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        response: Result<Response<'_>, VenueError>,
    ) {
        match response {
            Ok(response) => finish(cx, &context, parse(response)),
            Err(error) => fail(cx, &context, error),
        }
    }
}
