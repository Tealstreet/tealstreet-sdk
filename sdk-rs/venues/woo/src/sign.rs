//! Route-specific signing over exactly the bytes sent to WOO.
pub mod dispatch;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::text::JoinWith;
pub use safe_cex_engine::url::encode;
use safe_cex_engine::{Method, num};

pub fn query(params: &[(String, String)], sorted: bool) -> String {
    let mut rows: Vec<_> = params.iter().collect();
    if sorted {
        rows.sort_by(|a, b| a.0.cmp(&b.0));
    }
    rows.into_iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join_with("&")
}
pub fn signature(secret: &str, text: &str) -> String {
    hmac_sha256(secret.as_bytes(), text.as_bytes(), Encoding::Hex)
}

pub struct Signed {
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<String>,
}
pub fn request(
    key: &str,
    secret: &str,
    now: f64,
    method: Method,
    path: &str,
    params: &[(String, String)],
    body: Option<String>,
) -> Signed {
    let (mut wire, text) = prepare(key, now, method, path, params, body);
    wire.headers[2].1 = signature(secret, &text);
    wire
}

pub(super) fn prepare(
    key: &str,
    now: f64,
    method: Method,
    path: &str,
    params: &[(String, String)],
    body: Option<String>,
) -> (Signed, String) {
    let timestamp = num::format(safe_cex_engine::time::wire_stamp_ms(now));
    let v1 = path.contains("/v1/");
    let serialized = query(params, v1);
    let mutation = method != Method::Get;
    let path = if !serialized.is_empty() && (!v1 || !mutation) {
        format!("{path}?{serialized}")
    } else {
        path.into()
    };
    let body = if v1 && mutation {
        Some(serialized.clone())
    } else {
        body
    };
    let text = if v1 {
        format!("{serialized}|{timestamp}")
    } else {
        format!(
            "{timestamp}{}{path}{}",
            method.as_str(),
            body.as_deref().unwrap_or("")
        )
    };
    (
        Signed {
            path,
            body,
            headers: vec![
                ("x-api-key", key.into()),
                ("x-api-timestamp", timestamp),
                ("x-api-signature", String::new()),
                (
                    "Content-Type",
                    if v1 && mutation {
                        "application/x-www-form-urlencoded"
                    } else {
                        "application/json"
                    }
                    .into(),
                ),
            ],
        },
        text,
    )
}

/// Legacy WS recipe; uses the same integer wire policy as REST.
pub fn websocket_auth(options: &crate::Options, now: f64) -> Option<String> {
    let secret = options.secret.as_deref()?;
    let timestamp = safe_cex_engine::time::wire_stamp_ms(now);
    let sign = signature(secret, &format!("|{}", num::format(timestamp)));
    Some(websocket_text(
        options.key.as_deref().unwrap_or(""),
        sign,
        timestamp,
    ))
}

pub(super) fn websocket_text(key: &str, sign: String, timestamp: f64) -> String {
    #[derive(serde::Serialize)]
    struct Auth<'a> {
        event: &'a str,
        params: AuthParams<'a>,
    }
    #[derive(serde::Serialize)]
    struct AuthParams<'a> {
        apikey: &'a str,
        sign: String,
        timestamp: f64,
    }
    serde_json::to_string(&Auth {
        event: "auth",
        params: AuthParams {
            apikey: key,
            sign,
            timestamp,
        },
    })
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorted_v1_form_and_get_sign_identical_parameter_bytes() {
        let p = vec![
            ("symbol".into(), "PERP_BTC_USDT".into()),
            ("note".into(), "x + /é".into()),
        ];
        let post = request(
            "replay-key",
            "replay-secret",
            1700000000000.0,
            Method::Post,
            "/v1/order",
            &p,
            None,
        );
        let get = request(
            "replay-key",
            "replay-secret",
            1700000000000.0,
            Method::Get,
            "/v1/orders",
            &p,
            None,
        );
        assert_eq!(
            post.body.as_deref(),
            Some("note=x%20%2B%20%2F%C3%A9&symbol=PERP_BTC_USDT")
        );
        assert_eq!(
            get.path,
            format!("/v1/orders?{}", post.body.as_ref().unwrap())
        );
        assert_eq!(post.headers[2], get.headers[2]);
        assert!(get.body.is_none());
    }
    #[test]
    fn v3_method_path_query_and_exact_body_all_participate() {
        let p = vec![
            ("symbol".into(), "PERP_BTC_USDT".into()),
            ("page".into(), "1".into()),
        ];
        let body = r#"{"quantity":"1","price":"10"}"#;
        let r = request(
            "replay-key",
            "replay-secret",
            1700000000000.0,
            Method::Put,
            "/v3/order/12",
            &p,
            Some(body.into()),
        );
        assert_eq!(r.path, "/v3/order/12?symbol=PERP_BTC_USDT&page=1");
        assert_eq!(r.body.as_deref(), Some(body));
        // Independent Python hashlib/hmac oracle, not a request reserialization.
        assert_eq!(
            r.headers[2].1,
            "d0fba14b11fe6593463dbc780649df636dd1a44c0b6eee31356542c7235d3240"
        );
    }
}
