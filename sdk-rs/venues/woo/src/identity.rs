//! Rare account identity reads never commit live account state.
use crate::{Pending, Woo};
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
    affiliate_id: Option<String>,
}

#[derive(Deserialize)]
struct Body {
    success: bool,
    data: Option<Data>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    application_id: Option<Id>,
    #[serde(rename = "referrerID")]
    referrer_id: Option<Id>,
    position_mode: String,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("WOO X account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "WOO X account identity response is invalid")?;
    if !body.success {
        return Err("WOO X account info request failed".into());
    }
    let data = body
        .data
        .ok_or("WOO X account info response is missing data")?;
    if !matches!(data.position_mode.as_str(), "ONE_WAY" | "HEDGE_MODE") {
        return Err("WOO X account info response has an invalid position mode".into());
    }
    let user_id = data
        .application_id
        .and_then(Id::value)
        .ok_or("WOO X account info response did not include an application ID")?;
    let affiliate_id = data.referrer_id.filter(Id::truthy).and_then(Id::value);
    Ok(Identity {
        user_id,
        affiliate_id,
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
impl Woo {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }

        if let Err(error) = self.signed(
            cx,
            Method::Get,
            "/v3/accountinfo",
            Vec::new(),
            None,
            Pending::Identity(command),
        ) {
            finish(cx, command, Err(error));
        }
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
