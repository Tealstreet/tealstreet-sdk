//! Public facts projected under an independently captured, verified owner.
//! Parsing is not evidence of key provenance and grants no registry authority.
use super::{Family, PublicKey};
use safe_cex_engine::signing::SignError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Venue {
    Backpack,
    Hyperliquid,
    Hyperliquidspot,
    Grvt,
    Txflow,
    Polymarket,
    Asterdex,
    Orderly,
    Kodiakfi,
    Mode,
    What,
    Woofi,
    Lighter,
    #[serde(rename = "lighter-robinhood")]
    LighterRobinhood,
    Paradex,
}
impl Venue {
    fn family(self) -> Family {
        match self {
            Self::Backpack
            | Self::Orderly
            | Self::Kodiakfi
            | Self::Mode
            | Self::What
            | Self::Woofi => Family::Ed25519,
            Self::Lighter | Self::LighterRobinhood => Family::Schnorr,
            Self::Paradex => Family::Stark,
            Self::Hyperliquid
            | Self::Hyperliquidspot
            | Self::Grvt
            | Self::Txflow
            | Self::Polymarket
            | Self::Asterdex => Family::Ethereum,
        }
    }
    fn namespace(self) -> &'static str {
        match self {
            Self::Backpack => "backpack",
            Self::Hyperliquid | Self::Hyperliquidspot => "hyperliquid",
            Self::Grvt => "grvt",
            Self::Txflow => "txflow",
            Self::Polymarket => "polymarket",
            Self::Asterdex => "asterdex",
            Self::Orderly => "orderly",
            Self::Kodiakfi => "kodiakfi",
            Self::Mode => "mode",
            Self::What => "what",
            Self::Woofi => "woofi",
            Self::Lighter => "lighter",
            Self::LighterRobinhood => "lighter-robinhood",
            Self::Paradex => "paradex",
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Network {
    Mainnet,
    Testnet,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Facts {
    venue: Venue,
    network: Network,
    credential: Credential,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Credential {
    Ed25519 {
        public_key: String,
    },
    Agent {
        signer: String,
        account_scope: AccountScope,
    },
    Grvt {
        trading_account: String,
        signer: String,
    },
    Wallet {
        signer: String,
    },
    Polymarket {
        signer: String,
        funder: String,
    },
    AsterV3 {
        user: String,
        signer: String,
    },
    Orderly {
        account: String,
        public_key: String,
    },
    Schnorr {
        account_index: String,
        api_key_index: u8,
        registered_public_key: String,
    },
    Stark {
        account: String,
        public_key: String,
    },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum AccountScope {
    Main {},
    Vault { address: String },
}

pub struct Projection {
    digest: [u8; 32],
    scope: String,
    key_id: String,
    public: PublicKey,
}
crate::redacted_debug!(Projection);
impl Projection {
    /// Facts must come from the captured account/registration, never a proof reply.
    pub fn from_json(verified_owner: &str, input: &str) -> Result<Self, SignError> {
        text(verified_owner)?;
        if input.len() > 8192 {
            return Err(SignError::InvalidRequest);
        }
        let facts: Facts = serde_json::from_str(input).map_err(|_| SignError::InvalidRequest)?;
        let (components, public) = project(facts.venue, facts.credential)?;
        let scope = serde_json::to_string(&[
            facts.venue.namespace(),
            match facts.network {
                Network::Mainnet => "mainnet",
                Network::Testnet => "testnet",
            },
            "",
        ])
        .map_err(|_| SignError::InvalidRequest)?;
        let key_id = serde_json::to_string(&components).map_err(|_| SignError::InvalidRequest)?;
        let mut digest = Sha256::new();
        digest.update(b"tealstreet/identity/v1");
        for value in [verified_owner, &scope, &key_id] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        Ok(Self {
            digest: digest.finalize().into(),
            scope,
            key_id,
            public,
        })
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }
}
fn text(value: &str) -> Result<(), SignError> {
    if value.is_empty()
        || value.len() > 1024
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(SignError::InvalidRequest);
    }
    Ok(())
}
fn bytes(value: &str, width: usize) -> Result<Vec<u8>, SignError> {
    let value = value.strip_prefix("0x").ok_or(SignError::InvalidRequest)?;
    if value.len() != width * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(SignError::InvalidRequest);
    }
    let bytes = value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let digit = |b: u8| {
                if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b.to_ascii_lowercase() - b'a' + 10
                }
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect::<Vec<_>>();
    if bytes.iter().all(|b| *b == 0) {
        return Err(SignError::InvalidRequest);
    }
    Ok(bytes)
}
fn canonical(value: &str, width: usize) -> Result<String, SignError> {
    Ok(format!("0x{}", crate::hex(&bytes(value, width)?)))
}
fn signer(value: &str, family: Family) -> Result<(String, PublicKey), SignError> {
    let width = match family {
        Family::Ethereum => 20,
        Family::Schnorr => 40,
        _ => 32,
    };
    let raw = bytes(value, width)?;
    Ok((
        format!("0x{}", crate::hex(&raw)),
        PublicKey::new(family, &raw)?,
    ))
}
fn project(venue: Venue, credential: Credential) -> Result<(Vec<String>, PublicKey), SignError> {
    use Credential as C;
    use Venue as V;
    let family = venue.family();
    let tag: &str;
    let mut before = Vec::new();
    let mut after = Vec::new();
    let public;
    match (venue, credential) {
        (V::Backpack, C::Ed25519 { public_key }) => {
            tag = "ed25519";
            public = public_key;
        }
        (
            V::Hyperliquid | V::Hyperliquidspot,
            C::Agent {
                signer,
                account_scope,
            },
        ) => {
            tag = "agent";
            public = signer;
            match account_scope {
                AccountScope::Main {} => after.push("main".into()),
                AccountScope::Vault { address } => {
                    after.push("vault".into());
                    after.push(canonical(&address, 20)?);
                }
            }
        }
        (
            V::Grvt,
            C::Grvt {
                trading_account,
                signer,
            },
        ) => {
            text(&trading_account)?;
            tag = "grvt";
            before.push(trading_account);
            public = signer;
        }
        (V::Txflow, C::Wallet { signer }) => {
            tag = "wallet";
            public = signer;
        }
        (V::Polymarket, C::Polymarket { signer, funder }) => {
            tag = "polymarket";
            public = signer;
            after.push(canonical(&funder, 20)?);
        }
        (V::Asterdex, C::AsterV3 { user, signer }) => {
            tag = "aster-v3";
            before.push(canonical(&user, 20)?);
            public = signer;
        }
        (
            V::Orderly | V::Kodiakfi | V::Mode | V::What | V::Woofi,
            C::Orderly {
                account,
                public_key,
            },
        ) => {
            tag = "orderly";
            before.push(canonical(&account, 32)?);
            public = public_key;
        }
        (
            V::Lighter | V::LighterRobinhood,
            C::Schnorr {
                account_index,
                api_key_index,
                registered_public_key,
            },
        ) => {
            if account_index.is_empty()
                || account_index.len() > 20
                || !account_index.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(SignError::InvalidRequest);
            }
            let index = account_index
                .parse::<u64>()
                .map_err(|_| SignError::InvalidRequest)?;
            tag = "schnorr";
            before.extend([index.to_string(), api_key_index.to_string()]);
            public = registered_public_key;
        }
        (
            V::Paradex,
            C::Stark {
                account,
                public_key,
            },
        ) => {
            let raw: [u8; 32] = bytes(&account, 32)?
                .try_into()
                .map_err(|_| SignError::InvalidRequest)?;
            if starknet_crypto::Felt::from_bytes_be(&raw).to_bytes_be() != raw {
                return Err(SignError::InvalidRequest);
            }
            tag = "stark";
            before.push(canonical(&account, 32)?);
            public = public_key;
        }
        _ => return Err(SignError::InvalidRequest),
    }
    let (public_text, public) = signer(&public, family)?;
    let mut components = vec![tag.into()];
    components.extend(before);
    components.push(public_text);
    components.extend(after);
    Ok((components, public))
}
