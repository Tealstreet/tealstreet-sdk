//! Primary-only native POST reads use the shared immutable selection.
use crate::Cryptocom;
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::selected_read::{ReadRole, SelectedRead};
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
