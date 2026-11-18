//! Cloudflare Access authentication for the release-proof endpoints.
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;
use tokio::time::Instant;

const KEY_TTL: Duration = Duration::from_secs(300);
const KEY_REFRESH_EVERY: Duration = Duration::from_secs(30);
const MAX_KEY_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct Config {
    issuer: String,
    audience: String,
}

impl Config {
    pub fn parse(domain: Option<&str>, audience: Option<&str>) -> Option<Self> {
        let domain = domain?.to_ascii_lowercase();
        let team = domain.strip_suffix(".cloudflareaccess.com")?;
        let audience = audience?;
        if team.is_empty()
            || team.len() > 63
            || team.starts_with('-')
            || team.ends_with('-')
            || !team.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || audience.is_empty()
            || audience.len() > 256
            || !audience.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return None;
        }
        Some(Self {
            issuer: format!("https://{domain}"),
            audience: audience.into(),
        })
    }
}

struct Keys {
    set: JwkSet,
    fetched: Option<Instant>,
    attempted: Option<Instant>,
}

pub struct Verifier {
    config: Option<Config>,
    client: Option<reqwest::Client>,
    keys: tokio::sync::Mutex<Keys>,
}

impl Verifier {
    pub fn new(config: Option<Config>) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(2))
            .build()
            .ok();
        if config.is_none() || client.is_none() {
            crate::state::Log::warn(
                "release proof disabled: Access team domain or AUD missing or invalid, or verifier unavailable",
            );
        }
        Self {
            config,
            client,
            keys: tokio::sync::Mutex::new(Keys {
                set: JwkSet { keys: Vec::new() },
                fetched: None,
                attempted: None,
            }),
        }
    }

    pub async fn verify(&self, token: &str) -> bool {
        self.verify_with(token, |client, issuer| async move {
            fetch_keys(&client, &issuer).await
        })
        .await
    }

    async fn verify_with<F, Fut>(&self, token: &str, fetch: F) -> bool
    where
        F: FnOnce(reqwest::Client, String) -> Fut,
        Fut: std::future::Future<Output = Option<JwkSet>>,
    {
        let (Some(config), Some(client)) = (&self.config, &self.client) else {
            return false;
        };
        if token.len() > 16 * 1024 {
            return false;
        }
        let Ok(header) = decode_header(token) else {
            return false;
        };
        if header.alg != Algorithm::RS256 {
            return false;
        }
        let Some(kid) = header.kid.filter(|kid| !kid.is_empty() && kid.len() <= 256) else {
            return false;
        };
        let mut keys = self.keys.lock().await;
        let stale = keys.fetched.is_none_or(|at| at.elapsed() >= KEY_TTL);
        let unknown = keys.set.find(&kid).is_none();
        if (stale || unknown)
            && keys
                .attempted
                .is_none_or(|at| at.elapsed() >= KEY_REFRESH_EVERY)
        {
            keys.attempted = Some(Instant::now());
            if let Some(set) = fetch(client.clone(), config.issuer.clone()).await {
                keys.set = set;
                keys.fetched = Some(Instant::now());
            }
        }
        if keys.fetched.is_none_or(|at| at.elapsed() >= KEY_TTL) {
            return false;
        }
        let Some(key) = keys
            .set
            .find(&kid)
            .and_then(|key| DecodingKey::from_jwk(key).ok())
        else {
            return false;
        };
        validate(token, &key, config)
    }
}

#[derive(Deserialize)]
struct Claims {
    exp: u64,
    iss: String,
    #[serde(default, deserialize_with = "deserialize_not_before")]
    nbf: Option<u64>,
}

fn deserialize_not_before<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Some)
}

fn validate(token: &str, key: &DecodingKey, config: &Config) -> bool {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.leeway = 0;
    validation.validate_nbf = true;
    validation.set_required_spec_claims(&["exp", "iss", "aud"]);
    validation.set_audience(&[&config.audience]);
    validation.set_issuer(&[&config.issuer]);
    decode::<Claims>(token, key, &validation)
        .ok()
        .is_some_and(|token| {
            let now = crate::accounts::now_ms() / 1000;
            token.claims.iss == config.issuer
                && token.claims.exp > now
                && token.claims.nbf.is_none_or(|not_before| not_before <= now)
        })
}

