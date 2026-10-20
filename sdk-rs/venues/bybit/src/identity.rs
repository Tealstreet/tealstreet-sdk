//! V5 API-key identity reads reuse the native read signer and signature resend.
use crate::{BybitLinear, Pending, Rest, Then, sign::Query};
use safe_cex_engine::{CommandId, Cx, Response};
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
fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result = result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()));
    if let Err(error) = &result {
        cx.command_error(command, error);
    }
    cx.done(command, result);
}
impl BybitLinear {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if self.variant.is_hyro() {
            finish(
                cx,
                command,
                Err("HyroTrader account identity is unsupported".into()),
            );
            return;
        }
        if let Some(error) = &self.config_error {
            finish(cx, command, Err(error.clone()));
            return;
        }
        if let Some(read) = &self.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        if self
            .credentials
            .as_ref()
            .is_none_or(|c| c.key.is_empty() || c.secret.is_empty())
        {
            finish(
                cx,
                command,
                Err("Bybit account identity requires credentials".into()),
            );
            return;
        }
        self.send(
            cx,
            Rest::get("/v5/user/query-api", Query::new()).signed(),
            Then::Identity(command),
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Result<Response<'_>, &str>,
    ) {
        finish(cx, command, response.map_err(str::to_owned).and_then(parse));
    }
}
