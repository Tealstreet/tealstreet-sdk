//! OKX signs the exact path/query and serialized body it sends.
use safe_cex_crypto::{Encoding, hmac_sha256};

pub struct Credentials {
    pub key: String,
    pub secret: String,
    pub passphrase: String,
}

impl Credentials {
    pub fn signature(&self, timestamp: &str, method: &str, path: &str, body: &str) -> String {
        hmac_sha256(
            self.secret.as_bytes(),
            format!("{timestamp}{method}{path}{body}").as_bytes(),
            Encoding::Base64,
        )
    }

    pub fn headers(
        &self,
        now: f64,
        method: &str,
        path: &str,
        body: &str,
    ) -> Vec<(&'static str, String)> {
        let timestamp = iso_timestamp(now);
        vec![
            ("OK-ACCESS-KEY", self.key.clone()),
            (
                "OK-ACCESS-SIGN",
                self.signature(&timestamp, method, path, body),
            ),
            ("OK-ACCESS-TIMESTAMP", timestamp),
            ("OK-ACCESS-PASSPHRASE", self.passphrase.clone()),
        ]
    }
}

// Gregorian civil date from Unix days (400-year eras); no clock or locale.
fn iso_timestamp(ms: f64) -> String {
    let ms = ms.floor() as i64;
    let days = ms.div_euclid(86_400_000);
    let within = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_date(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60,
        within % 1000
    )
}

pub(crate) fn civil_date(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signs_exact_get_query_post_body_and_ws_preimages() {
        let c = Credentials {
            key: "test-key".into(),
            secret: "test-secret".into(),
            passphrase: "test-passphrase".into(),
        };
        assert_eq!(
            c.signature(
                "1970-01-01T00:00:00.000Z",
                "GET",
                "/api/v5/account/config",
                ""
            ),
            "29klY97actzxrGqk1y/O9sExvXx4Rs9/13y6nq9tfi0="
        );
        assert_eq!(
            c.signature("1234567890", "GET", "/users/self/verify", ""),
            "qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ="
        );
        assert_eq!(
            c.signature(
                "1970-01-01T00:00:00.000Z",
                "POST",
                "/api/v5/trade/batch-orders",
                r#"[{"instId":"BTC-USDT-SWAP"}]"#
            ),
            "J5gNN8urKafdLakUupoqtac2yY2yQukwhRQPZYuj1ss="
        );
    }
    #[test]
    fn utc_milliseconds_and_leap_day() {
        assert_eq!(iso_timestamp(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            iso_timestamp(1_709_209_849_123.9),
            "2024-02-29T12:30:49.123Z"
        );
        assert_eq!(iso_timestamp(-1.0), "1969-12-31T23:59:59.999Z");
    }
}
