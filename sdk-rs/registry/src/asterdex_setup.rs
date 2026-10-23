//! Trusted AsterDEX setup (RUST_ASTER_SIGNER_ADMISSION.md decisions 4-5).
//! Resolves the API version from the material's shape, maps both slots and
//! leaves only public config behind; the venue never sees key material.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, Secret};
use safe_cex_engine::signing::KeyEncoding;
use safe_cex_venue_asterdex::sign::{Version, is_address, is_private_key};

use crate::{ASTERDEX_MAIN_WALLET_REQUIRED, EngineOptions, VenueError, invalid};

fn nonempty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn take(value: &mut Option<String>) -> Secret {
    Secret::new(value.take().unwrap_or_default())
}

pub(crate) fn local_setup(
    options: &mut EngineOptions,
    public_data_only: bool,
) -> Result<LocalSetup, VenueError> {
    let version = options.aster_version(options.asterdex.api_version);
    let key = nonempty(&options.key);
    let secret = options.secret.as_deref().unwrap_or_default();
    let user = nonempty(&options.asterdex.user_address).or(nonempty(&options.application_id));
    // V3 wallet credentials under V1 mean the API wallet sits where the main wallet goes.
    if !public_data_only
        && version == Version::V1
        && key.as_deref().is_some_and(is_address)
        && is_private_key(secret)
        && user.is_none()
    {
        return Err(invalid(ASTERDEX_MAIN_WALLET_REQUIRED));
    }
    options.asterdex.api_version = Some(version);
    let (encoding, aliased) = match version {
        Version::V1 => (KeyEncoding::Utf8, false),
        Version::V3 => {
            let auth_signer = nonempty(&options.asterdex.signer_address);
            options.asterdex.signer_address = auth_signer.clone().or(key);
            options.asterdex.read_signer_address =
                auth_signer.or(nonempty(&options.asterdex.read_key));
            (
                KeyEncoding::Hex,
                nonempty(&options.asterdex.private_key).is_some(),
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
    // A blank wallet or secret is absent, as Aster's legacy rule trimmed it.
    let blank = wallet.as_deref().is_none_or(|text| text.trim().is_empty());
    // V3 wallet keys are trimmed hex, as the legacy venue read them; V1 HMAC
    // secrets stay byte-exact.
    if version == Version::V3
        && let Some(text) = wallet.as_mut()
    {
        let trimmed = text.trim().to_owned();
        drop(Secret::new(std::mem::replace(text, trimmed)));
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
    // The uniform `readCredentials` blob is inert here; the agent is `asterdex.readKey`.
    if let Some(mut inert) = options.read_credentials.take() {
        inert.erase_secrets();
    }
    let read_key = options.asterdex.read_key.take();
    let read_secret = options.asterdex.read_secret.take();
    // Today's read agent prefers the primary wallet even when a read secret is set.
    let secondary = read_key
        .zip(read_secret)
        .map(|(key, secret)| LocalIdentity {
            key: Secret::new(key),
            secret: if aliased {
                drop(Secret::new(secret));
                Secret::default()
            } else {
                Secret::new(secret)
            },
            secret_encoding: encoding,
            signing_from_primary: aliased,
            ..Default::default()
        });
    Ok(LocalSetup { primary, secondary })
}
