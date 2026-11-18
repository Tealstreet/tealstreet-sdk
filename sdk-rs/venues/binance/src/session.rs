//! USD-M/COIN-M/PM listenKey upkeep as an engine session. Each reconnect
//! opens a new session generation, so a stale create or keepalive response
//! can no longer reach the account reducers.
use crate::account::Request;
use crate::{BinanceFutures, Pending, Query, sign};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Session, SessionPolicy};
use safe_cex_engine::{Cx, Method};

/// Same raw bound as the other Binance background requests.
const SESSION_TIMEOUT_MS: u32 = 20_000;

impl BinanceFutures {
    /// One SessionAuth selection per request. Livenet presents only the API
    /// key; testnet signs the request.
    pub(crate) fn listen_key_session_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        session: &mut Option<Session>,
        method: Method,
        request: Request,
    ) {
        let pending = Pending::Account(request);
        let Ok((selection, credentials)) = self.credentials.select(CredentialRole::SessionAuth)
        else {
            return;
        };
        let credential = selection.credential(credentials.credential.clone());
        // Binance returns the active key on create and extends it on
        // keepalive, so an extra create or keepalive after an unknown
        // outcome is harmless: the session is repeat-safe.
        if method == Method::Post || session.is_none() {
            if let Some(old) = session.take() {
                // Nothing parks on a listenKey session, so it has no waiters.
                let waiters = cx.retire_session(&old);
                debug_assert!(waiters.is_ok_and(|waiters| waiters.is_empty()));
            }
            match cx.open_session(SessionPolicy {
                credential: Some(credential.clone()),
                repeat_safe: true,
                // Signed: routed by its credential, never a session egress.
                private_egress: None,
            }) {
                Ok(opened) => *session = Some(opened),
                Err(_) => return self.session_unavailable(cx, pending),
            }
        }
        let context = session.as_ref().and_then(|session| {
            cx.session_context(
                session,
                Policy {
                    operation: Operation::Mutation,
                    credential: Some(credential),
                    timeout_ms: SESSION_TIMEOUT_MS,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .ok()
        });
        let Some(context) = context else {
            return self.session_unavailable(cx, pending);
        };
        let recipe = if self.options.testnet {
            self.signed_recipe(credentials.clone(), Query::new())
        } else {
            sign::Http::new(
                credentials.clone(),
                Query::new(),
                self.options.recv_window,
                false,
            )
        };
        let queued = cx.request_signed_managed(
            context.clone(),
            None,
            method,
            format!("{}{}", self.urls.account_rest, self.paths.listen_key),
            None,
            Box::new(recipe),
            Pending::Background(context, Box::new(pending)),
        );
        if let Err((pending, _)) = queued {
            self.session_unavailable(cx, pending);
        }
    }

    /// A local refusal, not a network failure: report it as such, then take
    /// the native create/keepalive retry path.
    fn session_unavailable(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        let pending = match pending {
            Pending::Background(context, pending) => {
                let _ = cx.finish_http_context(&context);
                *pending
            }
            pending => pending,
        };
        if let Pending::Account(request) = pending {
            self.account_request_failed(
                cx,
                request,
                safe_cex_engine::VenueError::new("listenKey session is unavailable"),
            );
        }
    }
}
