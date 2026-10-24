//! Rare account identity reads never commit live account state.
use crate::{Backpack, Pending};
use safe_cex_engine::credential_selection::{CredentialUnavailable, SlotPolicy, Slots};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, CommandWait, Cx, Method, Response, VenueError};
use serde::{Deserialize, Serialize};

pub struct Request {
    root: ReadRoot,
    leg: ReadLeg,
}

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

fn finish(cx: &mut Cx<'_, Pending>, context: &Context, result: Result<Identity, VenueError>) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity).map_err(|error| VenueError::new(error.to_string()))
    });
    match result {
        Ok(data) => {
            let _ = cx.done_context(context, data);
        }
        Err(error) => {
            let _ = cx.command_error_context(context, &error.message);
            let _ = cx.fail_context(context, error);
        }
    }
}
fn local_refusal(cx: &mut Cx<'_, Pending>, message: &str) {
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    finish(cx, &context, Err(VenueError::new(message)));
}
impl Backpack {
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
        if let Some(Err(crate::sign::ReadCredentialsError::Configuration(message))) =
            self.options.read_credentials_status
        {
            local_refusal(cx, message);
            return;
        }
        if self.options.public_data_only {
            local_refusal(cx, "Backpack account identity requires credentials");
            return;
        }
        if let Some(Err(error)) = self.options.read_credentials_status {
            local_refusal(cx, error.message());
            return;
        }
        let has_read = self.options.read_credentials_status.is_some();
        let slots = Slots::new(
            self.options.identity.clone(),
            self.options.read_identity.clone(),
            SlotPolicy::default(),
        );
        let (selected, missing) = if has_read && self.options.read_identity.is_none() {
            (
                Err(CredentialUnavailable::Missing),
                "Backpack managed read signing identity unavailable",
            )
        } else {
            (
                SelectedRead::select(&slots, ReadRole::Read { scope: None }),
                "Backpack credentials unavailable",
            )
        };
        let selected = match selected {
            Ok(selected) => selected,
            Err(_) => {
                local_refusal(cx, missing);
                return;
            }
        };
        let Ok(root) = SelectedRead::command_root(
            cx,
            Ok(selected),
            LocalRefusal {
                diagnostic: Some(missing.into()),
                error: missing.into(),
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
            Err(reason) => {
                finish(cx, root.context(), Err(reason.read_venue_error()));
                return;
            }
        };
        let window = self
            .account
            .credentials
            .as_ref()
            .map_or(60000, |primary| primary.window);
        let declaration = crate::sign::Request {
            method: Method::Get,
            path: "/api/v1/account".into(),
            rows: vec![crate::sign::Params::new()],
            body: None,
            window,
        };
        let request = ReadRequest {
            method: Method::Get,
            url: "https://api.backpack.exchange/api/v1/account".into(),
            body: None,
            stream: None,
        };
        if let Err((pending, reason)) = leg.queue(
            cx,
            request,
            |identity| Box::new(crate::sign::Http::new(identity.clone(), declaration)),
            |leg| Pending::Identity(Request { root, leg }),
        ) {
            if let Pending::Identity(request) = pending {
                self.identity_failed(cx, request, reason.read_venue_error());
            } else {
                unreachable!("identity queue retains its own request");
            }
        }
    }

    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if request.root.validate_delivery(cx, &request.leg).is_err() {
            return;
        }
        finish(
            cx,
            request.root.context(),
            parse(response).map_err(VenueError::new),
        );
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        finish(cx, request.root.context(), Err(error));
    }
}
