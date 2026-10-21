//! Classic Futures user identity; the Spot identity route stays separate.
use crate::{Kucoin, Pending};
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
    sub_user_id: Option<Id>,
    referrer_user_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("KuCoin account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "KuCoin account identity response is invalid")?;
    if body.code != "200000" {
        return Err(if body.msg.is_empty() {
            format!("KuCoin account identity code {}", body.code)
        } else {
            body.msg
        });
    }
    let data = body
        .data
        .ok_or("KuCoin account identity response is missing data")?;
    let user_id = data
        .user_id
        .and_then(Id::value)
        .ok_or("KuCoin user-info response did not include a user ID")?;
    Ok(Identity {
        user_id,
        sub_id: data.sub_user_id.filter(Id::truthy).and_then(Id::value),
        affiliate_id: data.referrer_user_id.filter(Id::truthy).and_then(Id::value),
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
impl Kucoin {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read
                .validate()
                .and_then(|()| read.validate_passphrase(self.options.passphrase.as_deref()))
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        if self.options.public_data_only || self.credentials.is_none() {
            finish(
                cx,
                command,
                Err("KuCoin account identity requires credentials".into()),
            );
            return;
        }
        self.signed(
            cx,
            Method::Get,
            "/api/v1/user-info",
            None,
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
