//! Native twin of TS remoteSignerProvider.signApexOmni; one atomic device batch.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(untagged)]
enum Part<'a> {
    Bytes { bytes: &'a [u8] },
    Contract { contract: usize },
}

#[derive(Serialize)]
struct Descriptor<'a> {
    contracts: Vec<&'a [u8]>,
    payload: Vec<Part<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    credential: String,
    contracts: Vec<String>,
    signature: String,
}

impl Inner {
    pub(super) async fn sign_apexomni(
        &self,
        operations: &[Operation],
    ) -> Result<Vec<Vec<u8>>, SignError> {
        let primary = &self.primary;
        if let [Operation::Present { key }] = operations {
            return if *key == primary.credential {
                Ok(vec![primary.public.clone()])
            } else {
                refused()
            };
        }
        let count = operations.len().checked_sub(3).ok_or(SignError::Refused)?;
        if count > 29 {
            return refused();
        }
        let [
            Operation::Present { key: credential },
            Operation::Present { key: passphrase },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key,
                payload,
            },
        ] = &operations[count..]
        else {
            return refused();
        };
        if *credential != primary.credential
            || Some(passphrase) != primary.passphrase.as_ref()
            || !self.owns_signing(key)
            || payload.is_empty()
            || payload.len() > MAX_PARTS
        {
            return refused();
        }
        let passphrase = primary
            .passphrase_value
            .as_ref()
            .ok_or(SignError::Refused)?;
        let mut contracts = Vec::with_capacity(count);
        for operation in &operations[..count] {
            let Operation::Sign {
                algorithm: Algorithm::ZkLinkContract,
                key,
                payload,
            } = operation
            else {
                return refused();
            };
            let bytes = single_bytes(payload, 36)?;
            if Some(key) != primary.zklink_seed.as_ref() || bytes.len() != 36 || bytes[0] != 0xfe {
                return refused();
            }
            contracts.push(bytes);
        }
        let mut seen = vec![false; count];
        let mut parts = Vec::with_capacity(payload.len());
        for part in payload {
            parts.push(match part {
                PayloadPart::Bytes { bytes } if bytes.len() <= MAX_PAYLOAD => Part::Bytes { bytes },
                PayloadPart::Result {
                    index,
                    encoding: PartEncoding::Hex,
                } if seen.get(*index) == Some(&false) => {
                    seen[*index] = true;
                    Part::Contract { contract: *index }
                }
                _ => return refused(),
            });
        }
        if seen.contains(&false) {
            return refused();
        }
        let descriptor = serde_json::to_string(&Descriptor {
            contracts,
            payload: parts,
        })
        .map_err(|_| SignError::Refused)?;
        if descriptor.len() > MAX_PAYLOAD {
            return refused();
        }
        self.device(descriptor, "apexomni-batch", |value| {
            let text = value
                .as_str()
                .filter(|text| text.len() <= MAX_PAYLOAD)
                .ok_or(SignError::Refused)?;
            let answer: Answer = serde_json::from_str(text).map_err(|_| SignError::Refused)?;
            if answer.credential.as_bytes() != primary.public || answer.contracts.len() != count {
                return refused();
            }
            let mut values = Vec::with_capacity(count + 3);
            for signature in answer.contracts {
                if signature.len() != 128
                    || !signature
                        .bytes()
                        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
                {
                    return refused();
                }
                values.push(hex_bytes(&signature).ok_or(SignError::Refused)?);
            }
            values.push(primary.public.clone());
            values.push(passphrase.clone());
            values.push(mac(&Value::String(answer.signature), Encoding::Base64)?);
            Ok(values)
        })
        .await
    }
}
