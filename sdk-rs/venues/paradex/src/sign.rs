use alloy_primitives::U256;
use safe_cex_crypto::ethereum;
use safe_cex_engine::signing::KeyRef;
use serde::Deserialize;
use starknet_crypto::{Felt, pedersen_hash};

pub(crate) const CURVE_ORDER: &str =
    "3618502788666131213697322783095070105526743751716087489154079457884512865583";
const FIELD_PRIME: &str = "800000000000011000000000000000000000000000000000000000000000001";

#[derive(Clone, Deserialize)]
pub struct SystemConfig {
    pub starknet_chain_id: String,
    #[serde(default)]
    pub l1_chain_id: Option<String>,
    #[serde(default)]
    pub paraclear_account_hash: Option<String>,
    #[serde(default)]
    pub paraclear_account_proxy_hash: Option<String>,
}

pub mod dispatch;

/// Public signing facts: the account, chain and the provider's sealed Stark
/// key. The scalar is never here; every signature comes from the provider.
#[derive(Clone)]
pub struct Credentials {
    signing: KeyRef,
    public: Felt,
    address_felt: Felt,
    address: String,
    subkey: String,
    chain_id: Felt,
}

fn unsigned(input: &str) -> Result<U256, String> {
    let (digits, radix) = if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        (hex, 16)
    } else {
        (input, 10)
    };
    if digits.is_empty()
        || !digits.bytes().all(|b| {
            if radix == 16 {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err("Invalid Paradex field value".into());
    }
    U256::from_str_radix(digits, radix).map_err(|_| "Invalid Paradex field value".into())
}

pub(crate) fn felt(input: &str) -> Result<Felt, String> {
    let number = unsigned(input)?;
    if number >= U256::from_str_radix(FIELD_PRIME, 16).expect("field prime") {
        return Err("Paradex field value is out of range".into());
    }
    Ok(Felt::from_bytes_be(&number.to_be_bytes()))
}

pub(crate) fn chain_id(input: &str) -> Result<Felt, String> {
    let value = if input.starts_with("0x")
        || input.starts_with("0X")
        || input.bytes().all(|b| b.is_ascii_digit())
    {
        felt(input)?
    } else {
        short_string(input).map_err(|_| "Invalid Paradex chain ID")?
    };
    if value == Felt::ZERO {
        return Err("Invalid Paradex chain ID".into());
    }
    Ok(value)
}

/// The Ethereum chain an Ethereum-derived Stark key was derived on.
pub fn ethereum_chain(testnet: bool) -> &'static str {
    if testnet { "11155111" } else { "1" }
}

