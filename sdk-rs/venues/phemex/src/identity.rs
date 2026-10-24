//! Rare account identity reads never commit live account state.
use crate::{Pending, PhemexPerpetuals};
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
    code: Option<i64>,
    #[serde(default)]
    error: Option<serde::de::IgnoredAny>,
    data: Option<Data>,
}
#[derive(Deserialize)]
struct Data {
    account: Option<Account>,
}
#[derive(Deserialize)]
struct Account {
    #[serde(rename = "userID")]
    user_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Phemex account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "Phemex account identity response is invalid")?;
    if body.code.is_some_and(|code| code != 0) || body.error.is_some() {
        return Err("Phemex account identity request failed".into());
    }
    let user_id = body
        .data
        .and_then(|data| data.account)
        .and_then(|account| account.user_id)
        .and_then(Id::value)
        .ok_or("Phemex account-position response did not include a user ID")?;
    Ok(Identity { user_id })
}

fn finish(
    cx: &mut Cx<'_, Pending>,
    command: CommandId,
    result: Result<Identity, safe_cex_engine::VenueError>,
) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity)
            .map_err(|error| safe_cex_engine::VenueError::new(error.to_string()))
    });
    match result {
        Ok(data) => cx.done(command, data),
        Err(error) => {
            cx.command_error(command, &error.message);
            cx.fail(command, error);
        }
    }
}
impl PhemexPerpetuals {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }

        if self.options.public_data_only || self.credentials.is_none() {
            finish(
                cx,
                command,
                Err("Phemex account identity requires credentials".into()),
            );
            return;
        }
        self.request(
            cx,
            Method::Get,
            "/g-accounts/accountPositions",
            "currency=USDT",
            None,
            Pending::Identity(command),
            None,
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        finish(cx, command, parse(response).map_err(Into::into));
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: impl Into<safe_cex_engine::VenueError>,
    ) {
        finish(cx, command, Err(error.into()));
    }
}
