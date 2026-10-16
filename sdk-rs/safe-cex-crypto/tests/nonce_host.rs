//! Host-owned entropy contract; no RNG is added to production crypto.
use safe_cex_crypto::lighter::{SchnorrNonce, public_key, schnorr_sign, schnorr_verify};
use zeroize::Zeroize;

fn sample(
    mut fill: impl FnMut(&mut [u8; 40]) -> Result<(), String>,
) -> Result<SchnorrNonce, String> {
    for _ in 0..64 {
        let mut bytes = [0; 40];
        let entropy = fill(&mut bytes);
        if let Err(error) = entropy {
            bytes.zeroize();
            return Err(error);
        }
        let candidate = SchnorrNonce::from_bytes(&bytes);
        bytes.zeroize();
        if let Ok(nonce) = candidate {
            return Ok(nonce);
        }
    }
    Err("CSPRNG produced no valid nonce in 64 fresh draws".into())
}

#[test]
fn real_csprng_requires_resampling_and_looping_host_signs_every_request() {
    let output = std::process::Command::new("node")
        .args(["-e", "const c=require('node:crypto');process.stdout.write(JSON.stringify(Array.from({length:8192},()=>Array.from(c.randomBytes(40)))))"])
        .output().expect("Node CSPRNG required for host contract");
    assert!(output.status.success());
    // Raw text parser is the entry point; no json!/from_value construction.
    let samples: Vec<Vec<u8>> =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
    let rejected = samples
        .iter()
        .filter(|bytes| SchnorrNonce::from_bytes(bytes).is_err())
        .count();
    let fraction = rejected as f64 / samples.len() as f64;
    eprintln!(
        "uniform 40-byte candidate rejection: {rejected}/{} = {fraction:.4}",
        samples.len()
    );
    assert!(
        (0.45..0.55).contains(&fraction),
        "one-shot host fails approximately half its requests"
    );
    let mut draws = samples.into_iter();
    let mut key = [0; 40];
    key[0] = 1; // Public offline test key; never a production secret.
    let public = public_key(&key).unwrap();
    for _ in 0..128 {
        let nonce = sample(|buffer| {
            *buffer = draws
                .next()
                .ok_or("sample pool exhausted")?
                .try_into()
                .map_err(|_| "sample must be 40 bytes")?;
            Ok(())
        })
        .expect("looping host must satisfy every signing request");
        let signature = schnorr_sign(&[], &key, nonce).unwrap();
        assert!(schnorr_verify(&[], &public, &signature).unwrap());
    }
}

#[test]
fn entropy_errors_stop_and_broken_provider_is_bounded() {
    let mut calls = 0;
    assert!(
        sample(|_| {
            calls += 1;
            Err("entropy failed".into())
        })
        .is_err()
    );
    assert_eq!(calls, 1, "entropy failures must not be retried");
    calls = 0;
    assert!(
        sample(|bytes| {
            calls += 1;
            bytes.fill(0);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls, 64, "broken zero entropy must terminate");
}

#[test]
fn worked_contract_is_the_executed_host_loop() {
    let source = include_str!("nonce_host.rs");
    let start = source.find("fn sample(").unwrap();
    let end = source[start..].find("\n\n#[test]").unwrap() + start;
    assert!(
        include_str!("../CLAUDE.md").contains(&source[start..end]),
        "worked host contract must be the executed sampling loop"
    );
}
