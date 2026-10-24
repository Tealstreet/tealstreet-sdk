//! Non-trading reads under engine custody. Each request keeps its own native
//! one-time signature resend and three-retry budgets
//! (docs/plans/RUST_APEXOMNI_READ_CONTEXT.md).
use crate::{ApexOmni, Pending, account, history, public, sign};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{Context, Error, RetryEvidence};
use safe_cex_engine::selected_read::{AdmittedRead, ReadLeg, ReadLegPolicy, ReadRequest, ReadRoot};
use safe_cex_engine::{Cx, Method, Response, VenueError};

pub(crate) const TIMEOUT_MS: u32 = 20_000;
pub(crate) const MISSING: &str = "ApeX API key, signing key and passphrase are required";

pub enum Custody {
    /// Symbols without a passphrase ref: a credential-free Background read.
    Public(Context),
    /// A Background snapshot read, or the FetchAccount command read.
    Admitted(AdmittedRead),
    /// One physical history page under the caller's original root.
    Leg(ReadLeg),
}

impl Custody {
    fn context(&self) -> &Context {
        match self {
            Self::Public(context) => context,
            Self::Admitted(read) => read.context(),
            Self::Leg(leg) => leg.context(),
        }
    }
}

pub struct Read {
    custody: Custody,
    unsigned: sign::Request,
    at: Option<ArrayAt>,
    legacy: bool,
    retries: u8,
}

pub(crate) enum Native {
    Public(public::Request),
    Account(account::Request),
    History(Box<history::Request>),
}

impl Native {
    fn pending(self, read: Read) -> Pending {
        match self {
            Self::Public(request) => Pending::Public(Box::new(read), request),
            Self::Account(request) => Pending::Account(Box::new(read), request),
            Self::History(request) => Pending::History(Box::new(read), request),
        }
    }

    fn root(&self) -> Option<&ReadRoot> {
        match self {
            Self::History(request) => Some(request.root()),
            _ => None,
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Public(request) => request.reset(),
            Self::Account(request) => request.reset(),
            Self::History(request) => request.reset(),
        }
    }

    /// Background reads retire their own context; command roots end at their terminal.
    fn background(&self) -> bool {
        !matches!(
            self,
            Self::Account(account::Request::Identity) | Self::History(_)
        )
    }
}

impl Read {
    pub(crate) fn context(&self) -> &Context {
        self.custody.context()
    }

    fn delivered<P>(&self, cx: &Cx<'_, P>, root: Option<&ReadRoot>) -> bool {
        match &self.custody {
            Custody::Public(context) => cx.validate_background_read_http_delivery(context).is_ok(),
            Custody::Admitted(read) => {
                let context = read.context();
                cx.validate_background_read_http_delivery(context).is_ok()
                    || cx.validate_read_http_delivery(context, context).is_ok()
            }
            Custody::Leg(leg) => root.is_some_and(|root| root.validate_delivery(cx, leg).is_ok()),
        }
    }
}

pub(crate) enum Split {
    Read(Box<Read>, Native),
    Candles(public::Candles),
    Trade(crate::trading::Request),
}

pub(crate) fn split(pending: Pending) -> Split {
    match pending {
        Pending::Public(read, request) => Split::Read(read, Native::Public(request)),
        Pending::Account(read, request) => Split::Read(read, Native::Account(request)),
        Pending::History(read, request) => Split::Read(read, Native::History(request)),
        Pending::Candles(request) => Split::Candles(request),
        Pending::Trade(request) => Split::Trade(request),
    }
}

