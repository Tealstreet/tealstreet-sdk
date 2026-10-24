//! Account ownership is reported by the spot identity route for every product.
use crate::{BitgetFutures, Pending, paths, sign};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Cx, Method, Response, Venue, VenueError};
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
    parent_id: Option<Id>,
    inviter_id: Option<Id>,
}
fn parse(response: Response<'_>) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Bitget account identity HTTP {}", response.status));
    }
    let body: Body = serde_json::from_str(response.body)
        .map_err(|_| "Bitget account identity response is invalid")?;
    if body.code != "00000" {
        return Err(if body.msg.is_empty() {
            format!("Bitget account identity code {}", body.code)
        } else {
            body.msg
        });
    }
    let data = body
        .data
        .ok_or("Bitget account identity response is missing data")?;
    let own_id = data
        .user_id
        .and_then(Id::value)
        .ok_or("Bitget account info did not include a user ID")?;
    let parent = data.parent_id.filter(Id::truthy).and_then(Id::value);
    let (user_id, sub_id) = match parent {
        Some(parent) => (parent, Some(own_id)),
        None => (own_id, None),
    };
    Ok(Identity {
        user_id,
        sub_id,
        affiliate_id: data.inviter_id.filter(Id::truthy).and_then(Id::value),
    })
}
fn finish(cx: &mut Cx<'_, Pending>, root: &Context, result: Result<Identity, VenueError>) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity).map_err(|error| VenueError::new(error.to_string()))
    });
    match result {
        Ok(data) => {
            if cx.validate_read_http_delivery(root, root).is_ok() {
                let _ = cx.done_context(root, data);
            }
        }
        Err(error) => {
            if cx.command_error_context(root, &error.message).is_ok() {
                let _ = cx.fail_context(root, error);
            }
        }
    }
}
impl BitgetFutures {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>) {
        const PATH: &str = "/api/v2/spot/account/info";
        let refusal = if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            Some(VenueError::new(error))
        } else if !self.private_enabled() {
            Some(VenueError::new(
                "Bitget account identity requires credentials",
            ))
        } else {
            None
        };
        let (credential, identity) = if refusal.is_none() {
            self.read_selection(PATH)
        } else {
            (None, None)
        };
        // A rejected admission already published the command's typed failure.
        let Ok(root) = cx.admit_http_command(crate::read_policy(credential)) else {
            return;
        };
        if let Some(error) = refusal {
            finish(cx, &root, Err(error));
            return;
        }
        let recipe = sign::Http::new(identity, PATH.into());
        if let Err((pending, reason)) = cx.request_signed_managed(
            root.clone(),
            Method::Get,
            format!("{}{PATH}", paths::REST),
            None,
            Box::new(recipe),
            Pending::Identity(root),
        ) {
            self.on_sign_error(cx, pending, reason.venue_error());
        }
    }

    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        root: &Context,
        response: Response<'_>,
    ) {
        finish(cx, root, parse(response).map_err(VenueError::new));
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        root: &Context,
        error: VenueError,
    ) {
        finish(cx, root, Err(error));
    }
}
