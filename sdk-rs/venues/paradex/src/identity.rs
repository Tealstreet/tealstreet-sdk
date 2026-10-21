//! Identity reads share JWT admission without entering a financial account cycle.
use crate::{Paradex, Pending};
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
fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result =
        result.and_then(|identity| serde_json::to_string(&identity).map_err(|e| e.to_string()));
    match result {
        Ok(data) => cx.done(command, data),
        Err(error) => {
            cx.command_error(command, &error);
            cx.fail(command, error);
        }
    }
}

#[derive(Deserialize)]
struct Body {
    account_number: Id,
}
impl Paradex {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if !self.options.private_enabled() {
            finish(
                cx,
                command,
                Err("Paradex account identity requires credentials".into()),
            );
            return;
        }
        self.account.request(
            cx,
            Method::Get,
            "/account".into(),
            None,
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
        let result = crate::success(response).and_then(|()| {
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
        });
        finish(cx, command, result);
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
