use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::ed25519;
use safe_cex_engine::url;
use std::collections::BTreeMap;

use safe_cex_engine::signing::{
    Algorithm, Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

/// Immutable native transport and canonical signing facts, without credentials.
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub rows: Vec<Params>,
    pub body: Option<String>,
    pub window: u32,
}

pub fn instruction(method: Method, path: &str) -> Result<&'static str, SignError> {
    match (method, path) {
        (Method::Get, "/api/v1/account") => Ok("accountQuery"),
        (Method::Patch, "/api/v1/account") => Ok("accountUpdate"),
        (Method::Get, "/api/v1/position") => Ok("positionQuery"),
        (Method::Get, "/api/v1/capital/collateral") => Ok("collateralQuery"),
        (Method::Get, "/api/v1/orders") => Ok("orderQueryAll"),
        (Method::Post, "/api/v1/order" | "/api/v1/orders") => Ok("orderExecute"),
        (Method::Delete, "/api/v1/order") => Ok("orderCancel"),
        (Method::Delete, "/api/v1/orders") => Ok("orderCancelAll"),
        (Method::Get, "/wapi/v1/history/fills") => Ok("fillHistoryQueryAll"),
        (Method::Get, "/wapi/v1/history/orders") => Ok("orderHistoryQueryAll"),
        (Method::Get, "/wapi/v1/history/funding") => Ok("fundingHistoryQueryAll"),
        _ => Err(SignError::InvalidRequest),
    }
}

/// The provider presents the public API key and signs exact Ed25519 bytes.
pub struct Http {
    identity: Identity,
    declaration: Request,
    timestamp: String,
}

impl Http {
    pub fn new(identity: Identity, declaration: Request) -> Self {
        Self {
            identity,
            declaration,
            timestamp: String::new(),
        }
    }
}

impl SignStep for Http {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let declaration = &self.declaration;
        let instruction = instruction(declaration.method, &declaration.path)?;
        let is_batch = declaration.method == Method::Post && declaration.path == "/api/v1/orders";
        if declaration.rows.is_empty()
            || (!is_batch && declaration.rows.len() != 1)
            || declaration.window == 0
            || http.method != declaration.method
            || http.body != declaration.body
        {
            return Err(SignError::InvalidRequest);
        }
        if declaration.method == Method::Get {
            if declaration.body.is_some() {
                return Err(SignError::InvalidRequest);
            }
        } else if json_rows(declaration.body.as_deref(), is_batch)? != declaration.rows {
            return Err(SignError::InvalidRequest);
        }
        let query = if declaration.method == Method::Get {
            query(&declaration.rows[0])
        } else {
            String::new()
        };
        let path = if query.is_empty() {
            declaration.path.clone()
        } else {
            format!("{}?{query}", declaration.path)
        };
        if http.url != format!("https://api.backpack.exchange{path}") {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        self.timestamp = safe_cex_engine::num::format(stamp);
        let text = payload(instruction, &declaration.rows, stamp, declaration.window);
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(stamp),
                expiry_ms: None,
            },
            context: Context {
                venue: "backpack".into(),
                method: http.method.as_str().into(),
                host: "api.backpack.exchange".into(),
                path,
            },
            operations: vec![
                Operation::Present {
                    key: self.identity.credential.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Ed25519,
                    key: self.identity.signing.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: text.into_bytes(),
                    }],
                },
            ],
        })
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if result.values.len() != 2 {
            return Err(SignError::InvalidResult);
        }
        let key = std::str::from_utf8(&result.values[0])
            .ok()
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        if result.values[1].len() != 64 {
            return Err(SignError::InvalidResult);
        }
        http.headers = vec![
            ("X-API-Key", key.into()),
            ("X-Broker-Id", "1200".into()),
            ("X-Signature", STANDARD.encode(&result.values[1])),
            ("X-Timestamp", self.timestamp.clone()),
            ("X-Window", self.declaration.window.to_string()),
            ("Content-Type", "application/json".into()),
        ];
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        // X-Window is the venue acceptance bound; queued material is refreshed
        // after five seconds even when that native bound is longer.
        f64::from(self.declaration.window.clamp(1, 5000))
    }
}

struct SigningRow(BTreeMap<String, Box<serde_json::value::RawValue>>);
impl<'de> serde::Deserialize<'de> for SigningRow {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = SigningRow;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an order object with unique fields")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut fields: M,
            ) -> Result<Self::Value, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = fields.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate order field"));
                    }
                }
                Ok(SigningRow(values))
            }
        }
        decoder.deserialize_map(Visitor)
    }
}

fn json_rows(body: Option<&str>, batch: bool) -> Result<Vec<Params>, SignError> {
    let body = body.ok_or(SignError::InvalidRequest)?;
    let rows: Vec<SigningRow> = if batch {
        serde_json::from_str(body).map_err(|_| SignError::InvalidRequest)?
    } else {
        vec![serde_json::from_str(body).map_err(|_| SignError::InvalidRequest)?]
    };
    rows.into_iter()
        .map(|row| {
            row.0
                .into_iter()
                .map(|(key, value)| {
                    let raw = value.get();
                    let value = if raw.starts_with('"') {
                        serde_json::from_str::<String>(raw)
                            .map_err(|_| SignError::InvalidRequest)?
                    } else if raw == "true" || raw == "false" || raw.parse::<u32>().is_ok() {
                        raw.into()
                    } else {
                        return Err(SignError::InvalidRequest);
                    };
                    Ok((key, value))
                })
                .collect()
        })
        .collect()
}

