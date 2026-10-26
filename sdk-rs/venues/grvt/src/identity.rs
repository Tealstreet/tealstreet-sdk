//! Validate the selected subaccount, but attribute identity to its authenticated main account.
use crate::{Grvt, Job, Pending};
use safe_cex_engine::{CommandId, Cx, Response};
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
    affiliate_id: Option<String>,
}
#[derive(Deserialize)]
struct SummaryEnvelope {
    #[serde(rename = "r", alias = "result")]
    _result: Summary,
}
#[derive(Deserialize)]
struct Summary {
    #[serde(rename = "te", alias = "total_equity")]
    _equity: crate::wire::Number,
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
impl Grvt {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if self.options.public_data_only
            || self
                .options
                .key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            finish(
                cx,
                command,
                Err("GRVT account identity requires credentials".into()),
            );
            return;
        }
        #[derive(Serialize)]
        struct Body<'a> {
            sub_account_id: &'a str,
        }
        let body = serde_json::to_string(&Body {
            sub_account_id: self
                .account
                .trading_account_id
                .as_deref()
                .or(self.options.trading_account_id.as_deref())
                .unwrap_or(""),
        })
        .expect("GRVT identity request");
        self.request(
            cx,
            "/lite/v1/account_summary",
            body,
            Job::Identity { command },
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        let result = serde_json::from_str::<SummaryEnvelope>(response.body)
            .map_err(|_| "Invalid GRVT account identity summary".to_owned())
            .and_then(|_| {
                let user_id = self
                    .account
                    .session
                    .as_ref()
                    .map(|session| session.account_id.clone())
                    .filter(|id| !id.trim().is_empty())
                    .ok_or("GRVT account identity session is missing")?;
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
