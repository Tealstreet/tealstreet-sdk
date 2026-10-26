//! Nondefault actual-engine fixture; no live account or dispatcher override.
use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_engine::{Command, Cx, HttpRelease, Method, Response, VenueError};

struct Origin {
    ids: Identities,
    spot: bool,
}
impl Venue for Origin {
    type Pending = (Context, safe_cex_engine::CommandId);
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn http_release() -> HttpRelease<Self> {
        HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn dispatch_nonce() -> bool {
        true
    }
    fn start(&mut self, _: &mut Cx<'_, Self::Pending>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        let Command::FetchAccount { command } = command else {
            return;
        };
        let role = CredentialRole::Read {
            scope: Some(safe_cex_engine::AccountReadScope::Account),
        };
        let selected = if self.spot {
            safe_cex_venue_selected::spot(&self.ids, role)
        } else {
            safe_cex_venue_selected::futures(&self.ids, role)
        }
        .unwrap();
        let root = cx
            .admit_http_command(Policy {
                operation: Operation::Read,
                credential: Some(selected.credential()),
                timeout_ms: 20000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        let recipe = if self.spot {
            selected.spot_http("/0/private/Balance", "", 5000.)
        } else {
            selected.futures_http(
                Method::Get,
                "/derivatives/api/v3/accounts",
                "",
                false,
                5000.,
            )
        }
        .unwrap();
        let url = recipe.url().to_owned();
        let body = recipe.body().map(str::to_owned);
        cx.request_signed_managed(
            root.clone(),
            None,
            if self.spot { Method::Post } else { Method::Get },
            url,
            body,
            Box::new(recipe),
            (root, command),
        )
        .unwrap();
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        _: Response<'_>,
    ) {
        cx.done_context(&pending.0, "null".into()).unwrap();
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        let _ = cx.fail_context(&pending.0, error);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        let _ = cx.fail_context(&pending.0, error);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Self::Pending>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
use safe_cex_venues::kraken_selected as safe_cex_venue_selected;
#[wasm_bindgen(js_name=krakenHostedProofOrigin)]
pub fn origin(spot: bool, subject: String) -> Result<WasmEngine, JsError> {
    let mut identity = Identity::new(
        KeyRef("origin.credential".into()),
        Some(KeyRef("origin.signing".into())),
    );
    identity.stable_id = Some(subject);
    let ids = Identities {
        primary: Some(identity),
        secondary: None,
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(Origin {
        ids: ids.clone(),
        spot,
    });
    engine
        .set_identities(ids)
        .map_err(|_| JsError::new("Invalid fixture identity"))?;
    Ok(WasmEngine {
        inner: Box::new(engine),
        dispatch_signing: true,
        external_signer: true,
        construction: "{}".into(),
        #[cfg(feature = "dispatch-signing")]
        hosted_identity: None,
        #[cfg(feature = "dispatch-signing")]
        hosted_captures: Default::default(),
    })
}
