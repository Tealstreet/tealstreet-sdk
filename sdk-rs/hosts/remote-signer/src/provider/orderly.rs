use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    credential: String,
    signature: String,
}

impl Inner {
    pub(super) async fn sign_orderly(
        &self,
        request: &SignRequest,
    ) -> Result<SignResult, SignError> {
        let [
            Operation::Present { key: credential },
            Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: signing,
                payload,
            },
        ] = request.operations.as_slice()
        else {
            return refused();
        };
        if credential != &self.primary.credential
            || signing != &self.primary.signing
            || (request.context.venue != "orderly" && request.context.venue != self.venue)
            || request.receipt.validate().is_err()
            || request.receipt.nonce.is_some()
            || request.receipt.expiry_ms.is_some()
            || !self.account_name.as_deref().is_some_and(|name| {
                !name.is_empty() && name.len() <= 1024 && !name.chars().any(char::is_control)
            })
        {
            return refused();
        }
        let stamp = request.receipt.stamp_ms.ok_or(SignError::Refused)?;
        if stamp.is_sign_negative() {
            return refused();
        }
        let timestamp = format!("{stamp:.0}");
        let payload = utf8(single_bytes(payload, MAX_PAYLOAD)?)?;
        let ws = request.context.method == "WS";
        let canonical = if ws {
            payload == timestamp && request.context.path == "auth"
        } else {
            let rest = payload.strip_prefix(&timestamp).ok_or(SignError::Refused)?;
            match request.context.method.as_str() {
                "HOSTED" => ["GET/", "POST/", "PUT/", "DELETE/"]
                    .iter()
                    .any(|method| rest.starts_with(method)),
                "GET" | "POST" | "PUT" | "DELETE" => {
                    request.context.path.starts_with('/')
                        && rest.starts_with(&format!(
                            "{}{}",
                            request.context.method, request.context.path
                        ))
                }
                _ => false,
            }
        };
        let public = utf8(&self.primary.public)?;
        let key = public.strip_prefix("ed25519:").ok_or(SignError::Refused)?;
        if !canonical
            || !(32..=44).contains(&key.len())
            || bs58::decode(key)
                .into_vec()
                .map_err(|_| SignError::Refused)?
                .len()
                != 32
        {
            return refused();
        }
        let descriptor = json!({ "version":"orderly/1", "venue":self.venue, "slot":"primary",
            "kind":if ws { "wsAuth" } else { "http" }, "stampMs":stamp, "payload":payload });
        let descriptor = descriptor.to_string();
        if descriptor.len() > MAX_PAYLOAD {
            return refused();
        }
        let mut random = [0u8; 21];
        getrandom::getrandom(&mut random).map_err(|_| SignError::Unavailable)?;
        let id = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let values = self
            .ask(
                id,
                &self.venue,
                descriptor,
                "orderly-ed25519",
                None,
                |value| {
                    let encoded = value
                        .as_str()
                        .filter(|text| text.len() <= 512 && text.trim_start().starts_with('{'))
                        .ok_or(SignError::Refused)?;
                    let reply: Reply =
                        serde_json::from_str(encoded).map_err(|_| SignError::Refused)?;
                    if reply.credential != public {
                        return refused();
                    }
                    let encoding = base64::engine::general_purpose::URL_SAFE;
                    let signature = encoding
                        .decode(&reply.signature)
                        .map_err(|_| SignError::Refused)?;
                    if signature.len() != 64 || encoding.encode(&signature) != reply.signature {
                        return refused();
                    }
                    Ok(vec![reply.credential.into_bytes(), signature])
                },
            )
            .await?;
        let result = SignResult::new(values, request.receipt);
        request.validate_result(&result)?;
        Ok(result)
    }
}
