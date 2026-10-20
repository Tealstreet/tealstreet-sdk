//! Rare account identity reads never commit live account state.
use crate::{Bingx, Pending};
use safe_cex_engine::{CommandId, Cx, Method, Response};
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
            Self::Integer(value) => value.to_string(),
        };
        (!value.trim().is_empty()).then_some(value)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
}

#[derive(Deserialize)]
struct Body {
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<Vec<Row>>,
}
#[derive(Deserialize)]
struct Row {
    #[serde(default)]
    asset: String,
    #[serde(rename = "userId")]
    user_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("BingX account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "BingX account identity response is invalid")?;
    if body.code != 0 {
        return Err(if body.msg.is_empty() {
            format!("BingX account identity code {}", body.code)
        } else {
            body.msg
        });
    }
    let rows = body
        .data
        .ok_or("BingX balance response did not include an account balance")?;
    let index = rows.iter().position(|row| row.asset == "USDT").unwrap_or(0);
    let user_id = rows
        .into_iter()
        .nth(index)
        .and_then(|row| row.user_id)
        .and_then(Id::value)
        .ok_or("BingX balance response did not include a user ID")?;
    Ok(Identity { user_id })
}

fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result = result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()));
    if let Err(error) = &result {
        cx.command_error(command, error);
    }
    cx.done(command, result);
}
impl Bingx {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(error) = self.config_error {
            finish(cx, command, Err(error.into()));
            return;
        }

        let Some(credentials) = self.credentials.as_ref() else {
            finish(
                cx,
                command,
                Err("BingX account identity requires credentials".into()),
            );
            return;
        };
        let (url, headers, body) = crate::sign::Params::new().into_request(
            Method::Get,
            "/openApi/swap/v3/user/balance",
            Some(credentials),
            cx.now,
        );
        cx.request(Method::Get, url, headers, body, Pending::Identity(command));
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        finish(cx, command, parse(response));
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: &str,
    ) {
        finish(cx, command, Err(error.into()));
    }
}
