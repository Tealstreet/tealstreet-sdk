//! Shared signed Read admission; native reducers retain their own freshness checks.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::credential_selection::{CredentialUnavailable, SlotPolicy, Slots};
use safe_cex_engine::request_context::RetryPolicy;
use safe_cex_engine::selected_read::{
    AdmittedRead, BackgroundReadError, ReadPolicy, ReadRole, SelectedRead,
};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::{Cx, VenueError};

pub struct Background<T> {
    pub(crate) read: AdmittedRead,
    pub(crate) native: T,
}
pub(crate) fn read_slots(
    primary: Option<Identity>,
    secondary: Option<Identity>,
) -> (Option<Identity>, Slots<Identity>) {
    let slots = Slots::new(
        primary,
        secondary.clone(),
        SlotPolicy {
            secondary_requires_primary: true,
        },
    );
    (secondary, slots)
}
impl PhemexPerpetuals {
    pub(crate) fn selected_read(
        &self,
        role: ReadRole,
    ) -> Result<SelectedRead, CredentialUnavailable> {
        SelectedRead::select(&self.read_slots, role)
    }
    pub(crate) fn background_read(
        &self,
        cx: &mut Cx<'_, Pending>,
        role: ReadRole,
    ) -> Result<AdmittedRead, VenueError> {
        SelectedRead::background(
            cx,
            self.selected_read(role),
            ReadPolicy {
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .map_err(|error| match error {
            BackgroundReadError::Selection(_) => {
                VenueError::new("Phemex account read requires credentials")
            }
            BackgroundReadError::Admission(error) => error.read_venue_error(),
        })
    }
}