pub type Params = BTreeMap<String, String>;

pub fn query(params: &Params) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={}", url::encode_component(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn payload(instruction: &str, rows: &[Params], now: f64, window: u32) -> String {
    let now = safe_cex_engine::time::wire_stamp_ms(now);
    let mut text = rows
        .iter()
        .map(|row| {
            let q = query(row);
            format!(
                "instruction={instruction}{}",
                if q.is_empty() {
                    String::new()
                } else {
                    format!("&{q}")
                }
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    text.push_str(&format!("&timestamp={now:.0}&window={window}"));
    text
}

pub struct Credentials {
    pub key: String,
    seed: zeroize::Zeroizing<Vec<u8>>,
    pub window: u32,
}

impl Credentials {
    pub fn new(key: String, secret: &str) -> Result<Self, String> {
        let seed = zeroize::Zeroizing::new(
            STANDARD
                .decode(secret)
                .map_err(|_| "Invalid Backpack base64 seed")?,
        );
        if seed.len() != 32 {
            return Err("Ed25519 seed must be 32 bytes".into());
        }
        Ok(Self {
            key,
            seed,
            window: 60000,
        })
    }

    pub fn signature(&self, instruction: &str, rows: &[Params], now: f64) -> String {
        STANDARD.encode(
            ed25519::sign(
                payload(instruction, rows, now, self.window).as_bytes(),
                &self.seed,
            )
            .expect("validated seed"),
        )
    }

    pub fn ws_auth(&self, now: f64) -> [String; 4] {
        let now = safe_cex_engine::time::wire_stamp_ms(now);
        let bytes = payload("subscribe", &[Params::new()], now, 5000);
        [
            self.key.clone(),
            STANDARD.encode(ed25519::sign(bytes.as_bytes(), &self.seed).expect("validated seed")),
            format!("{now:.0}"),
            "5000".into(),
        ]
    }

    pub fn headers(
        &self,
        instruction: &str,
        rows: &[Params],
        now: f64,
    ) -> Vec<(&'static str, String)> {
        let now = safe_cex_engine::time::wire_stamp_ms(now);
        vec![
            ("X-API-Key", self.key.clone()),
            ("X-Broker-Id", "1200".into()),
            ("X-Signature", self.signature(instruction, rows, now)),
            ("X-Timestamp", format!("{now:.0}")),
            ("X-Window", self.window.to_string()),
            ("Content-Type", "application/json".into()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_protocol_time_is_floored_in_preimage_rest_and_ws() {
        let c = Credentials::new("public-test-key".into(), &STANDARD.encode([7u8; 32])).unwrap();
        let expected = "instruction=subscribe&timestamp=1000&window=5000";
        assert_eq!(
            payload("subscribe", &[Params::new()], 1000.875, 5000),
            expected
        );
        let auth = c.ws_auth(1000.875);
        assert_eq!(auth[2], "1000");
        assert_eq!(
            auth[1],
            STANDARD.encode(ed25519::sign(expected.as_bytes(), &c.seed).unwrap())
        );
        let headers = c.headers("subscribe", &[Params::new()], 1000.875);
        assert!(headers.contains(&("X-Timestamp", "1000".into())));
    }
    #[test]
    fn canonical_batch_matches_documented_signing_example() {
        let row = |price: &str, quantity: &str| {
            [
                ("symbol".into(), "SOL_USDC_PERP".into()),
                ("side".into(), "Bid".into()),
                ("orderType".into(), "Limit".into()),
                ("price".into(), price.into()),
                ("quantity".into(), quantity.into()),
            ]
            .into()
        };
        assert_eq!(
            payload(
                "orderExecute",
                &[row("141", "12"), row("140", "11")],
                1750793021519.0,
                5000
            ),
            "instruction=orderExecute&orderType=Limit&price=141&quantity=12&side=Bid&symbol=SOL_USDC_PERP&instruction=orderExecute&orderType=Limit&price=140&quantity=11&side=Bid&symbol=SOL_USDC_PERP&timestamp=1750793021519&window=5000"
        );
        assert_eq!(
            query(&[("text".into(), "a +/&é".into())].into()),
            "text=a%20%2B%2F%26%C3%A9"
        );
    }
    #[test]
    fn rest_and_ws_use_distinct_auth_windows() {
        let c = Credentials::new("public-test-key".into(), &STANDARD.encode([7u8; 32])).unwrap();
        assert!(
            c.headers("accountQuery", &[Params::new()], 1000.0)
                .iter()
                .any(|(k, v)| *k == "X-Window" && v == "60000")
        );
        assert_eq!(c.ws_auth(1000.0)[3], "5000");
        assert!(Credentials::new("key".into(), "invalid").is_err());
        assert!(Credentials::new("key".into(), &STANDARD.encode([7u8; 64])).is_err());
    }
}
