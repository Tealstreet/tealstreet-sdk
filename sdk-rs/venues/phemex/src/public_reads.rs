//! Native optional public signing under one managed Background Read.
use crate::{Pending, PhemexPerpetuals, dispatch_signing};
use safe_cex_engine::credential_selection::CredentialUnavailable;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::selected_read::{
    AdmittedRead, BackgroundReadError, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::{Cx, Method, RequestId, VenueError};

pub enum PublicRead {
    Signed(AdmittedRead),
    Unsigned(Context),
}
impl PublicRead {
    pub(crate) fn context(&self) -> &Context {
        match self {
            Self::Signed(read) => read.context(),
            Self::Unsigned(context) => context,
        }
    }
    #[allow(clippy::too_many_arguments, clippy::result_large_err)]
    pub(crate) fn queue(
        self,
        cx: &mut Cx<'_, Pending>,
        testnet: bool,
        rest: &str,
        path: &str,
        query: String,
        stream: Option<ArrayAt>,
        pending: impl FnOnce(Self) -> Pending,
    ) -> Result<RequestId, (Pending, safe_cex_engine::request_context::Error)> {
        let url = format!(
            "{rest}{path}{}",
            if query.is_empty() {
                String::new()
            } else {
                format!("?{query}")
            }
        );
        match self {
            Self::Signed(read) => read.queue(
                cx,
                ReadRequest {
                    method: Method::Get,
                    url,
                    body: None,
                    stream,
                },
                |identity| {
                    Box::new(dispatch_signing::Http::new(
                        identity.clone(),
                        testnet,
                        Method::Get,
                        path.into(),
                        query,
                        None,
                    ))
                },
                |read| pending(Self::Signed(read)),
            ),
            Self::Unsigned(context) => {
                let id = cx.request_managed(
                    context.clone(),
                    None,
                    Method::Get,
                    url,
                    None,
                    pending(Self::Unsigned(context)),
                )?;
                if let Some(stream) = stream {
                    cx.stream_response(id, stream);
                }
                Ok(id)
            }
        }
    }
}
impl PhemexPerpetuals {
    pub(crate) fn public_read(&self, cx: &mut Cx<'_, Pending>) -> Result<PublicRead, VenueError> {
        let policy = ReadPolicy {
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        match SelectedRead::select(&self.public_read_slots, ReadRole::Read { scope: None }) {
            Ok(selected) => SelectedRead::background(cx, Ok(selected), policy)
                .map(PublicRead::Signed)
                .map_err(|error| match error {
                    BackgroundReadError::Admission(error) => error.read_venue_error(),
                    BackgroundReadError::Selection(_) => {
                        VenueError::new("Phemex public read requires credentials")
                    }
                }),
            Err(CredentialUnavailable::Missing) => cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .map(PublicRead::Unsigned)
                .map_err(|error| error.read_venue_error()),
        }
    }
}
impl Pending {
    pub(crate) fn public_read_context(&self) -> Option<&Context> {
        match self {
            Self::Markets(_, _, read) | Self::Tickers(_, _, read) | Self::Depth { read, .. } => {
                Some(read.context())
            }
            _ => None,
        }
    }
}
