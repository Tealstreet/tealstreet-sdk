//! Capture venue registration through the existing routed public-read boundary.
use super::*;

pub(super) async fn resolve(
    _registry: &Registry,
    user: &str,
    init: &InitPayload,
    token: Option<(String, u64)>,
    link: Option<Arc<SignerLink>>,
    permit: &ProbePermit,
) -> Result<Projection, ProbeRefusal> {
    permit.check(user, init)?;
    let (account, index) = identity::lighter_indices(init).map_err(|_| ProbeRefusal::INVALID)?;
    let plan = routing_plan(init, false).map_err(|_| ProbeRefusal::INVALID)?;
    #[cfg(test)]
    let plan = _registry
        .construction_test_plans
        .get(user)
        .cloned()
        .unwrap_or(plan);
    let current = permit.current.clone();
    let lease = permit.lease.clone();
    let expiry = permit.token_expires_at;
    let deadline = permit.local_deadline;
    let captured_actor = permit
        .public_join
        .as_ref()
        .map(|proof| proof.account.clone());
    let allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
        current.load(Ordering::Acquire)
            && lease.as_ref().is_none_or(|lease| lease.allowed())
            && expiry.is_none_or(|expiry| expiry > now_ms())
            && deadline.is_none_or(|deadline| tokio::time::Instant::now() < deadline)
            && link.as_ref().is_none_or(|link| link.is_available())
            && captured_actor.as_ref().is_none_or(|account| {
                account.upgrade().is_some_and(|account| {
                    account.handle.disposal_cause().is_none() && account.running.io_allowed()
                })
            })
    });
    let proxy_token = ProxyToken::new();
    proxy_token.set(token);
    let transport = NetworkTransport::with_routing(plan, proxy_token.provider())
        .with_venue(&init.exchange_name)
        .with_io_guard(allowed.clone());
    #[cfg(test)]
    let transport = crate::delegated_e2e_io::Io::new(transport, _registry.e2e_venue.clone());
    let network = safe_cex_venues::lighter_registration::Network::from_options(
        init.exchange_name == "lighter-robinhood",
        init.testnet == Some(true),
    );
    let public = crate::admission::fetch_lighter_registered_key(
        &transport, network, account, index, allowed,
    )
    .await
    .map_err(|_| ProbeRefusal::TRANSIENT)?;
    permit.check(user, init)?;
    if permit
        .public_join
        .as_ref()
        .is_some_and(|proof| proof.public.as_bytes() != public.as_bytes())
    {
        return Err(ProbeRefusal::INVALID);
    }
    let facts = json!({"venue":init.exchange_name,
        "network":if init.testnet==Some(true) {"testnet"} else {"mainnet"},
        "credential":{"kind":"schnorr","accountIndex":account.to_string(),"apiKeyIndex":index,
            "registeredPublicKey":format!("0x{}",safe_cex_crypto::hex(public.as_bytes()))}});
    Projection::from_json(user, &facts.to_string()).map_err(|_| ProbeRefusal::INVALID)
}
