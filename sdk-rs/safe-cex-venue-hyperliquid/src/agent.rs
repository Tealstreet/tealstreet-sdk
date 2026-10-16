//! Pure agent leaves; wallet creation/signing/connection remains pending.
use crate::builder_fee::ApprovalField;

pub const APPROVE_AGENT_TYPES: &[(&str, &[ApprovalField])] = &[(
    "HyperliquidTransaction:ApproveAgent",
    &[
        ApprovalField {
            name: "hyperliquidChain",
            r#type: "string",
        },
        ApprovalField {
            name: "agentAddress",
            r#type: "address",
        },
        ApprovalField {
            name: "agentName",
            r#type: "string",
        },
        ApprovalField {
            name: "nonce",
            r#type: "uint64",
        },
    ],
)];

/// The host supplies each Math.random property read/call, including errors.
/// Four fresh calls preserve source order; the name is always ASCII.
pub fn generate_agent_wallet_name<E>(
    mut random: impl FnMut() -> Result<f64, E>,
) -> Result<String, E> {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut suffix = String::new();
    for _ in 0..4 {
        let index = (random()? * CHARS.len() as f64).floor();
        // String.charAt converts NaN to zero and out-of-range infinities or
        // negative indices to the empty string. -0 also selects index zero.
        let index = if index.is_nan() { 0.0 } else { index };
        if index >= 0.0 && index < CHARS.len() as f64 {
            suffix.push(CHARS[index as usize] as char);
        }
    }
    Ok(format!("Tealstreet {suffix}"))
}

/// Host operations represent the availability read and guarded second read.
/// ethereum_truthy freshly looks up window again, then reads ethereum and applies
/// JavaScript ToBoolean, preserving errors from either lookup;
/// adapters may expose any JS value without forcing it through JSON/UTF-8.
pub trait EthereumWindow {
    type Error;
    fn window_defined(&mut self) -> Result<bool, Self::Error>;
    /// Perform a fresh second window lookup, then ethereum read and ToBoolean.
    fn ethereum_truthy(&mut self) -> Result<bool, Self::Error>;
}

pub fn has_ethereum_wallet<W: EthereumWindow + ?Sized>(window: &mut W) -> Result<bool, W::Error> {
    if !window.window_defined()? {
        return Ok(false);
    }
    window.ethereum_truthy()
}
