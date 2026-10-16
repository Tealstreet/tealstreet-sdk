//! @tealstreet/poseidon-crypto over the native safe-cex-crypto Lighter arithmetic.
//! No WASM loads: initWasm settles immediately and marks the module ready.
use super::Host;
use safe_cex_crypto::lighter::{
    SchnorrNonce, poseidon2_hash, poseidon2_hash_fp5, poseidon2_two_to_one, public_key,
    schnorr_sign, schnorr_verify,
};
use safe_cex_exchange::js_runtime::{
    promise::Promise,
    value::{Js, Thrown},
};
use std::{cell::Cell, rc::Rc};

/// Fills one 40-byte Schnorr nonce candidate; the host's CSPRNG in production.
pub(crate) type Entropy = Rc<dyn Fn(&mut [u8; 40]) -> Result<(), String>>;

/// The host CSPRNG: OS entropy natively, `crypto.getRandomValues` on wasm32,
/// which is the source the Go WASM signer reads.
pub(super) fn os_entropy() -> Entropy {
    Rc::new(|bytes| getrandom::getrandom(bytes).map_err(|e| e.to_string()))
}

const DRAWS: usize = 64;

/// Fresh draws until one is a canonical nonzero scalar; entropy errors stop.
fn sample(entropy: &Entropy) -> Result<SchnorrNonce, String> {
    for _ in 0..DRAWS {
        let mut bytes = [0u8; 40];
        let drawn = entropy(&mut bytes);
        let candidate = drawn.map(|()| SchnorrNonce::from_bytes(&bytes).ok());
        bytes.fill(0);
        if let Some(nonce) = candidate? {
            return Ok(nonce);
        }
    }
    Err(format!(
        "CSPRNG produced no valid nonce in {DRAWS} fresh draws"
    ))
}

pub(super) fn install(host: &Host, entropy: Entropy) -> Result<(), Thrown> {
    let ready = Rc::new(Cell::new(false));
    let init = ready.clone();
    let runtime = host.runtime.clone();
    // ts: initWasm
    host.define_global(
        "initWasm",
        Js::function("initWasm", move |_, _| {
            init.set(true);
            Ok(Js::Promise(Promise::settled(&runtime, Ok(Js::Undefined))))
        }),
    )?;
    let probe = ready.clone();
    // ts: isWasmReady
    host.define_global(
        "isWasmReady",
        Js::function("isWasmReady", move |_, _| Ok(Js::Bool(probe.get()))),
    )?;
    // ts: getCryptoApi
    host.define_global(
        "getCryptoApi",
        Js::function("getCryptoApi", move |_, _| {
            if !ready.get() {
                return Err(Thrown::error(
                    "Error",
                    "WASM not initialized. Call initWasm() first.",
                ));
            }
            Ok(crypto_api(&entropy))
        }),
    )
}

/// `{ error }`, the Go `handleError` object.
fn wasm_error(message: &str) -> Js {
    Js::object(vec![("error", Js::str(message))])
}

/// Go's `js.Value.String()`: strings convert, other types become `<type...>`
/// tags, so hex decoding always reports their leading `<`.
fn go_string(value: &Js) -> String {
    match value {
        Js::String(units) => String::from_utf16_lossy(units),
        Js::Undefined => "<undefined>".into(),
        Js::Null => "<null>".into(),
        Js::Bool(b) => format!("<boolean: {b}>"),
        Js::Number(_) => "<number>".into(),
        Js::Function(_) => "<function>".into(),
        _ => "<object>".into(),
    }
}

/// encoding/hex InvalidByteError text: `%#U` of the offending byte.
fn invalid_byte(byte: u8) -> String {
    let printable = matches!(byte, 0x20..=0x7e | 0xa1..=0xff) && byte != 0xad;
    if printable {
        format!(
            "encoding/hex: invalid byte: U+{:04X} '{}'",
            byte,
            char::from(byte)
        )
    } else {
        format!("encoding/hex: invalid byte: U+{byte:04X}")
    }
}

fn nibble(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|d| d as u8)
}

/// hex.DecodeString, including its invalid-byte-before-odd-length precedence.
fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    let src = text.as_bytes();
    let mut out = Vec::with_capacity(src.len() / 2);
    let (pairs, rest) = src.as_chunks::<2>();
    for [high, low] in pairs {
        let high = nibble(*high).ok_or_else(|| invalid_byte(*high))?;
        let low = nibble(*low).ok_or_else(|| invalid_byte(*low))?;
        out.push((high << 4) | low);
    }
    if let Some(&last) = rest.first() {
        return Err(nibble(last).map_or_else(
            || invalid_byte(last),
            |_| "encoding/hex: odd length hex string".into(),
        ));
    }
    Ok(out)
}

