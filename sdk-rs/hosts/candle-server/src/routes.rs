//! Public routing: `/candles/<route>` and bare `<route>`, the CORS policy and
//! Cache-Control, from NX `workers/data-candles/src/{routing,index}.ts` and
//! `candle-server/src/server.ts`.

use crate::json::{Json, Object};
use crate::{Local, Reply, handlers};
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderValue, Method, Response, StatusCode, Uri, header},
};

pub const CANDLES_PREFIX: &str = "/candles";
const DEFAULT_CANDLE_CACHE_TTL_SECONDS: u64 = 300;

/// The Worker's closed-range interval table (`routing.ts` `INTERVAL_SECONDS`).
const CACHEABLE_INTERVAL_SECONDS: [(&str, f64); 17] = [
    ("1s", 1.),
    ("5s", 5.),
    ("15s", 15.),
    ("30s", 30.),
    ("1m", 60.),
    ("3m", 180.),
    ("5m", 300.),
    ("15m", 900.),
    ("30m", 1800.),
    ("1h", 3600.),
    ("2h", 7200.),
    ("4h", 14400.),
    ("6h", 21600.),
    ("8h", 28800.),
    ("12h", 43200.),
    ("1d", 86400.),
    ("1w", 604800.),
];

/// `URLSearchParams` over the raw query: first value wins, `+` is a space.
pub struct Query(Vec<(String, String)>);

impl Query {
    pub fn parse(query: Option<&str>) -> Self {
        Self(
            form_urlencoded::parse(query.unwrap_or("").as_bytes())
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect(),
        )
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn without(self, name: &str) -> Self {
        Self(self.0.into_iter().filter(|(key, _)| key != name).collect())
    }
}

pub fn router(local: Local) -> Router {
    Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap| {
        let local = local.clone();
        async move { route(local, method, uri, headers).await }
    })
}

/// The bare route under the public `/candles` prefix, or `None` for `/candles` itself.
pub fn strip_prefix(path: &str) -> Option<&str> {
    if path == CANDLES_PREFIX {
        return None;
    }
    match path.strip_prefix(CANDLES_PREFIX) {
        Some(rest) if rest.starts_with('/') => Some(rest),
        _ => Some(path),
    }
}

async fn route(local: Local, method: Method, uri: Uri, headers: HeaderMap) -> Response<Body> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if method == Method::OPTIONS {
        return respond(&origin, 204, None, None);
    }
    let Some(path) = strip_prefix(uri.path()) else {
        let location = match uri.query() {
            Some(query) => format!("{CANDLES_PREFIX}/?{query}"),
            None => format!("{CANDLES_PREFIX}/"),
        };
        let mut response = respond(&origin, 308, None, None);
        response.headers_mut().insert(
            header::LOCATION,
            HeaderValue::from_str(&location).expect("an ASCII location"),
        );
        return response;
    };
    // `verbose` is internal-only; a public caller can never set it.
    let query = Query::parse(uri.query()).without("verbose");
    let get = method == Method::GET;
    let reply = match path {
        "/health" | "/healthz" => {
            local
                .run(Box::new(|app| {
                    Box::pin(async move { handlers::health(&app).await })
                }))
                .await
        }
        "/api/market-data/symbols" if get => {
            local
                .run(Box::new(move |app| {
                    Box::pin(async move { handlers::symbols(&app, &query).await })
                }))
                .await
        }
        "/api/market-data/markets" if get => {
            local
                .run(Box::new(move |app| {
                    Box::pin(async move { handlers::markets(&app, &query).await })
                }))
                .await
        }
        "/api/market-data/market" if get => {
            local
                .run(Box::new(move |app| {
                    Box::pin(async move { handlers::market(&app, &query).await })
                }))
                .await
        }
        "/api/market-data/candles" if get => {
            let ttl = closed_range_ttl(&query, crate::pool::now_ms());
            let mut reply = local
                .run(Box::new(move |app| {
                    Box::pin(async move { handlers::candles(&app, &query).await })
                }))
                .await;
            if reply.status == 200 {
                reply.cache_ttl = ttl;
            }
            reply
        }
        _ => not_found(),
    };
    respond(&origin, reply.status, Some(reply.body), reply.cache_ttl)
}

pub fn not_found() -> Reply {
    Reply::json(
        404,
        Object::new().field("error", Json::str("not found")).build(),
    )
}

pub fn internal_error() -> Reply {
    Reply::json(
        500,
        Object::new()
            .field("error", Json::str("Internal error"))
            .field("code", Json::str("internal-error"))
            .build(),
    )
}

/// `getClosedRangeCacheTtl`: only a range ending before the current candle is final.
pub fn closed_range_ttl(query: &Query, now_ms: f64) -> Option<u64> {
    let interval = query.get("interval").unwrap_or("");
    let seconds = CACHEABLE_INTERVAL_SECONDS
        .iter()
        .find(|(name, _)| *name == interval)?
        .1;
    let interval_ms = seconds * 1000.;
    let to_ms = js_number(query.get("to").unwrap_or(""));
    if !to_ms.is_finite() || to_ms >= (now_ms / interval_ms).floor() * interval_ms {
        return None;
    }
    Some(parse_positive_int(
        std::env::var("CANDLE_CACHE_TTL_SECONDS").ok().as_deref(),
        DEFAULT_CANDLE_CACHE_TTL_SECONDS,
    ))
}

