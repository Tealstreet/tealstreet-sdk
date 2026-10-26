//! Unsigned identities do not start or alter the private account/signing lifecycle.
use crate::{Lighter, Pending, account::Lookup};
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Operation, Policy, RetryPolicy,
};
use safe_cex_engine::{Cx, Method, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Text(String),
    Integer(u64),
}
impl Id {
    fn text(self) -> String {
        match self {
            Self::Text(value) => value,
            Self::Integer(value) => value.to_string(),
        }
    }
    fn index(self) -> Option<String> {
        let text = self.text();
        let value = text.parse::<u64>().ok()?;
        (value <= i64::MAX as u64).then(|| value.to_string())
    }
}
#[derive(Deserialize)]
struct Row {
    #[serde(default)]
    account_index: Option<Id>,
    #[serde(default)]
    index: Option<Id>,
    #[serde(default, rename = "accountIndex")]
    legacy_index: Option<Id>,
    #[serde(default)]
    referrer: Option<Id>,
}
#[derive(Default, Deserialize)]
struct Data {
    #[serde(default, rename = "accountIndex")]
    account_index: Option<Id>,
    #[serde(default)]
    referrer: Option<Id>,
    #[serde(default)]
    accounts: Option<Vec<Row>>,
    #[serde(default)]
    sub_accounts: Option<Vec<Row>>,
}
#[derive(Deserialize)]
struct Envelope {
    code: Option<Id>,
    success: Option<bool>,
    accounts: Option<Vec<Row>>,
    sub_accounts: Option<Vec<Row>>,
    data: Option<Data>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    affiliate_id: Option<String>,
}
fn finish(cx: &mut Cx<'_, Pending>, root: &Context, result: Result<Identity, VenueError>) {
    if cx.validate_read_root_context(root).is_err() {
        return;
    }
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity).map_err(|error| VenueError::new(error.to_string()))
    });
    match result {
        Ok(data) => {
            let _ = cx.done_context(root, data);
        }
        Err(error) => {
            if cx.command_error_context(root, &error.message).is_ok() {
                let _ = cx.fail_context(root, error);
            }
        }
    }
}

pub struct Request {
    root: Context,
    leg: Context,
    candidates: VecDeque<Lookup>,
    address: bool,
}
fn parse(response: Response<'_>, address: bool) -> Result<Identity, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Lighter identity HTTP {}", response.status));
    }
    let envelope: Envelope = serde_json::from_str(response.body)
        .map_err(|_| "Invalid Lighter account identity response")?;
    let code = envelope.code.map(Id::text);
    if envelope.success == Some(false) || code.as_deref().is_some_and(|code| code != "200") {
        return Err("Lighter account identity lookup rejected".into());
    }
    let data = envelope.data.unwrap_or_default();
    let row = if address {
        envelope
            .sub_accounts
            .unwrap_or_default()
            .into_iter()
            .chain(envelope.accounts.unwrap_or_default())
            .chain(data.sub_accounts.unwrap_or_default())
            .chain(data.accounts.unwrap_or_default())
            .next()
    } else if envelope.success == Some(true) && data.account_index.is_some() {
        Some(Row {
            account_index: data.account_index,
            index: None,
            legacy_index: None,
            referrer: data.referrer,
        })
    } else if code.as_deref() == Some("200") {
        envelope.accounts.and_then(|rows| rows.into_iter().next())
    } else {
        None
    };
    let row = row.ok_or("Lighter identity lookup returned no account")?;
    let user_id = row
        .account_index
        .or(row.index)
        .or(row.legacy_index)
        .and_then(Id::index)
        .ok_or("Lighter identity lookup returned no valid account index")?;
    let affiliate_id = row
        .referrer
        .map(Id::text)
        .filter(|id| !id.trim().is_empty());
    Ok(Identity {
        user_id,
        affiliate_id,
    })
}
impl Lighter {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>) {
        let root = match cx.admit_caller_read_root(None, None) {
            Ok(root) => root,
            Err(CommandEntryError::Rejected(_)) => return,
            Err(CommandEntryError::Revoked) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
        };
        self.identity_next(
            cx,
            root,
            self.identity_candidates(),
            VenueError::new("Lighter identity requires an account index or L1 address selector"),
        );
    }
    fn identity_next(
        &self,
        cx: &mut Cx<'_, Pending>,
        root: Context,
        mut candidates: VecDeque<Lookup>,
        error: VenueError,
    ) {
        if cx.validate_read_root_context(&root).is_err() {
            return;
        }
        let Some(candidate) = candidates.pop_front() else {
            finish(cx, &root, Err(error));
            return;
        };
        let leg = match cx.http_leg_context(
            &root,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                finish(cx, &root, Err(error.venue_error()));
                return;
            }
        };
        let pending = Pending::Identity(Request {
            root: root.clone(),
            leg: leg.clone(),
            candidates,
            address: candidate.is_address(),
        });
        if let Err((_pending, error)) = cx.request_managed(
            leg,
            None,
            Method::Get,
            format!("{}/api/v1/{}", self.host(), candidate.path()),
            None,
            pending,
        ) {
            finish(cx, &root, Err(error.venue_error()));
        }
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if cx
            .validate_read_http_delivery(&request.root, &request.leg)
            .is_err()
        {
            return;
        }
        match parse(response, request.address) {
            Ok(identity) => finish(cx, &request.root, Ok(identity)),
            Err(error) => self.identity_failed(cx, request, VenueError::new(error)),
        }
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: impl Into<VenueError>,
    ) {
        if cx.validate_read_root_context(&request.root).is_err()
            || cx.finish_http_context(&request.leg).is_err()
        {
            return;
        }
        self.identity_next(cx, request.root, request.candidates, error.into());
    }
    pub(crate) fn identity_rejected(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        finish(cx, &request.root, Err(error));
    }
}
