//! Classic Futures user identity; the Spot identity route stays separate.
use crate::{Kucoin, Pending};
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::request_context::RetryPolicy;
use safe_cex_engine::selected_read::{
    AdmittedRead, LocalRefusal, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::{CommandId, Cx, Method, Response, VenueError};
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
impl Kucoin {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>, _command: CommandId) {
        let invalid = self.options.read_error();
        let selected = if invalid.is_some() {
            Err(CredentialUnavailable::Missing)
        } else {
            SelectedRead::select(&self.slots(), ReadRole::Read { scope: None })
        };
        let message = invalid.unwrap_or("KuCoin account identity requires credentials");
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
        const PATH: &str = "/api/v1/user-info";
        let broker = self
            .options
            .broker
            .as_ref()
            .map(crate::sign::BrokerOptions::descriptor);
        if let Err((Pending::Identity(read), error)) = read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url: format!("{}{PATH}", crate::REST),
                body: None,
                stream: None,
            },
            |identity| {
                Box::new(crate::sign::dispatch::Http::new(
                    identity.clone(),
                    Method::Get,
                    crate::REST,
                    PATH.into(),
                    None,
                    broker,
                ))
            },
            Pending::Identity,
        ) {
            self.identity_failed(cx, read, error.read_venue_error());
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
        match parse(response)
            .and_then(|identity| serde_json::to_string(&identity).map_err(|e| e.to_string()))
        {
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
        mut error: VenueError,
    ) {
        error.outcome = None;
        if cx
            .command_error_context(read.context(), error.message.clone())
            .is_ok()
        {
            let _ = cx.fail_context(read.context(), error);
        }
    }
}
