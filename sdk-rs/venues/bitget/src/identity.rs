//! Account ownership is reported by the spot identity route for every product.
use crate::{BitgetFutures, Pending};
use safe_cex_engine::{CommandId, Cx, Method, Response};
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
    sub_id: Option<String>,
    affiliate_id: Option<String>,
}
#[derive(Deserialize)]
struct Body {
    code: String,
    #[serde(default)]
    msg: String,
    data: Option<Data>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    user_id: Option<Id>,
    parent_id: Option<Id>,
    inviter_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Bitget account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "Bitget account identity response is invalid")?;
    if body.code != "00000" {
        return Err(if body.msg.is_empty() {
            format!("Bitget account identity code {}", body.code)
        } else {
            body.msg
        });
    }
    let data = body
        .data
        .ok_or("Bitget account identity response is missing data")?;
    let own_id = data
        .user_id
        .and_then(Id::value)
        .ok_or("Bitget account info did not include a user ID")?;
    let parent = data.parent_id.filter(Id::truthy).and_then(Id::value);
    let (user_id, sub_id) = match parent {
        Some(parent) => (parent, Some(own_id)),
        None => (own_id, None),
    };
    Ok(Identity {
        user_id,
        sub_id,
        affiliate_id: data.inviter_id.filter(Id::truthy).and_then(Id::value),
    })
}
fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result = result
        .and_then(|identity| serde_json::to_string(&identity).map_err(|error| error.to_string()));
    match result {
        Ok(data) => cx.done(command, data),
        Err(error) => {
            cx.command_error(command, &error);
            cx.fail(command, error);
        }
    }
}
impl BitgetFutures {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        if !self.private_enabled() {
            finish(
                cx,
                command,
                Err("Bitget account identity requires credentials".into()),
            );
            return;
        }
        const PATH: &str = "/api/v2/spot/account/info";
        self.send(
            cx,
            Method::Get,
            PATH,
            crate::Payload::Body(None),
            true,
            Pending::Identity(command),
        );
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
