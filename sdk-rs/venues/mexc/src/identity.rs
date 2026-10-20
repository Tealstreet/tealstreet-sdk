//! Validate the selected read key, but retain the TS primary-key fingerprint namespace.
use crate::{
    MexcFutures, Pending,
    wire::{Asset, Envelope},
};
use safe_cex_engine::{CommandId, Cx, Method, Response};
use serde::Serialize;
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

fn fingerprint(key: &str) -> String {
    let hash = safe_cex_crypto::sha256(key.as_bytes());
    let prefix: String = hash[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("mexc:{prefix}")
}
impl MexcFutures {
    pub(crate) fn fetch_identity(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            finish(cx, command, Err(error.into()));
            return;
        }
        let primary = self.options.key.as_deref().filter(|key| !key.is_empty());
        let signer = self
            .options
            .read_credentials
            .as_ref()
            .map(|read| read.secret.as_str())
            .or_else(|| {
                self.credentials
                    .as_ref()
                    .map(|credentials| credentials.secret.as_str())
            });
        if self.options.public_data_only || primary.is_none() || signer.is_none_or(str::is_empty) {
            finish(
                cx,
                command,
                Err("MEXC account identity requires credentials and a primary key".into()),
            );
            return;
        }
        if cx.now < self.account.paused_until {
            finish(
                cx,
                command,
                Err("MEXC account reads are rate limited".into()),
            );
            return;
        }
        self.signed(
            cx,
            Method::Get,
            "/api/v1/private/account/assets",
            String::new(),
            false,
            Pending::Identity(command),
        );
    }
    pub(crate) fn identity_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        let result = Envelope::checked(response.body).and_then(|envelope| {
            let _: Vec<Asset<'_>> = serde_json::from_str(envelope.payload()?)
                .map_err(|_| "Invalid MEXC identity assets")?;
            let primary = self
                .options
                .key
                .as_deref()
                .filter(|key| !key.is_empty())
                .ok_or("MEXC primary identity key is missing")?;
            Ok(Identity {
                user_id: fingerprint(primary),
                affiliate_id: None,
            })
        });
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