impl ApexOmni {
    /// Queue one read under `custody`; a refused queue settles it before any send.
    pub(crate) fn send_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        custody: Custody,
        request: sign::Request,
        at: Option<ArrayAt>,
        native: Native,
    ) {
        self.queue_read(cx, custody, request, at, false, 0, native);
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        custody: Custody,
        request: sign::Request,
        at: Option<ArrayAt>,
        legacy: bool,
        retries: u8,
        native: Native,
    ) {
        let target = sign::request_path(&request);
        let url = format!("{}{target}", self.rest);
        let wire = ReadRequest {
            method: request.method,
            url,
            body: None,
            stream: at,
        };
        let unsigned = request.clone();
        let read = |custody| Read {
            custody,
            unsigned: request,
            at,
            legacy,
            retries,
        };
        let recipe = move |identity: &safe_cex_engine::signing::Identity| {
            Box::new(sign::Http::new(identity.clone(), unsigned, legacy))
                as Box<dyn safe_cex_engine::SignStep>
        };
        let queued = match custody {
            Custody::Public(context) => {
                let pending = native.pending(read(Custody::Public(context.clone())));
                cx.request_managed(context, wire.method, wire.url, None, pending)
                    .map(|id| {
                        if let Some(at) = wire.stream {
                            cx.stream_response(id, at);
                        }
                    })
            }
            Custody::Admitted(admitted) => admitted
                .queue(cx, wire, recipe, |admitted| {
                    native.pending(read(Custody::Admitted(admitted)))
                })
                .map(drop),
            Custody::Leg(leg) => leg
                .queue(cx, wire, recipe, |leg| {
                    native.pending(read(Custody::Leg(leg)))
                })
                .map(drop),
        };
        if let Err((pending, error)) = queued {
            self.failed(cx, pending, error.venue_error());
        }
    }

    /// A native resend or retry under engine authority from the same custody;
    /// history pages re-mint from their original root. No renewed root or reselection.
    fn resend(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        mut native: Native,
        legacy: bool,
        retries: u8,
        error: VenueError,
    ) {
        let Read {
            custody,
            unsigned: request,
            at,
            ..
        } = read;
        let renewed = match custody {
            Custody::Public(context) => Err((Custody::Public(context), Error::RetryForbidden)),
            Custody::Admitted(admitted) => admitted
                .retry(cx, RetryEvidence::ReadFailure, ())
                .map(|(admitted, ())| Custody::Admitted(admitted))
                .map_err(|refusal| (Custody::Admitted(refusal.read), refusal.error)),
            Custody::Leg(leg) => match native.root() {
                Some(root) => root
                    .leg(
                        cx,
                        ReadLegPolicy {
                            timeout_ms: TIMEOUT_MS,
                            pacing: None,
                        },
                    )
                    .map(Custody::Leg)
                    .map_err(|error| (Custody::Leg(leg), error)),
                None => Err((Custody::Leg(leg), Error::Invalid)),
            },
        };
        let custody = match renewed {
            Ok(custody) => custody,
            Err((custody, _)) => {
                let read = Read {
                    custody,
                    unsigned: request,
                    at,
                    legacy,
                    retries,
                };
                self.read_failed(cx, read, native, error);
                return;
            }
        };
        native.reset();
        self.queue_read(cx, custody, request, at, legacy, retries, native);
    }

    pub(crate) fn read_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        native: Native,
        response: Response<'_>,
    ) {
        if !read.delivered(cx, native.root()) {
            // A command terminal belongs to the engine; Background state settles natively.
            if native.background() {
                let error = VenueError::new("ApeX read delivery is no longer current");
                self.read_failed(cx, read, native, error);
            }
            return;
        }
        match crate::api_response(response) {
            crate::ApiResponse::SignatureRefusal(error) if !read.legacy => {
                // The venue refused the first signature; a refused resend never leaves.
                let error = error.outcome(safe_cex_engine::Outcome::Rejected);
                let retries = read.retries;
                self.resend(cx, read, native, true, retries, error);
            }
            crate::ApiResponse::Failed(error) | crate::ApiResponse::SignatureRefusal(error) => {
                self.read_failed(cx, read, native, error);
            }
            crate::ApiResponse::Success => {
                let background = native.background();
                let context = read.context().clone();
                match native {
                    Native::Public(request) => self.public_response(cx, request, response),
                    Native::Account(request) => {
                        self.account_response(cx, request, &context, response)
                    }
                    Native::History(request) => {
                        self.history_response(cx, *request, &context, response)
                    }
                }
                if background {
                    let _ = cx.finish_http_context(&context);
                }
            }
        }
    }

    pub(crate) fn read_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &mut Pending,
        item: &str,
    ) {
        match pending {
            Pending::Public(read, request) if read.delivered(cx, None) => {
                self.public_item(cx, request, item)
            }
            Pending::Account(read, request) if read.delivered(cx, None) => {
                self.account_item(cx, request, item)
            }
            Pending::History(read, request) if read.delivered(cx, Some(request.root())) => {
                self.history_item(cx, request, item)
            }
            _ => {}
        }
    }

    pub(crate) fn read_transport_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        native: Native,
        error: &str,
    ) {
        let error = crate::trading::unknown(error).kind(safe_cex_engine::ErrorKind::Network);
        // Markets defer to the startup backoff.
        if read.unsigned.method == Method::Get
            && read.retries < 3
            && !matches!(native, Native::Public(_))
        {
            let (legacy, retries) = (read.legacy, read.retries + 1);
            self.resend(cx, read, native, legacy, retries, error);
        } else {
            self.read_failed(cx, read, native, error);
        }
    }

    pub(crate) fn read_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        native: Native,
        error: VenueError,
    ) {
        let background = native.background();
        let context = read.context().clone();
        match native {
            Native::Public(request) => self.public_failed(cx, request, &error.message),
            Native::Account(request) => self.account_failed(cx, request, &context, error),
            Native::History(request) => self.history_failed(cx, *request, &context, &error.message),
        }
        if background {
            let _ = cx.finish_http_context(&context);
        }
    }
}
