//! A rare configuration validation must not change account settings or reconciliation state.
use crate::{Pending, Task, WeexFutures, account_wire::AccountConfig};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    affiliate_id: Option<String>,
}
fn finish(cx: &mut Cx<'_, Pending>, command: CommandId, result: Result<Identity, VenueError>) {
    let result = result.and_then(|identity| {
        serde_json::to_string(&identity).map_err(|e| VenueError::new(e.to_string()))
    });
    match result {
        Ok(data) => cx.done(command, data),
        Err(error) => {
            cx.command_error(command, &error.message);
            cx.fail(command, error);
        }
    }
}

/// The account ID is the caller-assigned primary stableId from trusted setup.
fn unavailable() -> VenueError {
    VenueError::new("WEEX account identity requires a trusted primary stableId")
        .kind(ErrorKind::Unsupported)
        .outcome(Outcome::NotSent)
}
#[derive(Deserialize)]
struct Status {
    #[serde(default)]
    code: Option<Code>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Code {
    Text(String),
    Integer(u64),
}
impl Code {
    fn valid(&self) -> bool {
        match self {
            Self::Text(s) => matches!(s.as_str(), "0" | "200"),
            Self::Integer(n) => matches!(n, 0 | 200),
        }
    }
}
impl WeexFutures {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if self.invalid_read_credentials {
            finish(cx, command, Err(crate::READ_CREDENTIALS_INVALID.into()));
            return;
        }
        let (Some(_), Some(signer)) = (
            &self.credentials.primary,
            self.credentials.select(Method::Get),
        ) else {
            finish(
                cx,
                command,
                Err("WEEX account identity requires a non-public primary key".into()),
            );
            return;
        };
        if self.identity.is_none() {
            finish(cx, command, Err(unavailable()));
            return;
        }
        if cx.now < self.paused_until {
            finish(
                cx,
                command,
                Err("WEEX account reads are rate limited".into()),
            );
            return;
        }
        if let Err(error) = crate::managed::read(
            cx,
            signer,
            command,
            "/capi/v3/account/accountConfig".into(),
            self.seed_generation,
            |context| Task::Identity { context },
        ) {
            finish(cx, command, Err(error));
        }
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        response: Response<'_>,
    ) {
        let validated = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!("WEEX account identity HTTP {}", response.status));
            }
            let status: Status = serde_json::from_str(response.body)
                .map_err(|_| "Invalid WEEX identity response")?;
            if status.code.as_ref().is_some_and(|code| !code.valid()) {
                return Err("WEEX account identity validation rejected".into());
            }
            let config: AccountConfig = serde_json::from_str(response.body)
                .map_err(|_| "Invalid WEEX account identity configuration")?;
            if !config.can_trade {
                return Err("WEEX API key is valid but trading is disabled".into());
            }
            Ok(())
        })();
        let result = validated
            .map_err(VenueError::new)
            .and_then(|()| self.identity.clone().ok_or_else(unavailable))
            .and_then(|user_id| {
                serde_json::to_string(&Identity {
                    user_id,
                    affiliate_id: None,
                })
                .map_err(|e| VenueError::new(e.to_string()))
            });
        match result {
            Ok(data) => crate::managed::complete(cx, context, data),
            Err(error) => Self::identity_failed(cx, context, error),
        }
    }
    pub(crate) fn identity_failed(
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        error: impl Into<VenueError>,
    ) {
        let error = error.into();
        let _ = cx.command_error_context(context, error.message.clone());
        crate::managed::reject(cx, context, error);
    }
}
