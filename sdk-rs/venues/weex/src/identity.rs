//! A rare configuration validation must not change account settings or reconciliation state.
use crate::{Pending, Task, WeexFutures, account_wire::AccountConfig};
use safe_cex_engine::{CommandId, Cx, Method, Response};
use serde::{Deserialize, Serialize};
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

pub(crate) fn fingerprint(key: &str) -> String {
    let hash = safe_cex_crypto::sha256(key.as_bytes());
    let prefix: String = hash[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("weex:{prefix}")
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
        if let Some(read) = &self.read_credentials
            && let Err(error) = read.validate().and_then(|()| {
                read.validate_passphrase(self.credentials.as_ref().map(|c| c.passphrase.as_str()))
            })
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        if self.identity.is_none() {
            finish(
                cx,
                command,
                Err("WEEX account identity requires a non-public primary key".into()),
            );
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
        if !self.signed_request(
            cx,
            Method::Get,
            "/capi/v3/account/accountConfig",
            "",
            None,
            Task::Identity(command),
            None,
        ) {
            finish(
                cx,
                command,
                Err("WEEX account identity requires credentials".into()),
            );
        }
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        let result = (|| {
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
            let user_id = self
                .identity
                .clone()
                .ok_or("WEEX primary identity key is missing")?;
            Ok(Identity {
                user_id,
                affiliate_id: None,
            })
        })();
        finish(cx, command, result);
    }
    pub(crate) fn identity_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: &str,
    ) {
        finish(cx, command, Err(error.into()));
    }
}
