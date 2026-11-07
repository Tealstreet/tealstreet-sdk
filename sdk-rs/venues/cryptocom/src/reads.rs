//! Primary-only native POST reads use the shared immutable selection.
use crate::Cryptocom;
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::selected_read::{ReadRole, SelectedRead};

#[derive(Clone, Copy)]
pub(crate) enum FailureRecovery {
    Strict,
    Response,
    Transport,
}
impl FailureRecovery {
    pub(crate) fn released(error: &safe_cex_engine::VenueError) -> Self {
        match error.kind {
            Some(safe_cex_engine::ErrorKind::Network | safe_cex_engine::ErrorKind::Timeout) => {
                Self::Transport
            }
            _ => Self::Strict,
        }
    }
    pub(crate) fn retry(self, message: &str, retries: u8) -> bool {
        match self {
            Self::Strict => false,
            Self::Response => message.contains("40101") && retries < 1,
            Self::Transport if message.contains("40101") => retries < 1,
            Self::Transport => retries < 3,
        }
    }
}
impl Cryptocom {
    pub(crate) fn selected_read(&self) -> Result<SelectedRead, CredentialUnavailable> {
        SelectedRead::select(&self.slots, ReadRole::Read { scope: None })
    }
}

pub(crate) fn response_failure(
    response: safe_cex_engine::Response<'_>,
    message: impl Into<String>,
) -> safe_cex_engine::VenueError {
    use safe_cex_engine::classify::{self, Native, NativeCode, Released};
    let envelope = serde_json::from_str::<crate::wire::RestEnvelope<'_>>(response.body).ok();
    let native = envelope.as_ref().map(|value| Native {
        code: NativeCode::Int(value.code),
        message: value.message,
    });
    classify::read(
        &crate::trading::ERRORS,
        Released::Response(&response),
        native.as_ref(),
        message,
    )
    .error
}
