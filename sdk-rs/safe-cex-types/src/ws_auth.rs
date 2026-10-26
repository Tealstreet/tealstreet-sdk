//! Canonical remote-signer WS payloads; crypto re-exports these bottom-layer types.
//! Raw metadata preserves unknown values without introducing retained-field depth limits.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsAuth {
    pub to_sign: Option<String>,
    pub output_format: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_data: Option<Box<serde_json::value::RawValue>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsAuthRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub ws_auth: Option<WsAuth>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub request_type: Option<Box<serde_json::value::RawValue>>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsAuthResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_auth_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<'de> Deserialize<'de> for WsAuthRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RequestVisitor;
        impl<'de> serde::de::Visitor<'de> for RequestVisitor {
            type Value = WsAuthRequest;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a WS auth request object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut request = WsAuthRequest {
                    id: None,
                    ws_auth: None,
                    request_type: None,
                };
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => request.id = Some(map.next_value()?),
                        "type" => request.request_type = Some(map.next_value()?),
                        "wsAuth" => request.ws_auth = map.next_value_seed(WsAuthSeed)?,
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(request)
            }
        }
        deserializer.deserialize_map(RequestVisitor)
    }
}

struct WsAuthSeed;
impl<'de> serde::de::DeserializeSeed<'de> for WsAuthSeed {
    type Value = Option<WsAuth>;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        ws_auth_payload(deserializer)
    }
}

// Field-wise parsing skips unknown values instead of materializing the envelope.
// Ignored metadata may contain numbers or nesting that serde_json::Value rejects.
fn ws_auth_payload<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<WsAuth>, D::Error> {
    struct PayloadVisitor;
    impl<'de> serde::de::Visitor<'de> for PayloadVisitor {
        type Value = Option<WsAuth>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a WS auth payload or an ignored non-object value")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut auth = WsAuth {
                to_sign: None,
                output_format: None,
                extra_data: None,
            };
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    "toSign" => {
                        auth.to_sign = map.next_value::<Value>()?.as_str().map(str::to_owned)
                    }
                    "outputFormat" => auth.output_format = map.next_value()?,
                    "extraData" => auth.extra_data = Some(map.next_value()?),
                    _ => {
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
            }
            Ok(Some(auth))
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            while sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok(None)
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }
    }
    deserializer.deserialize_any(PayloadVisitor)
}

impl<'de> Deserialize<'de> for WsAuth {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ws_auth_payload(deserializer)?
            .ok_or_else(|| serde::de::Error::custom("a WS auth payload object is required"))
    }
}

fn present_json<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

impl<'de> Deserialize<'de> for WsAuthResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            #[serde(default, deserialize_with = "present_json")]
            id: Option<Value>,
            ws_auth_signature: Option<String>,
            error: Option<String>,
        }
        struct ResponseVisitor;
        impl<'de> serde::de::Visitor<'de> for ResponseVisitor {
            type Value = WsAuthResponse;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a WS auth response object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let wire = Wire::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(WsAuthResponse {
                    id: wire.id,
                    ws_auth_signature: wire.ws_auth_signature,
                    error: wire.error,
                })
            }
        }
        deserializer.deserialize_map(ResponseVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_bybit_envelope_preserves_metadata_and_signature_response() {
        let raw = r#"{"id":"bybit-1","type":"ws-auth","wsAuth":{"toSign":"GET/realtime1700000000000","outputFormat":"hex","extraData":{"expires":1700000000000}}}"#;
        let request: WsAuthRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(request.id, Some(Value::String("bybit-1".into())));
        assert_eq!(request.request_type.as_ref().unwrap().get(), r#""ws-auth""#);
        let auth = request.ws_auth.unwrap();
        assert_eq!(auth.to_sign.as_deref(), Some("GET/realtime1700000000000"));
        let metadata: Value =
            serde_json::from_str(auth.extra_data.as_ref().unwrap().get()).unwrap();
        assert_eq!(metadata["expires"], 1700000000000u64);
        let response: WsAuthResponse =
            serde_json::from_str(r#"{"id":"bybit-1","wsAuthSignature":"signature"}"#).unwrap();
        assert_eq!(response.ws_auth_signature.as_deref(), Some("signature"));
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            r#"{"id":"bybit-1","wsAuthSignature":"signature"}"#
        );
    }
    #[test]
    fn unknown_metadata_is_retained_without_changing_parser_limits() {
        for metadata in [
            "null".to_owned(),
            r#"{"expiresAt":1e400}"#.to_owned(),
            format!("{}0{}", "[".repeat(200), "]".repeat(200)),
        ] {
            let raw = format!(
                r#"{{"id":"metadata","wsAuth":{{"toSign":"challenge-token","outputFormat":"hex","extraData":{metadata}}}}}"#
            );
            let request: WsAuthRequest = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                request
                    .ws_auth
                    .as_ref()
                    .unwrap()
                    .extra_data
                    .as_ref()
                    .unwrap()
                    .get(),
                metadata
            );
            assert!(
                serde_json::to_string(&request)
                    .unwrap()
                    .contains(&format!(r#""extraData":{metadata}"#))
            );
        }
    }
    #[test]
    fn qualified_decoders_reject_positional_records() {
        for raw in ["[]", r#"["id",null]"#] {
            let mut parser = serde_json::Deserializer::from_str(raw);
            assert!(WsAuthRequest::deserialize(&mut parser).is_err());
            let mut parser = serde_json::Deserializer::from_str(raw);
            assert!(WsAuth::deserialize(&mut parser).is_err());
            let mut parser = serde_json::Deserializer::from_str(raw);
            assert!(WsAuthResponse::deserialize(&mut parser).is_err());
        }
    }
}
