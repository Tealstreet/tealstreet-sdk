use super::*;
use safe_cex_venues::lighter_registration::{Network, TIMEOUT_MS};

impl Registry {
    pub(crate) fn resolve_identity(
        &self,
        user: &str,
        mut init: InitPayload,
        token: Option<(String, u64)>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> impl std::future::Future<Output = Result<InitPayload, String>> + Send + use<> {
        let captured_user = user.to_owned();
        let lease = self.router_leases.get(user).map(|lease| lease.permit());
        let authorized = !user.is_empty()
            && token.is_some()
            && (!self.router_leases.enforced() || lease.is_some());
        let expiry = token.as_ref().map(|(_, expiry)| *expiry);
        let allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            authorized
                && expiry.is_some_and(|expiry| expiry > now_ms())
                && lease.as_ref().is_none_or(|lease| lease.allowed())
        });
        let captured = allowed.clone();
        let metadata_allowed: Arc<dyn Fn() -> bool + Send + Sync> =
            Arc::new(move || captured() && current());
        let plan = routing_plan(&init, false);
        #[cfg(test)]
        let plan = self
            .construction_test_plans
            .get(user)
            .cloned()
            .map(Ok)
            .unwrap_or(plan);
        let proxy = ProxyToken::new();
        proxy.set(token);
        async move {
            if !metadata_allowed() {
                return Err("Account identity resolution expired".into());
            }
            let network = Network::from_options(
                init.exchange_name == "lighter-robinhood",
                init.testnet == Some(true),
            );
            let index = identity::decimal(init.api_key_index.as_deref(), 254)? as u8;
            let transport = NetworkTransport::with_routing(plan?, proxy.provider())
                .with_venue(&init.exchange_name)
                .with_io_guard(metadata_allowed.clone());
            let account = match identity::decimal(init.application_id.as_deref(), i64::MAX as u64) {
                Ok(account) => account,
                Err(_) => {
                    lookup_account(&transport, network, &init, metadata_allowed.clone()).await?
                }
            };
            let public = crate::admission::fetch_lighter_registered_key(
                &transport,
                network,
                account,
                index,
                metadata_allowed.clone(),
            )
            .await
            .map_err(|_| "Could not resolve account identity")?;
            if !metadata_allowed() {
                return Err("Account identity resolution expired".into());
            }
            init.application_id = Some(account.to_string());
            init.api_key_index = Some(index.to_string());
            init.lighter_registered_key = Some(identity::RegisteredLighterKey::new(
                captured_user,
                identity::scope(&init),
                account,
                index,
                &public,
                allowed,
            )?);
            Ok(init)
        }
    }
}

fn lookup_address(text: &str) -> Option<String> {
    let start = text.find("0x")?;
    let candidate = text.get(start..start + 42)?;
    (candidate[2..].bytes().all(|b| b.is_ascii_hexdigit())
        && text
            .as_bytes()
            .get(start + 42)
            .is_none_or(|b| !b.is_ascii_hexdigit()))
    .then(|| candidate.to_ascii_lowercase())
}

async fn lookup_account(
    transport: &NetworkTransport,
    network: Network,
    init: &InitPayload,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<u64, String> {
    let configured = init
        .application_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let address = match configured {
        Some(value) => lookup_address(value),
        None => lookup_address(&init.key),
    }
    .ok_or("Lighter account identity requires an account index or L1 address")?;
    for path in [
        format!("account?by=l1_address&value={address}"),
        format!("accountsByL1Address?l1_address={address}"),
    ] {
        let request = safe_cex_engine::HttpRequest {
            id: safe_cex_engine::RequestId(1),
            method: safe_cex_engine::Method::Get,
            url: format!("{}/api/v1/{path}", network.origin()),
            timeout_ms: Some(TIMEOUT_MS),
            route: None,
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        let body = crate::admission::read_public_metadata(transport, request, allowed.clone())
            .await
            .map_err(|_| "Could not resolve account identity")?;
        let value: Value =
            serde_json::from_slice(&body).map_err(|_| "Invalid account identity metadata")?;
        if value
            .get("code")
            .is_some_and(|code| code.as_u64() != Some(200))
            || value.get("success") == Some(&Value::Bool(false))
        {
            return Err("Invalid account identity metadata".into());
        }
        let rows = [
            "/sub_accounts",
            "/accounts",
            "/data/sub_accounts",
            "/data/accounts",
        ]
        .into_iter()
        .find_map(|path| value.pointer(path).and_then(Value::as_array))
        .ok_or("Invalid account identity metadata")?;
        if rows.is_empty() {
            continue;
        }
        if rows.len() != 1 {
            return Err("Lighter account identity requires an explicit account index".into());
        }
        let row = &rows[0];
        let mut account = None;
        for field in ["index", "account_index", "accountIndex"] {
            let Some(value) = row.get(field) else {
                continue;
            };
            let text = value
                .as_str()
                .map(str::to_owned)
                .or_else(|| value.as_u64().map(|n| n.to_string()))
                .ok_or("Invalid account identity metadata")?;
            let parsed = identity::decimal(Some(&text), i64::MAX as u64)?;
            if account.is_some_and(|prior| prior != parsed) {
                return Err("Invalid account identity metadata".into());
            }
            account = Some(parsed);
        }
        return account.ok_or_else(|| "Invalid account identity metadata".into());
    }
    Err("Could not resolve account identity".into())
}
