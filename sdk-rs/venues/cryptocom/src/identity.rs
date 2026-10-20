//! A rare primary-signed identity read, separate from live reconciliation.
use crate::{Cryptocom, Pending, sign::Params, wire::RestEnvelope};
use safe_cex_engine::{CommandId, Cx, Response};
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
    if let Err(error) = &result {
        cx.command_error(command, error);
    }
    cx.done(command, result);
}

pub struct Request {
    command: CommandId,
    retries: u8,
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
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        if self.options.public_data_only || self.credentials.is_none() {
            finish(
                cx,
                command,
                Err("Crypto.com account identity requires credentials".into()),
            );
            return;
        }
        self.identity_request(
            cx,
            Request {
                command,
                retries: 0,
            },
        );
    }
    fn identity_request(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        self.signed(
            cx,
            "private/get-accounts",
            &Params::default(),
            Pending::Identity(request),
            false,
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
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
            Ok(Some(identity)) => finish(cx, request.command, Ok(identity)),
            Err(error) => finish(cx, request.command, Err(error)),
        }
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: &str,
    ) {
        if request.retries < 3 {
            request.retries += 1;
            self.identity_request(cx, request);
        } else {
            finish(cx, request.command, Err(error.into()));
        }
    }
}