impl Credentials {
    /// `derived` accounts own the address their sealed key derives under the
    /// configured class hashes, on the network's fixed Ethereum chain; a
    /// configured chain that differs refuses rather than re-deriving.
    pub fn new(
        signing: KeyRef,
        public: [u8; 32],
        account: &str,
        subkey: &str,
        derived: Option<bool>,
        config: &SystemConfig,
    ) -> Result<Self, String> {
        let supplied_address = felt(account).map_err(|_| "Invalid Paradex account address")?;
        if supplied_address == Felt::ZERO
            || felt(subkey).map_err(|_| "Invalid Paradex subkey address")? == Felt::ZERO
        {
            return Err("Invalid Paradex account or subkey address".into());
        }
        let chain_id = chain_id(&config.starknet_chain_id)?;
        let public = Felt::from_bytes_be(&public);
        let (address_felt, address) = match derived {
            None => (supplied_address, account.to_lowercase()),
            Some(testnet) => {
                let expected = unsigned(ethereum_chain(testnet)).expect("fixed chain");
                let configured = config
                    .l1_chain_id
                    .as_deref()
                    .ok_or("Missing Paradex Ethereum chain ID")?;
                if unsigned(configured).map_err(|_| "Invalid Paradex Ethereum chain ID")?
                    != expected
                {
                    return Err("Paradex Ethereum chain ID does not match this network".into());
                }
                let address = derived_address(public, config)?;
                if address == Felt::ZERO {
                    return Err("Invalid Paradex derived account address".into());
                }
                (address, format!("{address:#x}"))
            }
        };
        Ok(Self {
            signing,
            public,
            address_felt,
            address,
            subkey: subkey.into(),
            chain_id,
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn subkey(&self) -> &str {
        &self.subkey
    }
    /// The `/auth` typed-data hash for these seconds.
    pub fn auth_hash(&self, timestamp: u64, expiration: u64) -> Result<Felt, String> {
        typed_hash(
            self.chain_id,
            self.address_felt,
            AUTH_TYPE,
            &auth_values(timestamp, expiration),
        )
    }
    /// An order's (or, with `id`, an amendment's) typed-data hash at `now_ms`.
    pub fn order_hash(&self, now_ms: u64, order: &dispatch::Preimage<'_>) -> Result<Felt, String> {
        let (definition, values, len) = order_values(
            now_ms,
            order.market,
            order.side,
            order.kind,
            order.size,
            order.price,
            order.id,
        )?;
        typed_hash(self.chain_id, self.address_felt, definition, &values[..len])
    }
    /// The venue's signature text for a canonical provider `r || s`.
    pub fn signature_text(r: &[u8; 32], s: &[u8; 32]) -> String {
        format!(
            "[\"{}\",\"{}\"]",
            U256::from_be_bytes(*r),
            U256::from_be_bytes(*s)
        )
    }
}

pub(crate) const AUTH_TYPE: &str =
    "Request(method:felt,path:felt,body:felt,timestamp:felt,expiration:felt)";

pub(crate) fn auth_values(timestamp: u64, expiration: u64) -> [Felt; 5] {
    [
        short_string("POST").expect("short string"),
        short_string("/v1/auth").expect("short string"),
        Felt::ZERO,
        Felt::from(timestamp),
        Felt::from(expiration),
    ]
}

/// The native order/amendment preimage: one definition for local signing and
/// hosted descriptors, so both hash exactly the same fields.
pub(crate) fn order_values(
    now_ms: u64,
    market: &str,
    side: &str,
    order_type: &str,
    size: &str,
    price: Option<&str>,
    id: Option<&str>,
) -> Result<(&'static str, [Felt; 7], usize), String> {
    let side = match side {
        "BUY" => Felt::ONE,
        "SELL" => Felt::TWO,
        _ => return Err("Invalid Paradex order side".into()),
    };
    let size = quantum(size)?;
    let price = if order_type == "MARKET" || price.is_none_or(str::is_empty) {
        "0".into()
    } else {
        quantum(price.expect("price is present"))?
    };
    let mut values = [
        Felt::from(now_ms),
        word(market)?,
        side,
        word(order_type)?,
        felt(&size)?,
        felt(&price)?,
        Felt::ZERO,
    ];
    let (definition, len) = if let Some(id) = id {
        values[6] = word(id)?;
        (
            "ModifyOrder(timestamp:felt,market:felt,side:felt,orderType:felt,size:felt,price:felt,id:felt)",
            7,
        )
    } else {
        (
            "Order(timestamp:felt,market:felt,side:felt,orderType:felt,size:felt,price:felt)",
            6,
        )
    };
    Ok((definition, values, len))
}

/// The Paradex StarkNet typed-data message hash over `values`.
pub(crate) fn typed_hash(
    chain_id: Felt,
    account: Felt,
    definition: &str,
    values: &[Felt],
) -> Result<Felt, String> {
    let mut fields = vec![selector(definition).map_err(|_| "Invalid Paradex signing type")?];
    fields.extend_from_slice(values);
    let domain = hash_elements(&[
        selector("StarkNetDomain(name:felt,chainId:felt,version:felt)").expect("ASCII type"),
        short_string("Paradex").expect("short string"),
        chain_id,
        Felt::ONE,
    ]);
    Ok(hash_elements(&[
        short_string("StarkNet Message").expect("short string"),
        domain,
        account,
        hash_elements(&fields),
    ]))
}

pub(crate) fn word(input: &str) -> Result<Felt, String> {
    if input.is_empty() {
        return Ok(Felt::ZERO);
    }
    if input.starts_with("0x")
        || input.starts_with("0X")
        || input.bytes().all(|b| b.is_ascii_digit())
    {
        felt(input)
    } else {
        short_string(input).map_err(|_| "Invalid Paradex signing field".into())
    }
}

/// Integer truncation after scaling, without converting signed decimals through f64.
pub fn quantum(input: &str) -> Result<String, String> {
    let invalid = || "Invalid Paradex order quantum".to_owned();
    let input = input.strip_prefix('+').unwrap_or(input);
    let (mantissa, exponent) = match input.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().map_err(|_| invalid())?),
        None => (input, 0),
    };
    let mut digits = String::new();
    let mut decimal = None;
    for b in mantissa.bytes() {
        match b {
            b'0'..=b'9' => digits.push(b as char),
            b'.' if decimal.is_none() => decimal = Some(digits.len()),
            _ => return Err(invalid()),
        }
    }
    if digits.is_empty() {
        return Err(invalid());
    }
    let integer_digits = decimal.unwrap_or(digits.len()) as i64 + i64::from(exponent) + 8;
    let result = if integer_digits <= 0 {
        "0".into()
    } else {
        if integer_digits > 80 {
            return Err(invalid());
        }
        let length = integer_digits as usize;
        digits.truncate(length);
        if digits.len() < length {
            digits.extend(std::iter::repeat_n('0', length - digits.len()));
        }
        let trimmed = digits.trim_start_matches('0');
        if trimmed.is_empty() {
            "0".into()
        } else {
            trimmed.to_owned()
        }
    };
    felt(&result).map_err(|_| invalid())?;
    Ok(result)
}

// Paradex uses only these fixed Cairo-0 encodings. Keeping them here avoids the
// unrelated JSON-RPC and ABI implementations of the general Starknet SDK.
fn short_string(input: &str) -> Result<Felt, &'static str> {
    if !input.is_ascii() || input.len() > 31 {
        return Err("Invalid Paradex short string");
    }
    let mut bytes = [0u8; 32];
    bytes[32 - input.len()..].copy_from_slice(input.as_bytes());
    Ok(Felt::from_bytes_be(&bytes))
}

