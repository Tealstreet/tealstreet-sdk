use super::*;
use crate::admission::LocalProof;
use safe_cex_crypto::admission::{
    PublicKey,
    identity::Venue,
    wire::{Request, Response},
};

pub(super) async fn verify(
    user: &str,
    init: &InitPayload,
    candidate: Option<&Credentials>,
    link: Option<&Arc<SignerLink>>,
    socket: u64,
    projection: &Projection,
    permit: &ProbePermit,
) -> Result<(), ProbeRefusal> {
    permit.check(user, init)?;
    let scope: [String; 3] =
        serde_json::from_str(projection.scope()).map_err(|_| ProbeRefusal::INVALID)?;
    let namespace = if init.exchange_name == "hyperliquidspot" {
        "hyperliquid"
    } else {
        &init.exchange_name
    };
    let network = if init.testnet == Some(true) {
        "testnet"
    } else {
        "mainnet"
    };
    if scope != [namespace, network, ""].map(str::to_owned) {
        return Err(ProbeRefusal::INVALID);
    }
    let mut entropy = [0; 16];
    getrandom::getrandom(&mut entropy).map_err(|_| ProbeRefusal::TRANSIENT)?;
    let attempt = format!("join-{}", safe_cex_crypto::hex(&entropy));
    let expected = projection.public_key();
    let family = expected.family();
    let public = PublicKey::new(family, expected.as_bytes()).map_err(|_| ProbeRefusal::INVALID)?;
    let mut proof = LocalProof::issue(socket, &attempt, projection.digest(), public, now_ms())
        .map_err(|_| ProbeRefusal::TRANSIENT)?;
    let verified = match (candidate, link) {
        (None, Some(link)) if init.signing_mode.as_deref() != Some("delegated") => {
            let venue: Venue = serde_json::from_value(init.exchange_name.clone().into())
                .map_err(|_| ProbeRefusal::INVALID)?;
            let request = Request::new(
                &attempt,
                venue,
                &init.account_name,
                init.instance_id.as_deref().ok_or(ProbeRefusal::INVALID)?,
                proof.statement().clone(),
            )
            .map_err(|_| ProbeRefusal::INVALID)?;
            let value = serde_json::to_value(request).map_err(|_| ProbeRefusal::INVALID)?;
            permit.check(user, init)?;
            if !link.is_available() || link.socket() != socket {
                return Err(ProbeRefusal::TRANSIENT);
            }
            let (received_socket, raw) = link
                .join_proof(&attempt, &value)
                .await
                .map_err(|_| ProbeRefusal::TRANSIENT)?;
            permit.check(user, init)?;
            if !link.is_available() || link.socket() != socket {
                return Err(ProbeRefusal::TRANSIENT);
            }
            let response = Response::from_json(&raw, family).map_err(|_| ProbeRefusal::INVALID)?;
            if response.id() != attempt {
                return Err(ProbeRefusal::INVALID);
            }
            if let Response::Refused(refusal) = &response {
                // Classify only the closed decoder's fixed refusal projection.
                let refusal = serde_json::to_value(refusal).map_err(|_| ProbeRefusal::INVALID)?;
                return Err(if refusal["error"] == "Join proof unavailable" {
                    ProbeRefusal::UNAVAILABLE
                } else {
                    ProbeRefusal::INVALID
                });
            }
            let signature = response.signature().ok_or(ProbeRefusal::INVALID)?;
            proof.verify(received_socket, now_ms(), &signature)
        }
        (Some(candidate), None) if init.signing_mode.as_deref() == Some("delegated") => {
            let prepared = prepare_delegated(init, candidate.pair())
                .map_err(|_| ProbeRefusal::INVALID.at(RefusalStage::SignerSetup))?;
            permit.check(user, init)?;
            let reference = prepared
                .resident
                .identities()
                .primary
                .as_ref()
                .ok_or(ProbeRefusal::INVALID)?
                .signing_ref()
                .map_err(|_| ProbeRefusal::INVALID)?;
            proof
                .verify_candidate(socket, now_ms(), &prepared.resident.provider(), reference)
                .map_err(|_| ProbeRefusal::INVALID)?
        }
        _ => return Err(ProbeRefusal::INVALID),
    };
    permit.check(user, init)?;
    if verified {
        Ok(())
    } else {
        Err(ProbeRefusal::INVALID)
    }
}
