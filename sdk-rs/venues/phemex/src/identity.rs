//! Rare account identity reads never commit live account state.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::request_context::{Context, Error, Operation};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, CommandWait, Cx, Method, Response, VenueError};

pub struct Read {
    root: ReadRoot,
    leg: ReadLeg,
}
impl Read {
    pub(crate) fn validate_delivery(&self, cx: &Cx<'_, Pending>) -> Result<(), Error> {
        self.root.validate_delivery(cx, &self.leg)
    }
}
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
    root: &Context,
    result: Result<Identity, safe_cex_engine::VenueError>,
) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity)
            .map_err(|error| safe_cex_engine::VenueError::new(error.to_string()))
    });
    match result {
        Ok(data) => {
            let _ = cx.done_context(root, data);
        }
        Err(error) => {
            let _ = cx.command_error_context(root, error.message.clone());
            let _ = cx.fail_context(root, error);
        }
    }
}
impl PhemexPerpetuals {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, _command: CommandId) {
        match cx.command_wait() {
            Some(CommandWait::Remaining(_)) => {}
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return;
            }
        }
        let refusal = self
            .options
            .read_credentials
            .as_ref()
            .and_then(|read| read.validate().err().map(VenueError::from))
            .or_else(|| {
                (self.options.public_data_only || self.credentials.is_none())
                    .then(|| VenueError::new("Phemex account identity requires credentials"))
            });
        let selected = if refusal.is_some() {
            Err(CredentialUnavailable::Missing)
        } else {
            self.selected_read(ReadRole::Read { scope: None })
        };
        let error = refusal
            .unwrap_or_else(|| VenueError::new("Phemex account identity requires credentials"));
        let Ok(root) = SelectedRead::command_root(
            cx,
            selected,
            LocalRefusal {
                diagnostic: Some(error.message.clone()),
                error,
            },
        ) else {
            return;
        };
        let leg = match root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                finish(cx, root.context(), Err(error.read_venue_error()));
                return;
            }
        };
        let path = "/g-accounts/accountPositions";
        let query = "currency=USDT";
        let request = ReadRequest {
            method: Method::Get,
            url: format!("{}{path}?{query}", self.rest()),
            body: None,
            stream: None,
        };
        let queued = leg.queue(
            cx,
            request,
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    self.options.testnet,
                    Method::Get,
                    path.into(),
                    query.into(),
                    None,
                ))
            },
            |leg| Pending::Identity(Box::new(Read { root, leg })),
        );
        if let Err((Pending::Identity(read), error)) = queued {
            self.identity_failed(cx, read, error.read_venue_error());
        }
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: Box<Read>,
        response: Response<'_>,
    ) {
        let _ = cx.finish_http_context(read.leg.context());
        finish(cx, read.root.context(), parse(response).map_err(Into::into));
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: Box<Read>,
        error: impl Into<VenueError>,
    ) {
        let _ = cx.finish_http_context(read.leg.context());
        finish(cx, read.root.context(), Err(error.into()));
    }
}
