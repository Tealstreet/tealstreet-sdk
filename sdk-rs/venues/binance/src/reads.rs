//! Explicit read adapters; all send and completion authority stays in the engine.
use crate::{BinanceFutures, Credentials, Pending, Query, sign};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Venue, VenueError};

impl BinanceFutures {
    pub(crate) fn read_context(
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        credential: Option<Credential>,
    ) -> Result<Context, VenueError> {
        cx.http_context(
            Scope::Command(command),
            Policy {
                operation: Operation::Read,
                credential,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .map_err(|_| {
            VenueError::new("Binance read lifecycle is unavailable").kind(ErrorKind::Internal)
        })
    }

    pub(crate) fn managed_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        url: String,
        credentials: Option<Credentials>,
        stream: Option<ArrayAt>,
        pending: Pending,
    ) {
        let queued = match credentials {
            Some(credentials) => cx.request_signed_managed(
                context,
                Method::Get,
                url,
                None,
                Box::new(sign::Http::new(
                    credentials,
                    Query::new(),
                    self.options.recv_window,
                    true,
                )),
                pending,
            ),
            None => cx.request_managed(context, Method::Get, url, None, pending),
        };
        match queued {
            Ok(request) => {
                if let Some(at) = stream {
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
