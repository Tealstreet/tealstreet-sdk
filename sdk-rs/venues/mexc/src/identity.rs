//! Validation uses the selected Read; persisted identity is trusted Primary metadata.
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
impl MexcFutures {
    pub(crate) fn fetch_identity(&mut self, cx: &mut Cx<'_, Pending>, _command: CommandId) {
        let error = if self.options.read_credentials_valid == Some(false) {
            Some("Invalid MEXC read credentials")
        } else if self.options.public_data_only
            || self.options.identities.primary.is_none()
            || self.read_selection(false, None).is_err()
        {
            Some("MEXC account identity requires credentials and a primary key")
        } else if self
            .options
            .identities
            .primary
            .as_ref()
            .is_none_or(|primary| primary.require_stable_id().is_err())
        {
            Some("MEXC primary account identity mapping is required")
        } else if cx.now < self.account.paused_until {
            Some("MEXC account reads are rate limited")
        } else {
            None
        };
        if let Some(error) = error {
            let Ok(context) = cx.admit_http_command(safe_cex_engine::request_context::Policy {
                operation: safe_cex_engine::request_context::Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: safe_cex_engine::request_context::RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.command_error_context(&context, error);
            let _ = cx.fail_context(&context, error);
            return;
        }
        self.signed(
            cx,
            Method::Get,
            "/api/v1/private/account/assets",
            String::new(),
            false,
            Pending::Identity(_command),
        );
    }
    pub(crate) fn identity_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _command: CommandId,
        response: Response<'_>,
    ) {
        let result = Envelope::checked(response.body).and_then(|envelope| {
            let _: Vec<Asset<'_>> = serde_json::from_str(envelope.payload()?)
                .map_err(|_| "Invalid MEXC identity assets")?;
            let primary = self
                .options
                .identities
                .primary
                .as_ref()
                .ok_or("MEXC primary identity key is missing")?;
            let user_id = primary
                .require_stable_id()
                .map_err(|_| "MEXC primary account identity mapping is required")?
                .into();
            serde_json::to_string(&Identity {
                user_id,
                affiliate_id: None,
            })
            .map_err(|e| e.to_string())
        });
        match result {
            Ok(data) => self.private_done(cx, data),
            Err(error) => self.private_fail(cx, error, true),
        }
    }
    pub(crate) fn identity_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _command: CommandId,
        error: &str,
    ) {
        self.private_fail(cx, error, true);
    }
}
