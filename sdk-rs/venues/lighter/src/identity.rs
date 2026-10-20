//! Unsigned identities do not start or alter the private account/signing lifecycle.
use crate::{Lighter, Pending, account::Lookup};
use safe_cex_engine::{CommandId, Cx, Method, Response};
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
fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, String>) {
    let result =
        result.and_then(|identity| serde_json::to_string(&identity).map_err(|e| e.to_string()));
    if let Err(error) = &result {
        cx.command_error(command, error);
    }
    cx.done(command, result);
}

pub struct Request {
    command: CommandId,
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
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        self.identity_next(
            cx,
            command,
            self.lookup_candidates(),
            "Lighter identity requires an account index or L1 address selector",
        );
    }
    fn identity_next(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        mut candidates: VecDeque<Lookup>,
        error: &str,
    ) {
        let Some(candidate) = candidates.pop_front() else {
            finish(cx, command, Err(error.into()));
            return;
        };
        cx.request(
            Method::Get,
            format!("{}/api/v1/{}", self.host(), candidate.path()),
            vec![],
            None,
            Pending::Identity(Request {
                command,
                candidates,
                address: candidate.is_address(),
            }),
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        match parse(response, request.address) {
            Ok(identity) => finish(cx, request.command, Ok(identity)),
            Err(error) => self.identity_failed(cx, request, &error),
        }
    }
    pub(crate) fn identity_failed(&self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        self.identity_next(cx, request.command, request.candidates, error);
    }
}