fn encode_hex(bytes: &[u8]) -> Js {
    Js::str(&bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

fn word_aligned(bytes: &[u8], what: &str) -> Result<(), String> {
    if bytes.len().is_multiple_of(8) {
        Ok(())
    } else {
        Err(format!("{what} length must be multiple of 8 bytes"))
    }
}

fn private_key(value: &Js, context: &str) -> Result<Vec<u8>, String> {
    let key = decode_hex(&go_string(value)).map_err(|e| format!("{context}: {e}"))?;
    if key.len() != 40 {
        return Err(format!("private key must be 40 bytes, got {}", key.len()));
    }
    Ok(key)
}

type Body = fn(&Entropy, &[Js]) -> Result<Js, Thrown>;

/// The hash input: hex text only; Go's Uint8Array branch has no js_runtime value.
fn hash_input(args: &[Js], name: &str) -> Result<Result<Vec<u8>, String>, Thrown> {
    match args.first() {
        None => Ok(Err(format!("{name} requires 1 argument"))),
        Some(Js::String(_)) => Ok(decode_hex(&go_string(&args[0]))
            .map_err(|e| format!("invalid hex string: {e}"))
            .and_then(|bytes| word_aligned(&bytes, "input").map(|()| bytes))),
        Some(_) => Err(Thrown::type_error(&format!(
            "{name} input must be a hex string"
        ))),
    }
}

fn reply(result: Result<Js, String>) -> Result<Js, Thrown> {
    Ok(result.unwrap_or_else(|message| wasm_error(&message)))
}

const METHODS: [(&str, Body); 7] = [
    ("poseidon2Hash", |_, a| {
        reply(
            hash_input(a, "poseidon2Hash")?
                .and_then(|bytes| poseidon2_hash(&bytes).map(|h| encode_hex(&h))),
        )
    }),
    ("poseidon2HashFp5", |_, a| {
        reply(
            hash_input(a, "poseidon2HashFp5")?
                .and_then(|bytes| poseidon2_hash_fp5(&bytes).map(|h| encode_hex(&h))),
        )
    }),
    ("poseidon2HashTwoToOne", |_, a| {
        reply((|| {
            if a.len() < 2 {
                return Err("poseidon2HashTwoToOne requires 2 arguments".to_string());
            }
            let left = decode_hex(&go_string(&a[0]))
                .map_err(|e| format!("invalid hex string for hash1: {e}"))?;
            let right = decode_hex(&go_string(&a[1]))
                .map_err(|e| format!("invalid hex string for hash2: {e}"))?;
            poseidon2_two_to_one(&left, &right).map(|h| encode_hex(&h))
        })())
    }),
    ("schnorrGenerateKeys", |entropy, _| {
        let key = sample_key(entropy).map_err(|e| Thrown::error("Error", &e))?;
        let public = public_key(&key).map_err(|e| Thrown::error("Error", &e))?;
        Ok(Js::object(vec![
            ("privateKey", encode_hex(&key)),
            ("publicKey", encode_hex(&public)),
        ]))
    }),
    ("schnorrPkFromSk", |_, a| {
        reply((|| {
            let Some(key) = a.first() else {
                return Err("schnorrPkFromSk requires 1 argument".to_string());
            };
            let key = private_key(key, "invalid hex string")?;
            public_key(&key).map(|pk| encode_hex(&pk))
        })())
    }),
    ("schnorrSign", |entropy, a| {
        let checked = (|| {
            if a.len() < 2 {
                return Err(
                    "schnorrSign requires 2 arguments: message (hex), privateKey (hex)".to_string(),
                );
            }
            let message =
                decode_hex(&go_string(&a[0])).map_err(|e| format!("invalid message hex: {e}"))?;
            let key = private_key(&a[1], "invalid private key hex")?;
            word_aligned(&message, "message")?;
            Ok((message, key))
        })();
        let (message, key) = match checked {
            Ok(parts) => parts,
            Err(message) => return Ok(wasm_error(&message)),
        };
        let nonce = sample(entropy).map_err(|e| Thrown::error("Error", &e))?;
        reply(schnorr_sign(&message, &key, nonce).map(|s| encode_hex(&s)))
    }),
    ("schnorrVerify", |_, a| {
        reply((|| {
            if a.len() < 3 {
                return Err("schnorrVerify requires 3 arguments: message (hex), publicKey (hex), signature (hex)".to_string());
            }
            let message =
                decode_hex(&go_string(&a[0])).map_err(|e| format!("invalid message hex: {e}"))?;
            let key = decode_hex(&go_string(&a[1]))
                .map_err(|e| format!("invalid public key hex: {e}"))?;
            let signature =
                decode_hex(&go_string(&a[2])).map_err(|e| format!("invalid signature hex: {e}"))?;
            word_aligned(&message, "message")?;
            // Go's Validate reports every key/signature failure as `false`.
            Ok(Js::Bool(
                schnorr_verify(&message, &key, &signature).unwrap_or(false),
            ))
        })())
    }),
];

/// Private key bytes drawn like a nonce: a canonical nonzero scalar.
fn sample_key(entropy: &Entropy) -> Result<[u8; 40], String> {
    for _ in 0..DRAWS {
        let mut bytes = [0u8; 40];
        entropy(&mut bytes)?;
        if SchnorrNonce::from_bytes(&bytes).is_ok() {
            return Ok(bytes);
        }
    }
    Err(format!(
        "CSPRNG produced no valid key in {DRAWS} fresh draws"
    ))
}

/// The PoseidonCrypto object node-loader's getCryptoApi returns: a fresh object
/// of own arrow members per call.
fn crypto_api(entropy: &Entropy) -> Js {
    let api = Js::object(vec![(
        "isReady",
        Js::function("isReady", |_, _| Ok(Js::Bool(true))),
    )]);
    for (name, body) in METHODS {
        let entropy = entropy.clone();
        let _ = api.define(name, Js::function(name, move |_, a| body(&entropy, &a)));
    }
    api
}
