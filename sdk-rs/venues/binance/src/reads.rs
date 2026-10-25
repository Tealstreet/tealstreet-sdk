//! Explicit read adapters; all send and completion authority stays in the engine.
use crate::{BinanceFutures, Credentials, Pending, Query};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Credential, Operation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, ErrorKind, Method, Venue, VenueError};

pub(crate) struct Read {
    pub url: String,
    pub credentials: Option<Credentials>,
    pub query: Query,
    pub stream: Option<ArrayAt>,
}

pub(crate) fn policy(credential: Option<Credential>) -> Policy {
    Policy {
        operation: Operation::Read,
        credential,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

impl BinanceFutures {
    pub(crate) fn read_context(
        cx: &mut Cx<'_, Pending>,
        credential: Option<Credential>,
    ) -> Result<Context, CommandEntryError> {
        cx.admit_http_command(policy(credential))
    }

    pub(crate) fn background_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        credential: Option<Credential>,
        pending: Pending,
    ) {
        let context = match cx.http_context(Scope::Background, policy(credential)) {
            Ok(context) => context,
            Err(_) => {
                self.on_sign_error(
                    cx,
                    pending,
                    VenueError::new("Binance read lifecycle is unavailable")
                        .kind(ErrorKind::Internal),
                );
                return;
            }
        };
        let pending = Pending::Background(context.clone(), Box::new(pending));
        self.managed_read(cx, context, read, pending);
    }

    /// Explicit native account snapshots, independent of session-token writes.
    pub(crate) fn background_signed_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        query: Query,
        stream: Option<ArrayAt>,
        pending: Pending,
    ) {
        let Ok((selection, credentials)) = self
            .credentials
            .select(CredentialRole::Read { scope: None })
        else {
            self.on_sign_error(
                cx,
                pending,
                VenueError::new("Binance read requires credentials").kind(ErrorKind::InvalidInput),
            );
            return;
        };
        self.background_read(
            cx,
            Read {
                url: format!("{}{path}", self.urls.account_rest),
                credentials: Some(credentials.clone()),
                query,
                stream,
            },
            Some(selection.credential(credentials.credential.clone())),
            pending,
        );
    }

    pub(crate) fn managed_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        read: Read,
        pending: Pending,
    ) {
        let queued = match read.credentials {
            Some(credentials) => cx.request_signed_managed(
                context,
                None,
                Method::Get,
                read.url,
                None,
                Box::new(self.signed_recipe(credentials, read.query)),
                pending,
            ),
            None => cx.request_managed(context, None, Method::Get, read.url, None, pending),
        };
        match queued {
            Ok(request) => {
                if let Some(at) = read.stream {
                    cx.stream_response(request, at);
                }
            }
            Err((pending, _)) => self.on_sign_error(
                cx,
                pending,
                VenueError::new("Binance read preparation is unavailable")
                    .kind(ErrorKind::Internal),
            ),
        }
    }
}
