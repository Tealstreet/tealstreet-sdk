//! Function-for-function example main, over the loader API and console binding.
use crate::poseidon_loaders::{CryptoValue, LoaderError, LoaderFuture, PoseidonIndex};
use std::rc::Rc;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleValue {
    Text(String),
    Bool(bool),
    Crypto(CryptoValue),
}
pub trait ExampleConsole {
    fn log(&self, error: bool, args: &[ConsoleValue]) -> Result<(), LoaderError>;
}
fn text(s: &str) -> ConsoleValue {
    ConsoleValue::Text(s.into())
}
pub fn main(index: Rc<PoseidonIndex>, console: Rc<dyn ExampleConsole>) -> LoaderFuture<()> {
    let logged = console.log(false, &[text("Initializing WASM...")]);
    let pending = logged.map(|_| index.init_wasm());
    Box::pin(async move {
        pending?.await?;
        console.log(false, &[text("WASM initialized successfully!\n")])?;
        let crypto = index.get_crypto_api()?;
        console.log(false, &[text("=== Example 1: Poseidon2 Hash ===")])?;
        let input = "0000000000000000000000000000000000000000000000000000000000000000";
        console.log(false, &[text("Input:"), text(input)])?;
        let hash = crypto.poseidon2_hash(input.into())?;
        console.log(false, &[text("Hash:"), ConsoleValue::Crypto(hash)])?;
        console.log(false, &[])?;
        console.log(false, &[text("=== Example 2: Hash Two-to-One ===")])?;
        let h1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let h2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        console.log(false, &[text("Hash1:"), text(h1)])?;
        console.log(false, &[text("Hash2:"), text(h2)])?;
        let combined = crypto.poseidon2_hash_two_to_one(h1.into(), h2.into())?;
        console.log(false, &[text("Combined:"), ConsoleValue::Crypto(combined)])?;
        console.log(false, &[])?;
        console.log(false, &[text("=== Example 3: Generate Schnorr Keys ===")])?;
        let keys = match crypto.schnorr_generate_keys()? {
            CryptoValue::Keys(keys) => keys,
            CryptoValue::Error(e) => {
                console.log(true, &[text("Error generating keys:"), text(&e.error)])?;
                return Ok(());
            }
            _ => return Err(LoaderError::new("Expected SchnorrKeyPair or WasmError")),
        };
        console.log(false, &[text("Private Key:"), text(&keys.private_key)])?;
        console.log(false, &[text("Public Key:"), text(&keys.public_key)])?;
        console.log(false, &[])?;
        console.log(false, &[text("=== Example 4: Derive Public Key ===")])?;
        let derived = crypto.schnorr_pk_from_sk(keys.private_key.clone())?;
        let matches = matches!(&derived,CryptoValue::Text(s)if s==&keys.public_key);
        console.log(false, &[text("Derived PK:"), ConsoleValue::Crypto(derived)])?;
        console.log(
            false,
            &[text("Matches generated PK:"), ConsoleValue::Bool(matches)],
        )?;
        console.log(false, &[])?;
        console.log(false, &[text("=== Example 5: Sign and Verify ===")])?;
        let message = "0000000000000000000000000000000000000000000000000000000000000001";
        console.log(false, &[text("Message:"), text(message)])?;
        let signature = match crypto.schnorr_sign(message.into(), keys.private_key)? {
            CryptoValue::Text(signature) => signature,
            CryptoValue::Error(e) => {
                console.log(true, &[text("Error signing:"), text(&e.error)])?;
                return Ok(());
            }
            _ => return Err(LoaderError::new("Expected signature or WasmError")),
        };
        console.log(false, &[text("Signature:"), text(&signature)])?;
        let valid = crypto.schnorr_verify(message.into(), keys.public_key, signature.clone())?;
        console.log(
            false,
            &[text("Signature valid:"), ConsoleValue::Crypto(valid)],
        )?;
        let wrong = match crypto.schnorr_generate_keys()? {
            CryptoValue::Keys(keys) => keys,
            CryptoValue::Error(e) => {
                console.log(
                    true,
                    &[text("Error generating wrong keys:"), text(&e.error)],
                )?;
                return Ok(());
            }
            _ => return Err(LoaderError::new("Expected SchnorrKeyPair or WasmError")),
        };
        let invalid = crypto.schnorr_verify(message.into(), wrong.public_key, signature)?;
        console.log(
            false,
            &[
                text("Wrong key verification (should be false):"),
                ConsoleValue::Crypto(invalid),
            ],
        )?;
        Ok(())
    })
}