async fn fetch_keys(client: &reqwest::Client, issuer: &str) -> Option<JwkSet> {
    let mut response = client
        .get(format!("{issuer}/cdn-cgi/access/certs"))
        .send()
        .await
        .ok()?;
    if response.status() != reqwest::StatusCode::OK
        || response
            .content_length()
            .is_some_and(|n| n > MAX_KEY_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_KEY_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let keys: JwkSet = serde_json::from_slice(&bytes).ok()?;
    (!keys.keys.is_empty() && keys.keys.len() <= 64).then_some(keys)
}

/// At most forty immediately, then ten per second per peer; bounded source map.
pub struct Limiter(std::sync::Mutex<HashMap<IpAddr, (Instant, f64)>>, f64, f64);

impl Default for Limiter {
    fn default() -> Self {
        Self(Default::default(), 40., 10.)
    }
}

impl Limiter {
    /// Per-user DO alarms share the cloudflared peer: bounded separately from proofs.
    pub fn router_heartbeats() -> Self {
        Self(Default::default(), 1024., 512.)
    }
    pub fn allow(&self, peer: IpAddr) -> bool {
        let peer = peer.to_canonical();
        let mut sources = self.0.lock().unwrap();
        let now = Instant::now();
        sources.retain(|_, (last, _)| now.duration_since(*last) < Duration::from_secs(60));
        if !sources.contains_key(&peer) && sources.len() >= 1024 {
            return false;
        }
        let (at, tokens) = sources.entry(peer).or_insert((now, self.1));
        *tokens = (*tokens + now.duration_since(*at).as_secs_f64() * self.2).min(self.1);
        *at = now;
        if *tokens < 1. {
            return false;
        }
        *tokens -= 1.;
        true
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::{Value, json};

    // Public test-only key material, never trusted by the production verifier.
    pub(crate) fn fixture() -> (&'static EncodingKey, JwkSet) {
        static KEY: std::sync::OnceLock<EncodingKey> = std::sync::OnceLock::new();
        let key = KEY.get_or_init(|| {
            EncodingKey::from_rsa_der(include_bytes!(
                "../tests/fixtures/access-test-signing-key.der"
            ))
        });
        (
            key,
            serde_json::from_str(include_str!("../tests/fixtures/access-test-jwks.json")).unwrap(),
        )
    }

    pub(crate) fn token(patch: Value, kid: &str) -> String {
        let (key, _) = fixture();
        let now = crate::accounts::now_ms() / 1000;
        let mut claims =
            json!({"iss":"https://test-team.cloudflareaccess.com","aud":["abcdef"],"exp":now+3600});
        claims
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.into());
        encode(&header, &claims, key).unwrap()
    }

    pub(crate) async fn verifier() -> Verifier {
        let verifier = Verifier::new(Config::parse(
            Some("test-team.cloudflareaccess.com"),
            Some("abcdef"),
        ));
        let mut keys = verifier.keys.lock().await;
        keys.set = fixture().1;
        keys.fetched = Some(Instant::now());
        keys.attempted = Some(Instant::now());
        drop(keys);
        verifier
    }

    #[test]
    fn config_rejects_urls_paths_userinfo_ports_and_nonhex_audiences() {
        assert!(Config::parse(Some("test-team.cloudflareaccess.com"), Some("abcdef")).is_some());
        for domain in [
            "https://test-team.cloudflareaccess.com",
            "test-team.cloudflareaccess.com/path",
            "test-team.cloudflareaccess.com:443",
            "user@test-team.cloudflareaccess.com",
            "test-team.cloudflareaccess.com.evil",
            "test.team.cloudflareaccess.com",
            "-test.cloudflareaccess.com",
            "test_.cloudflareaccess.com",
            "test.cloudflareaccess.com/",
            "",
        ] {
            assert!(
                Config::parse(Some(domain), Some("abcdef")).is_none(),
                "{domain}"
            );
        }
        for aud in [None, Some(""), Some("not-hex"), Some("a b")] {
            assert!(Config::parse(Some("test-team.cloudflareaccess.com"), aud).is_none());
        }
        assert!(Config::parse(None, Some("abcdef")).is_none());
    }

    #[tokio::test]
    async fn verifies_signature_issuer_audience_and_expiry_without_accepting_proxy_tokens() {
        let verifier = verifier().await;
        assert!(verifier.verify(&token(json!({}), "test")).await);
        assert!(
            verifier
                .verify(&token(json!({"aud":["other","abcdef"]}), "test"))
                .await
        );
        for claims in [
            json!({"iss":"https://other.cloudflareaccess.com"}),
            json!({"aud":["different"]}),
            json!({"aud":null}),
            json!({"exp":1}),
            json!({"nbf":crate::accounts::now_ms()/1000+3600}),
        ] {
            assert!(
                !verifier.verify(&token(claims.clone(), "test")).await,
                "{claims}"
            );
        }
        let mut forged = token(json!({}), "test");
        let signature = forged.rfind('.').unwrap() + 1;
        forged.replace_range(
            signature..signature + 1,
            if &forged[signature..signature + 1] == "A" {
                "B"
            } else {
                "A"
            },
        );
        assert!(!verifier.verify(&forged).await);
        assert!(
            !verifier
                .verify(&crate::auth::unsigned_token(
                    "u",
                    crate::accounts::now_ms() / 1000 + 3600
                ))
                .await
        );
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("test".into());
        assert!(
            !verifier
                .verify(
                    &encode(
                        &header,
                        &json!({"exp":u64::MAX}),
                        &EncodingKey::from_secret(b"test")
                    )
                    .unwrap()
                )
                .await
        );
        assert!(!Verifier::new(None).verify(&token(json!({}), "test")).await);
    }

    #[tokio::test(start_paused = true)]
    async fn jwks_cache_is_bounded_refreshes_unknown_kid_and_fails_closed_after_expiry() {
        let verifier = Verifier::new(Config::parse(
            Some("test-team.cloudflareaccess.com"),
            Some("abcdef"),
        ));
        let token = token(json!({}), "test");
        let reads = std::cell::Cell::new(0);
        let fetch = |_, _| {
            reads.set(reads.get() + 1);
            std::future::ready(Some(fixture().1))
        };
        assert!(verifier.verify_with(&token, fetch).await);
        assert!(verifier.verify_with(&token, fetch).await);
        assert_eq!(reads.get(), 1);
        let unknown = self::token(json!({}), "rotated");
        assert!(!verifier.verify_with(&unknown, fetch).await);
        assert_eq!(reads.get(), 1);
        tokio::time::advance(KEY_REFRESH_EVERY).await;
        assert!(!verifier.verify_with(&unknown, fetch).await);
        assert_eq!(reads.get(), 2);
        tokio::time::advance(KEY_TTL).await;
        assert!(
            !verifier
                .verify_with(&token, |_, _| std::future::ready(None))
                .await
        );
        assert!(!verifier.verify_with(&token, fetch).await);
        tokio::time::advance(KEY_REFRESH_EVERY).await;
        assert!(verifier.verify_with(&token, fetch).await);
    }

    #[tokio::test(start_paused = true)]
    async fn rate_limit_is_per_peer_refills_and_bounds_source_memory() {
        let limiter = Limiter::default();
        let one: IpAddr = "127.0.0.1".parse().unwrap();
        for _ in 0..40 {
            assert!(limiter.allow(one));
        }
        assert!(!limiter.allow(one));
        assert!(limiter.allow("127.0.0.2".parse().unwrap()));
        tokio::time::advance(Duration::from_millis(100)).await;
        assert!(limiter.allow(one));
        assert!(!limiter.allow("::ffff:127.0.0.1".parse().unwrap()));
        for ip in 3..1025u32 {
            assert!(limiter.allow(IpAddr::V4(ip.into())));
        }
        assert!(!limiter.allow(IpAddr::V4(5000u32.into())));
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(limiter.allow(IpAddr::V4(5000u32.into())));
        assert_eq!(limiter.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn review_3080_malformed_not_before_is_refused() {
        let verifier = verifier().await;
        assert!(verifier.verify(&token(json!({}), "test")).await);
        assert!(
            !verifier
                .verify(&token(
                    json!({"nbf":crate::accounts::now_ms()/1000+3600}),
                    "test"
                ))
                .await
        );
        assert!(
            !verifier
                .verify(&token(json!({"nbf":"9999999999"}), "test"))
                .await,
            "signed non-numeric nbf must fail closed"
        );
    }

    #[tokio::test]
    async fn review_3080_issuer_must_be_the_exact_string() {
        let verifier = verifier().await;
        assert!(verifier.verify(&token(json!({}), "test")).await);
        assert!(
            !verifier
                .verify(&token(
                    json!({"iss":"https://other.cloudflareaccess.com"}),
                    "test"
                ))
                .await
        );
        let array_issuer = json!({"iss":[
            "https://test-team.cloudflareaccess.com",
            "https://other.cloudflareaccess.com"
        ]});
        assert!(
            !verifier.verify(&token(array_issuer, "test")).await,
            "array issuer must fail closed"
        );
    }

    #[tokio::test]
    async fn optional_not_before_accepts_only_an_elapsed_numeric_date() {
        let verifier = verifier().await;
        let now = crate::accounts::now_ms() / 1000;
        for claims in [json!({}), json!({"nbf":now}), json!({"nbf":now-1})] {
            assert!(verifier.verify(&token(claims, "test")).await);
        }
        for not_before in [
            json!(null),
            json!(false),
            json!(-1),
            json!(0.5),
            json!("0"),
            json!([]),
            json!({}),
        ] {
            assert!(
                !verifier
                    .verify(&token(json!({"nbf":not_before}), "test"))
                    .await,
                "malformed nbf must fail closed: {not_before}"
            );
        }
    }
}