fn selector(input: &str) -> Result<Felt, &'static str> {
    if !input.is_ascii() {
        return Err("Invalid Paradex selector");
    }
    let mut hash = ethereum::keccak256(input.as_bytes());
    hash[0] &= 0x03; // Starknet keccak is the low 250 bits of Keccak-256.
    Ok(Felt::from_bytes_be(&hash))
}

fn hash_elements(elements: &[Felt]) -> Felt {
    let hash = elements
        .iter()
        .fold(Felt::ZERO, |hash, value| pedersen_hash(&hash, value));
    pedersen_hash(&hash, &Felt::from(elements.len()))
}

fn account_address(public: Felt, proxy: Felt, calldata: &[Felt]) -> Felt {
    let hash = hash_elements(&[
        short_string("STARKNET_CONTRACT_ADDRESS").expect("short string"),
        Felt::ZERO, // Account deployment has no deployer contract.
        public,
        proxy,
        hash_elements(calldata),
    ]);
    // Starknet contract addresses are reduced modulo 2^251 - 256.
    let bound: U256 = (U256::ONE << 251) - U256::from(256);
    let address = U256::from_be_bytes(hash.to_bytes_be()) % bound;
    Felt::from_bytes_be(&address.to_be_bytes())
}

/// The Paraclear account address a Stark public key derives under `config`.
fn derived_address(public: Felt, config: &SystemConfig) -> Result<Felt, String> {
    let implementation = felt(
        config
            .paraclear_account_hash
            .as_deref()
            .ok_or("Missing Paradex account class hash")?,
    )?;
    let proxy = felt(
        config
            .paraclear_account_proxy_hash
            .as_deref()
            .ok_or("Missing Paradex proxy class hash")?,
    )?;
    let calldata = [
        implementation,
        selector("initialize").expect("ASCII selector"),
        Felt::TWO,
        public,
        Felt::ZERO,
    ];
    Ok(account_address(public, proxy, &calldata))
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::starknet;

    #[derive(Deserialize)]
    struct TextVector {
        input: String,
        output: String,
    }
    #[derive(Deserialize)]
    struct HashVector {
        input: Vec<String>,
        output: String,
    }
    #[derive(Deserialize)]
    struct AddressVector {
        public: String,
        proxy: String,
        calldata: Vec<String>,
        output: String,
    }
    #[derive(Deserialize)]
    struct EthereumVector {
        chain: String,
        key: String,
        digest: String,
        seed: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Formulas {
        short_strings: Vec<TextVector>,
        selectors: Vec<TextVector>,
        hashes: Vec<HashVector>,
        addresses: Vec<AddressVector>,
        ethereum: Vec<EthereumVector>,
    }
    fn vectors() -> Formulas {
        serde_json::from_str(include_str!("../tests/sign-formulas.json")).unwrap()
    }

    #[test]
    fn fixed_cairo_encodings_match_independent_starknet_js() {
        let vectors = vectors();
        for row in vectors.short_strings {
            assert_eq!(
                short_string(&row.input).unwrap(),
                felt(&row.output).unwrap()
            );
        }
        for row in vectors.selectors {
            assert_eq!(selector(&row.input).unwrap(), felt(&row.output).unwrap());
        }
        for row in vectors.hashes {
            let fields: Vec<_> = row.input.iter().map(|value| felt(value).unwrap()).collect();
            assert_eq!(hash_elements(&fields), felt(&row.output).unwrap());
        }
        for row in vectors.addresses {
            let calldata: Vec<_> = row
                .calldata
                .iter()
                .map(|value| felt(value).unwrap())
                .collect();
            assert_eq!(
                account_address(
                    felt(&row.public).unwrap(),
                    felt(&row.proxy).unwrap(),
                    &calldata
                ),
                felt(&row.output).unwrap()
            );
        }
        assert!(short_string(&"A".repeat(32)).is_err());
        assert!(short_string("Parádex").is_err());
        assert!(selector("Parádex").is_err());
    }

    #[test]
    fn fixed_eip712_digest_and_seed_match_independent_ethers() {
        for row in vectors().ethereum {
            let digest = starknet::ethereum_key_digest(&row.chain).unwrap();
            assert_eq!(format!("0x{}", safe_cex_crypto::hex(&digest)), row.digest);
            assert_eq!(ethereum::sign_digest(&digest, &row.key).unwrap(), row.seed);
        }
        for chain in [
            "",
            "-1",
            "1.5",
            " 1",
            "0x",
            "0xq",
            "1e3",
            "0x10000000000000000000000000000000000000000000000000000000000000000",
        ] {
            let error = starknet::ethereum_key_digest(chain).unwrap_err();
            assert_eq!(error, "Invalid Paradex Ethereum chain ID");
        }
    }
}
