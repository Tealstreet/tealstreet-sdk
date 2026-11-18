//! Offline tool for Tealstreet's Lighter builder-fee exemptions.
//!
//! Run it on a trusted, offline machine. The seed file is the signing key:
//! never commit it, upload it, or put it on a server.
//!
//!   cargo run -p safe-cex-venue-lighter --example builder_exemption -- keygen <seed-file>
//!       Writes a new 32-byte seed (hex) and prints the public key line for KEYS.
//!   cargo run -p safe-cex-venue-lighter --example builder_exemption -- sign <seed-file> <key-id> <account-index> <days> [mainnet|robinhood]
//!       Prints an `lbx1.` exemption for one account, valid for <days> (max 400).
//!   cargo run -p safe-cex-venue-lighter --example builder_exemption -- check <public-key-hex> <key-id> <token> [mainnet|robinhood]
//!       Verifies a token against a public key and prints its account and expiry.

use safe_cex_crypto::ed25519;
use safe_cex_venue_lighter::builder_exemption::{MAX_LIFETIME_S, payload, token, verify};
use std::time::{SystemTime, UNIX_EPOCH};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(text: &str) -> Result<[u8; N], String> {
    let text = text.trim();
    if text.len() != N * 2 {
        return Err(format!("expected {} hex characters", N * 2));
    }
    let mut out = [0u8; N];
    for (i, pair) in text.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).map_err(|e| e.to_string())?, 16)
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["keygen", path] => {
            if std::path::Path::new(path).exists() {
                return Err(format!("{path} already exists; refusing to overwrite a key"));
            }
            let mut seed = [0u8; 32];
            use std::io::Read;
            std::fs::File::open("/dev/urandom")
                .and_then(|mut f| f.read_exact(&mut seed))
                .map_err(|e| format!("no system randomness: {e}"))?;
            std::fs::write(path, hex(&seed)).map_err(|e| e.to_string())?;
            let public = ed25519::public_key(&seed)?;
            println!("Seed written to {path}. Keep it offline.");
            println!("Public key: {}", hex(&public));
            println!("KEYS entry: (1, {:?}),", public);
            Ok(())
        }
        ["sign", path, key_id, account, days, ref rest @ ..] => {
            let network = rest.first().copied().unwrap_or("mainnet");
            let seed = unhex::<32>(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)?;
            let key_id: u32 = key_id.parse().map_err(|_| "key id must be a number")?;
            let account: u64 = account.parse().map_err(|_| "account index must be a number")?;
            let days: u64 = days.parse().map_err(|_| "days must be a number")?;
            if days == 0 || days * 86_400 > MAX_LIFETIME_S {
                return Err("days must be between 1 and 400".into());
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
            let expires = now + days * 86_400;
            let sig = ed25519::sign(payload(network, account, expires, key_id).as_bytes(), &seed)?;
            println!("{}", token(account, expires, key_id, &sig));
            Ok(())
        }
        ["check", public, key_id, tok, ref rest @ ..] => {
            let network = rest.first().copied().unwrap_or("mainnet");
            let public = unhex::<32>(public)?;
            let key_id: u32 = key_id.parse().map_err(|_| "key id must be a number")?;
            let exemption = verify(tok, network, &[(key_id, public)]).map_err(str::to_owned)?;
            println!("valid: account {} on {network}, expires {} (unix s)", exemption.account_index, exemption.expires_s);
            Ok(())
        }
        _ => Err("usage: keygen <seed-file> | sign <seed-file> <key-id> <account> <days> [network] | check <public-key-hex> <key-id> <token> [network]".into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
