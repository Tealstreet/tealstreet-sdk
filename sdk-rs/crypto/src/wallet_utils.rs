//! ethers.Wallet constructor and comparison facade over the native Ethereum signer.
use alloy_signer_local::PrivateKeySigner;
pub fn create_wallet_from_private_key(private_key: &str) -> Result<PrivateKeySigner, String> {
    // ethers requires exactly 32 bytes and a lowercase 0x prefix (when present).
    let body = private_key.strip_prefix("0x").unwrap_or(private_key);
    if body.len() != 64 || !body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid private key".into());
    }
    private_key
        .parse()
        .map_err(|error: alloy_signer_local::LocalSignerError| error.to_string())
}
pub fn validate_wallet_and_private_key(private_key: &str, address: &str) -> Result<bool, String> {
    let wallet = create_wallet_from_private_key(private_key)?;
    Ok(wallet.address().to_checksum(None).to_lowercase() == address.to_lowercase())
}
