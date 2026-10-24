//! A rare primary-signed identity read, separate from live reconciliation.
use crate::{Cryptocom, Pending, sign::Params, wire::RestEnvelope};
use safe_cex_engine::request_context::{Context, Operation};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, CommandWait, Cx, Method, Response, VenueError};
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
fn finish(cx: &mut Cx<'_, Pending>, root: &Context, result: Result<Identity, VenueError>) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity).map_err(|e| VenueError::new(e.to_string()))
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
pub struct Request {
    root: ReadRoot,
    leg: Option<ReadLeg>,
    retries: u8,
}
impl Request {
    pub(crate) fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg
            .as_ref()
            .is_some_and(|leg| self.root.validate_delivery(cx, leg).is_ok())
    }
    fn retire_leg(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = self.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
    }
}
#[derive(Deserialize)]
struct Body {
    this_account: Account,
}
#[derive(Deserialize)]
struct Account {
    uuid: Id,
}
impl Cryptocom {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, _command: CommandId) {
        if matches!(cx.command_wait(), Some(CommandWait::Expired)) {
            let _ = cx.reject_expired_command(Operation::Read);
            return;
        }
        let refusal = self
            .options
            .secondary_validation
            .and_then(Result::err)
            .or_else(|| {
                (self.options.public_data_only || self.credentials.is_none())
                    .then_some("Crypto.com account identity requires credentials")
            });
        let selected = if refusal.is_some() {
            Err(safe_cex_engine::credential_selection::CredentialUnavailable::Missing)
        } else {
            self.selected_read()
        };
        let error =
            VenueError::new(refusal.unwrap_or("Crypto.com account identity requires credentials"));
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
        self.identity_request(
            cx,
            Request {
                root,
                leg: None,
                retries: 0,
            },
        );
    }
    fn identity_request(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let leg = match request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                finish(cx, request.root.context(), Err(error.read_venue_error()));
                return;
            }
        };
        let path = "private/get-accounts";
        let url = format!("{}/{path}", self.rest);
        if let Err((Pending::Identity(request), error)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Post,
                url: url.clone(),
                body: None,
                stream: None,
            },
            |identity| {
                Box::new(crate::sign::Envelope::http(
                    identity.clone(),
                    path,
                    Params::default(),
                    &url,
                ))
            },
            |leg| {
                Pending::Identity(Request {
                    leg: Some(leg),
                    ..request
                })
            },
        ) {
            self.identity_reject(cx, request, error.read_venue_error());
        }
    }
    pub(crate) fn identity_reject(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: VenueError,
    ) {
        request.retire_leg(cx);
        finish(cx, request.root.context(), Err(error));
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        request.retire_leg(cx);
        let result = (|| {
            let envelope: RestEnvelope<'_> = serde_json::from_str(response.body)
                .map_err(|_| "Invalid Crypto.com account identity response".to_owned())?;
            if envelope.code == 40101 && request.retries < 1 {
                return Ok(None);
            }
            if !(200..300).contains(&response.status) || envelope.code != 0 {
                return Err(format!(
                    "Crypto.com account identity rejected (HTTP {}, code {})",
                    response.status, envelope.code
                ));
            }
            let body: Body = serde_json::from_str(
                envelope
                    .result
                    .ok_or("Missing Crypto.com account identity result")?
                    .get(),
            )
            .map_err(|_| "Invalid Crypto.com account identity result")?;
            let user_id = body
                .this_account
                .uuid
                .value()
                .ok_or("Crypto.com account identity is missing")?;
            Ok(Some(Identity {
                user_id,
                affiliate_id: None,
            }))
        })();
        match result {
            Ok(None) => {
                request.retries += 1;
                self.identity_request(cx, request);
            }
            Ok(Some(identity)) => finish(cx, request.root.context(), Ok(identity)),
            Err(error) => finish(
                cx,
                request.root.context(),
                Err(crate::reads::response_failure(response, error)),
            ),
        }
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: VenueError,
    ) {
        request.retire_leg(cx);
        if cx
            .validate_read_root_context(request.root.context())
            .is_err()
        {
            return;
        }
        if request.retries < 3 {
            request.retries += 1;
            self.identity_request(cx, request);
        } else {
            finish(cx, request.root.context(), Err(error));
        }
    }
}
