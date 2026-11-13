//! Shared local admission helpers for both hosted signing modes. Session policy,
//! admission pacing and venue activation stay with the existing join owner.
use safe_cex_crypto::admission::{PublicKey, Statement};
use safe_cex_engine::signing::{KeyRef, Provider, SignError};
use safe_cex_native::{HostEvent, Transport};
use safe_cex_venues::lighter_registration::{
    self, MAX_RESPONSE_BYTES, Network, Refused, TIMEOUT_MS,
};
use std::{sync::Arc, time::Duration};

/// One server-issued statement, tied to its original connection. A response
/// consumes the attempt on every outcome. This helper never attaches an account.
pub struct LocalProof {
    connection: u64,
    statement: Statement,
    public: PublicKey,
    consumed: bool,
    deadline: tokio::time::Instant,
}
impl LocalProof {
    pub fn issue(
        connection: u64,
        attempt: &str,
        identity_digest: [u8; 32],
        public: PublicKey,
        now_ms: u64,
    ) -> Result<Self, SignError> {
        let mut nonce = [0; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| SignError::Unavailable)?;
        Ok(Self {
            connection,
            statement: Statement::new(attempt, identity_digest, nonce, now_ms)?,
            public,
            consumed: false,
            deadline: tokio::time::Instant::now() + safe_cex_remote_signer::SIGN_TIMEOUT,
        })
    }
    pub fn statement(&self) -> &Statement {
        &self.statement
    }
    pub fn verify(&mut self, connection: u64, now_ms: u64, signature: &[u8]) -> bool {
        self.consume(connection, now_ms) && self.public.verify(&self.statement, signature)
    }
    /// The candidate is already installed in the existing temporary local
    /// provider. Derive through its opaque ref; never export or compare secrets.
    pub fn verify_candidate(
        &mut self,
        connection: u64,
        now_ms: u64,
        provider: &Provider,
        key: &KeyRef,
    ) -> Result<bool, SignError> {
        if !self.consume(connection, now_ms) {
            return Ok(false);
        }
        self.public.matches_provider(provider, key)
    }
    fn consume(&mut self, connection: u64, now_ms: u64) -> bool {
        let consumed = std::mem::replace(&mut self.consumed, true);
        !consumed
            && connection == self.connection
            && tokio::time::Instant::now() <= self.deadline
            && self.statement.current_at(
                now_ms,
                safe_cex_remote_signer::SIGN_TIMEOUT.as_millis() as u64,
            )
    }
}

/// Use the account's existing routed transport and captured IO authority.
/// Dropping this future closes its event channel, canceling native IO. Each
/// continuation checks the captured owner before accepting any public metadata.
pub async fn fetch_lighter_registered_key<T: Transport>(
    transport: &T,
    network: Network,
    account: u64,
    index: u8,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<PublicKey, Refused> {
    if !allowed() {
        return Err(Refused);
    }
    let request = lighter_registration::request(network, account, index)?;
    let id = request.id;
    let work = async {
        let prepared = transport
            .prepare_http(
                request.clone(),
                safe_cex_engine::request_context::RoutingIntent::Public,
            )
            .await
            .map_err(|_| Refused)?;
        if !allowed() {
            return Err(Refused);
        }
        let bound = prepared.bind(request).map_err(|_| Refused)?;
        if !bound.ready() || !allowed() {
            return Err(Refused);
        }
        let (events, mut receive) = tokio::sync::mpsc::unbounded_channel();
        struct AbortOnDrop(tokio::task::AbortHandle);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _request = AbortOnDrop(bound.dispatch(events));
        let mut started = false;
        let mut body = Vec::new();
        while let Some(event) = receive.recv().await {
            if !allowed() {
                return Err(Refused);
            }
            match event {
                HostEvent::HttpStart {
                    request,
                    status: 200,
                    ..
                } if request == id && !started => started = true,
                HostEvent::HttpChunk(request, chunk) if request == id && started => {
                    if chunk.len() > MAX_RESPONSE_BYTES - body.len() {
                        return Err(Refused);
                    }
                    body.extend_from_slice(&chunk);
                }
                HostEvent::HttpEnd(request) if request == id && started => {
                    let public = lighter_registration::decode(&body, account, index)?;
                    let public =
                        PublicKey::new(safe_cex_crypto::admission::Family::Schnorr, &public)
                            .map_err(|_| Refused)?;
                    return if allowed() { Ok(public) } else { Err(Refused) };
                }
                _ => return Err(Refused),
            }
        }
        Err(Refused)
    };
    tokio::time::timeout(Duration::from_millis(u64::from(TIMEOUT_MS)), work)
        .await
        .map_err(|_| Refused)?
}
