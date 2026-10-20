//! Rare account identity reads never commit live account state.
use crate::{Backpack, Pending};
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
#[serde(rename_all = "camelCase")]
struct Body {
    account_id: Option<Id>,
    user_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "Backpack account identity HTTP {}",
            response.status
        ));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "Backpack account identity response is invalid")?;
    let user_id = body
        .account_id
        .filter(Id::truthy)
        .or(body.user_id)
        .and_then(Id::value)
        .ok_or("Backpack account response did not include an account ID or user ID")?;
    Ok(Identity {
        user_id,
        affiliate_id: None,
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
impl Backpack {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }

        if self.options.public_data_only {
            finish(
                cx,
                command,
                Err("Backpack account identity requires credentials".into()),
            );
            return;
        }
        if let Err(error) = self.signed(
            cx,
            Method::Get,
            "/api/v1/account",
            crate::sign::Params::new(),
            None,
            Pending::Identity(command),
            false,
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
