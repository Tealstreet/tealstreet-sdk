//! Rare account identity reads never commit live account state.
use crate::{Bingx, Pending};
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::request_context::RetryPolicy;
use safe_cex_engine::selected_read::{
    AdmittedRead, LocalRefusal, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::{Cx, Method, Response, VenueError};
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

impl Bingx {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>) {
        let selected = if self.config_error.is_some() {
            Err(CredentialUnavailable::Missing)
        } else {
            SelectedRead::select(&self.account.slots, ReadRole::Read { scope: None })
        };
        let message = self
            .config_error
            .unwrap_or("BingX account identity requires credentials");
        let Ok(read) = SelectedRead::command(
            cx,
            selected,
            ReadPolicy {
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            LocalRefusal {
                diagnostic: Some(message.into()),
                error: message.into(),
            },
        ) else {
            return;
        };
        const PATH: &str = "/openApi/swap/v3/user/balance";
        let (url, _, body) = crate::sign::Params::new().render(Method::Get, PATH, Vec::new());
        if let Err((pending, error)) = read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body,
                stream: None,
            },
            |identity| {
                Box::new(crate::sign::dispatch::Http::new(
                    identity.clone(),
                    Method::Get,
                    PATH,
                    crate::sign::Params::new(),
                ))
            },
            Pending::Identity,
        ) {
            <Self as safe_cex_engine::Venue>::on_sign_error(self, cx, pending, error.venue_error());
        }
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: AdmittedRead,
        response: Response<'_>,
    ) {
        if cx
            .validate_read_http_delivery(read.context(), read.context())
            .is_err()
        {
            return;
        }
        match parse(response).and_then(|identity| {
            serde_json::to_string(&identity).map_err(|error| error.to_string())
        }) {
            Ok(data) => {
                let _ = cx.done_context(read.context(), data);
            }
            Err(error) => self.identity_failed(cx, read, error.into()),
        }
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: AdmittedRead,
        error: VenueError,
    ) {
        if cx
            .command_error_context(read.context(), error.message.clone())
            .is_ok()
        {
            let _ = cx.fail_context(read.context(), error);
        }
    }
}