/// `Number.parseInt(value, 10)` then the Worker's positive-integer fallback.
fn parse_positive_int(value: Option<&str>, fallback: u64) -> u64 {
    let text = value.unwrap_or("").trim_start();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    match digits.parse::<u64>() {
        Ok(parsed) if sign > 0 && parsed >= 1 => parsed,
        _ => fallback,
    }
}

/// JavaScript `Number(text)`: trimmed decimal, `0x`/`0o`/`0b` integers and
/// `Infinity`; the empty string is 0 and anything else is NaN.
pub fn js_number(text: &str) -> f64 {
    let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if text.is_empty() {
        return 0.;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return u128::from_str_radix(digits, radix).map_or(f64::NAN, |value| value as f64);
        }
    }
    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let decimal = text
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
        && text.chars().any(|c| c.is_ascii_digit());
    if !decimal {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

/// `getCorsHeaders`: `CANDLE_SERVER_CORS_ORIGINS`, read per request.
fn cors_origin(origin: &str) -> String {
    let configured = std::env::var("CANDLE_SERVER_CORS_ORIGINS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "*".into());
    let allowed: Vec<&str> = configured
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();
    if allowed.contains(&"*") || origin.is_empty() {
        "*".into()
    } else if allowed.contains(&origin) {
        origin.into()
    } else {
        allowed.first().copied().unwrap_or("*").into()
    }
}

fn respond(
    origin: &str,
    status: u16,
    body: Option<String>,
    cache_ttl: Option<u64>,
) -> Response<Body> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status).expect("a valid status"))
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors_origin(origin))
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "content-type")
        .header(header::VARY, "Origin")
        .header(
            header::CACHE_CONTROL,
            match cache_ttl {
                Some(ttl) => format!("public, max-age={ttl}"),
                None => "no-store".into(),
            },
        );
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json;charset=utf-8");
    }
    builder
        .body(body.map_or_else(Body::empty, Body::from))
        .expect("a valid response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_prefixed_and_bare_routes_resolve() {
        assert_eq!(strip_prefix("/candles/health"), Some("/health"));
        assert_eq!(
            strip_prefix("/candles/api/market-data/candles"),
            Some("/api/market-data/candles")
        );
        assert_eq!(
            strip_prefix("/api/market-data/candles"),
            Some("/api/market-data/candles")
        );
        assert_eq!(strip_prefix("/candles/"), Some("/"));
        assert_eq!(strip_prefix("/candles"), None);
        assert_eq!(strip_prefix("/candlesx"), Some("/candlesx"));
    }

    #[test]
    fn verbose_is_never_forwarded() {
        let query = Query::parse(Some("verbose=1&exchange=bybit")).without("verbose");
        assert_eq!(query.get("verbose"), None);
        assert_eq!(query.get("exchange"), Some("bybit"));
    }

    #[test]
    fn only_closed_ranges_are_publicly_cacheable() {
        let now = 1_736_899_230_000.; // 30 s into a minute
        let query =
            |interval: &str, to: f64| Query::parse(Some(&format!("interval={interval}&to={to}")));
        assert_eq!(
            closed_range_ttl(&query("1m", now - 31_000.), now),
            Some(300)
        );
        // The forming candle (and anything after its start) is never cached.
        assert_eq!(closed_range_ttl(&query("1m", now - 30_000.), now), None);
        assert_eq!(closed_range_ttl(&query("1m", now), now), None);
        // Intervals outside the Worker table stay uncached.
        assert_eq!(closed_range_ttl(&query("3h", 0.), now), None);
        assert_eq!(
            closed_range_ttl(&Query::parse(Some("interval=1m&to=abc")), now),
            None
        );
        assert_eq!(parse_positive_int(Some("120s"), 300), 120);
        assert_eq!(parse_positive_int(Some("0"), 300), 300);
        assert_eq!(parse_positive_int(None, 300), 300);
    }

    #[test]
    fn numbers_parse_like_javascript() {
        assert_eq!(js_number(" 12 "), 12.);
        assert_eq!(js_number(""), 0.);
        assert_eq!(js_number("0x10"), 16.);
        assert_eq!(js_number("1e3"), 1000.);
        assert!(js_number("inf").is_nan());
        assert!(js_number("12abc").is_nan());
        assert_eq!(js_number("Infinity"), f64::INFINITY);
    }

    #[test]
    fn cors_echoes_only_an_allowed_origin() {
        // SAFETY: tests in this module are the only readers of this variable.
        unsafe {
            std::env::set_var(
                "CANDLE_SERVER_CORS_ORIGINS",
                "https://a.test, https://b.test",
            )
        };
        assert_eq!(cors_origin("https://b.test"), "https://b.test");
        assert_eq!(cors_origin("https://evil.test"), "https://a.test");
        assert_eq!(cors_origin(""), "*");
        unsafe { std::env::remove_var("CANDLE_SERVER_CORS_ORIGINS") };
        assert_eq!(cors_origin("https://evil.test"), "*");
    }
}
