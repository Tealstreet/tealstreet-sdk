use crate::{
    TransportError,
    routing::{HowEgress, tunnel_endpoint},
};
use url::Url;

pub type TunnelEgress = HowEgress;

pub fn tunnel_egress_url(egress: HowEgress) -> &'static str {
    tunnel_endpoint(egress)
}

pub fn build_relay_url(egress: HowEgress, upstream: &str) -> String {
    let mut url = Url::parse(tunnel_egress_url(egress)).expect("generated endpoint is a URL");
    url.query_pairs_mut().append_pair("ts-relay", upstream);
    url.into()
}

pub fn is_relay_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| url.query_pairs().any(|(key, _)| key == "ts-relay"))
}

pub fn extract_domain_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .map(|url| url.host_str().unwrap_or("").to_owned())
}

pub(crate) fn parse_relay_url(url: &str) -> Result<(String, String), TransportError> {
    let mut parsed = Url::parse(url).map_err(|_| {
        TransportError::JavaScript(safe_cex_num::JsCoercionError {
            name: "TypeError",
            message: "Invalid URL",
        })
    })?;
    let upstream = parsed
        .query_pairs()
        .find(|(key, _)| key == "ts-relay")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| TransportError::Host(format!("not a relay url: {url}")))?;
    let remaining: Vec<_> = parsed
        .query_pairs()
        .filter(|(key, _)| key != "ts-relay")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    parsed.set_query(None);
    if !remaining.is_empty() {
        parsed.query_pairs_mut().extend_pairs(remaining);
    }
    Ok((parsed.into(), upstream))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relay_urls_roundtrip_query_encoding_and_require_a_nonempty_upstream() {
        let upstream = "wss://venue.test:443/private?tag=a b&token=x+y#part";
        let url = build_relay_url(HowEgress::Jp, upstream);
        assert!(is_relay_url(&url));
        assert!(url.contains("a+b"));
        assert_eq!(
            parse_relay_url(&url).unwrap(),
            (tunnel_egress_url(HowEgress::Jp).into(), upstream.into())
        );
        assert!(is_relay_url("wss://tunnel.test?ts-relay="));
        assert!(parse_relay_url("wss://tunnel.test?ts-relay=").is_err());
        assert!(!is_relay_url("not a url?ts-relay=x"));
        assert!(parse_relay_url("wss://tunnel.test?ts-relay=&ts-relay=valid").is_err());
        assert_eq!(
            parse_relay_url(
                "wss://tunnel.test/a?v=2&ts-relay=wss%3A%2F%2Fvenue&ts-relay=ignored#frag"
            )
            .unwrap(),
            ("wss://tunnel.test/a?v=2#frag".into(), "wss://venue".into())
        );
    }
    #[test]
    fn domains_follow_url_hostname_normalization() {
        assert_eq!(
            extract_domain_from_url("https://EXAMPLE.COM:443/a"),
            Some("example.com".into())
        );
        assert_eq!(
            extract_domain_from_url("https://[::1]/a"),
            Some("[::1]".into())
        );
        assert_eq!(extract_domain_from_url("file:///tmp/x"), Some("".into()));
        assert_eq!(extract_domain_from_url("/relative"), None);
    }
}
