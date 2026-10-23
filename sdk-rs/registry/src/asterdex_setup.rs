//! Trusted AsterDEX setup (RUST_ASTER_SIGNER_ADMISSION.md decisions 4-5).
//! Resolves the API version from the material's shape, maps both slots and
//! leaves only public config behind; the venue never sees key material.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, Secret};
use safe_cex_engine::signing::KeyEncoding;
use safe_cex_venue_asterdex::sign::{Version, is_address, is_private_key};

use crate::{ASTERDEX_MAIN_WALLET_REQUIRED, EngineOptions, VenueError, invalid};

/// Borrowed, so key material is never copied outside a `Secret`.
fn trimmed(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn take(value: &mut Option<String>) -> Secret {
    Secret::new(value.take().unwrap_or_default())
}

pub(crate) fn local_setup(
    options: &mut EngineOptions,
    public_data_only: bool,
) -> Result<LocalSetup, VenueError> {
    let version = options.aster_version(options.asterdex.api_version);
    let secret = options.secret.as_deref().unwrap_or_default();
    let user = trimmed(&options.asterdex.user_address).or(trimmed(&options.application_id));
    // V3 wallet credentials under V1 mean the API wallet sits where the main wallet goes.
    if !public_data_only
        && version == Version::V1
        && trimmed(&options.key).is_some_and(is_address)
        && is_private_key(secret)
        && user.is_none()
    {
        return Err(invalid(ASTERDEX_MAIN_WALLET_REQUIRED));
    }
    options.asterdex.api_version = Some(version);
    let (encoding, aliased) = match version {
        Version::V1 => (KeyEncoding::Utf8, false),
        Version::V3 => {
            // V3 keys are the API wallets' public addresses.
            let auth_signer = trimmed(&options.asterdex.signer_address).map(str::to_owned);
            options.asterdex.read_signer_address = auth_signer
                .clone()
                .or(trimmed(&options.asterdex.read_key).map(str::to_owned));
            options.asterdex.signer_address =
                auth_signer.or(trimmed(&options.key).map(str::to_owned));
            (
                KeyEncoding::Hex,
                trimmed(&options.asterdex.private_key).is_some(),
            )
        }
    };
    // V3 signs with the wallet key: `auth.privateKey`, else the secret.
    let wallet = if version == Version::V3 && aliased {
        drop(take(&mut options.secret));
        &mut options.asterdex.private_key
    } else {
        drop(take(&mut options.asterdex.private_key));
        &mut options.secret
    };
    // Blank is absent. V3 wallet keys are trimmed hex, as the legacy venue
    // read them; V1 HMAC secrets stay byte-exact.
    let blank = wallet.as_deref().is_none_or(|text| {
        let text = text.trim();
        text.is_empty() || (version == Version::V3 && text.eq_ignore_ascii_case("0x"))
    });
    if version == Version::V3
        && let Some(text) = wallet.as_mut()
    {
        let exact = text.trim().to_owned();
        drop(Secret::new(std::mem::replace(text, exact)));
    }
    let wallet = take(wallet);
    let wallet = if blank {
        drop(wallet);
        Secret::default()
    } else {
        wallet
    };
    let primary = (options.key.is_some() || !blank).then(|| LocalIdentity {
        secret: wallet,
        key: take(&mut options.key),
        secret_encoding: encoding,
        ..Default::default()
    });
    // Aster has no passphrase slot.
    drop(take(&mut options.passphrase));
    // The uniform `readCredentials` blob is inert here; the agent is `asterdex.readKey`.
    if let Some(mut inert) = options.read_credentials.take() {
        inert.erase_secrets();
    }
    let read_key = options.asterdex.read_key.take().map(Secret::new);
    let read_secret = options.asterdex.read_secret.take().map(Secret::new);
    // A half-supplied read agent installs nothing, as before. Today's read
    // agent prefers the primary wallet even when a read secret is set.
    let secondary = read_key
        .zip(read_secret)
        .map(|(key, secret)| LocalIdentity {
            key,
            secret: if aliased {
                drop(secret);
                Secret::default()
            } else {
                secret
            },
            secret_encoding: encoding,
            signing_from_primary: aliased,
            ..Default::default()
        });
    Ok(LocalSetup { primary, secondary })
}
