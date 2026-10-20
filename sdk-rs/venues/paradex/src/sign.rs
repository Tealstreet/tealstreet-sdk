use alloy_primitives::U256;
use safe_cex_crypto::{eip712, sha256, starknet};
use serde::{Deserialize, Serialize};
use starknet_core::{
    crypto::compute_hash_on_elements,
    types::Felt,
    utils::{cairo_short_string_to_felt, get_contract_address, get_selector_from_name},
};
use std::collections::BTreeMap;

const CURVE_ORDER: &str =
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

pub struct Credentials {
    key: Felt,
    address_felt: Felt,
    address: String,
    subkey: String,
    chain_id: String,
}

#[derive(Debug, Serialize)]
pub struct OrderSignature {
    pub signature: String,
    pub signature_timestamp: u64,
}

pub struct AuthSignature {
    pub signature: String,
    pub timestamp: u64,
    pub expiration: u64,
}

impl AuthSignature {
    pub fn headers(&self, address: &str) -> Vec<(&'static str, String)> {
        vec![
            ("Content-Type", "application/json".into()),
            ("Accept", "application/json".into()),
            ("PARADEX-STARKNET-ACCOUNT", address.into()),
            ("PARADEX-STARKNET-SIGNATURE", self.signature.clone()),
            ("PARADEX-TIMESTAMP", self.timestamp.to_string()),
            ("PARADEX-SIGNATURE-EXPIRATION", self.expiration.to_string()),
        ]
    }
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

fn felt(input: &str) -> Result<Felt, String> {
    let number = unsigned(input)?;
    if number >= U256::from_str_radix(FIELD_PRIME, 16).expect("field prime") {
        return Err("Paradex field value is out of range".into());
    }
    Ok(Felt::from_bytes_be(&number.to_be_bytes()))
}

fn secret(input: &str) -> Result<Felt, String> {
    let value = unsigned(input).map_err(|_| "Invalid Paradex private key")?;
    let order = U256::from_str_radix(CURVE_ORDER, 10).expect("curve order");
    if value == U256::ZERO || value >= order {
        return Err("Invalid Paradex private key".into());
    }
    Ok(Felt::from_bytes_be(&value.to_be_bytes()))
}

fn chain_id(input: &str) -> Result<String, String> {
    let value = if input.starts_with("0x")
        || input.starts_with("0X")
        || input.bytes().all(|b| b.is_ascii_digit())
    {
        felt(input)?
    } else {
        cairo_short_string_to_felt(input).map_err(|_| "Invalid Paradex chain ID")?
    };
    if value == Felt::ZERO {
        return Err("Invalid Paradex chain ID".into());
    }
    Ok(format!("{value:#x}"))
}

#[derive(Serialize)]
struct Field {
    name: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct Domain<'a> {
    name: &'static str,
    #[serde(rename = "chainId")]
    chain_id: &'a str,
    version: &'static str,
}

impl Credentials {
    pub fn new(
        key: &str,
        address: &str,
        subkey: &str,
        config: &SystemConfig,
    ) -> Result<Self, String> {
        let supplied_address = felt(address).map_err(|_| "Invalid Paradex account address")?;
        if supplied_address == Felt::ZERO
            || felt(subkey).map_err(|_| "Invalid Paradex subkey address")? == Felt::ZERO
        {
            return Err("Invalid Paradex account or subkey address".into());
        }
        let chain_id = chain_id(&config.starknet_chain_id)?;
        let (key, address_felt, address) = if key.len() == 66 {
            (secret(key)?, supplied_address, address.to_lowercase())
        } else {
            derive_ethereum(key, config)?
        };
        Ok(Self {
            key,
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

    fn signature(&self, primary: &str, names: &[&str], values: &[Felt]) -> Result<String, String> {
        let definition = format!(
            "{primary}({})",
            names
                .iter()
                .map(|name| format!("{name}:felt"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut fields =
            vec![get_selector_from_name(&definition).map_err(|_| "Invalid Paradex signing type")?];
        fields.extend_from_slice(values);
        let domain = compute_hash_on_elements(&[
            get_selector_from_name("StarkNetDomain(name:felt,chainId:felt,version:felt)")
                .expect("ASCII type"),
            cairo_short_string_to_felt("Paradex").expect("short string"),
            felt(&self.chain_id)?,
            Felt::ONE,
        ]);
        let hash = compute_hash_on_elements(&[
            cairo_short_string_to_felt("StarkNet Message").expect("short string"),
            domain,
            self.address_felt,
            compute_hash_on_elements(&fields),
        ]);
        let (r, s) = starknet::sign(&hash, &self.key).map_err(|_| "Paradex signing failed")?;
        Ok(format!("[\"{r}\",\"{s}\"]"))
    }

    pub fn auth(&self, now_ms: u64) -> Result<AuthSignature, String> {
        let timestamp = now_ms / 1000;
        let expiration = timestamp
            .checked_add(86400)
            .ok_or("Invalid Paradex auth timestamp")?;
        let signature = self.signature(
            "Request",
            &["method", "path", "body", "timestamp", "expiration"],
            &[
                cairo_short_string_to_felt("POST").expect("short string"),
                cairo_short_string_to_felt("/v1/auth").expect("short string"),
                Felt::ZERO,
                Felt::from(timestamp),
                Felt::from(expiration),
            ],
        )?;
        Ok(AuthSignature {
            signature,
            timestamp,
            expiration,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn order(
        &self,
        now_ms: u64,
        market: &str,
        side: &str,
        order_type: &str,
        size: &str,
        price: Option<&str>,
    ) -> Result<OrderSignature, String> {
        self.sign_order(now_ms, market, side, order_type, size, price, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn modify(
        &self,
        now_ms: u64,
        market: &str,
        side: &str,
        order_type: &str,
        size: &str,
        price: &str,
        id: &str,
    ) -> Result<OrderSignature, String> {
        if id.is_empty() {
            return Err("Cannot sign a Paradex amendment without an order ID".into());
        }
        self.sign_order(
            now_ms,
            market,
            side,
            order_type,
            size,
            Some(price),
            Some(id),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn sign_order(
        &self,
        now_ms: u64,
        market: &str,
        side: &str,
        order_type: &str,
        size: &str,
        price: Option<&str>,
        id: Option<&str>,
    ) -> Result<OrderSignature, String> {
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
        let mut names = vec!["timestamp", "market", "side", "orderType", "size", "price"];
        let mut values = vec![
            Felt::from(now_ms),
            word(market)?,
            side,
            word(order_type)?,
            felt(&size)?,
            felt(&price)?,
        ];
        if let Some(id) = id {
            names.push("id");
            values.push(word(id)?);
        }
        let signature = self.signature(
            if id.is_some() { "ModifyOrder" } else { "Order" },
            &names,
            &values,
        )?;
        Ok(OrderSignature {
            signature,
            signature_timestamp: now_ms,
        })
    }
}

fn word(input: &str) -> Result<Felt, String> {
    if input.is_empty() {
        return Ok(Felt::ZERO);
    }
    if input.starts_with("0x")
        || input.starts_with("0X")
        || input.bytes().all(|b| b.is_ascii_digit())
    {
        felt(input)
    } else {
        cairo_short_string_to_felt(input).map_err(|_| "Invalid Paradex signing field".into())
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

fn derive_ethereum(key: &str, config: &SystemConfig) -> Result<(Felt, Felt, String), String> {
    #[derive(Serialize)]
    struct Constant {
        action: &'static str,
    }
    #[derive(Serialize)]
    struct Ethereum<'a> {
        domain: Domain<'a>,
        types: BTreeMap<&'static str, Vec<Field>>,
        #[serde(rename = "primaryType")]
        primary_type: &'static str,
        message: Constant,
    }
    let chain = config
        .l1_chain_id
        .as_deref()
        .ok_or("Missing Paradex Ethereum chain ID")?;
    let raw = serde_json::to_string(&Ethereum {
        domain: Domain {
            name: "Paradex",
            chain_id: chain,
            version: "1",
        },
        types: [(
            "Constant",
            vec![Field {
                name: "action",
                kind: "string",
            }],
        )]
        .into(),
        primary_type: "Constant",
        message: Constant {
            action: "STARK Key",
        },
    })
    .map_err(|_| "Paradex key derivation failed")?;
    let seed = eip712::sign_typed_data(&raw, key).map_err(|_| "Invalid Paradex Ethereum key")?;
    let r = seed
        .strip_prefix("0x")
        .unwrap_or(&seed)
        .get(..64)
        .ok_or("Invalid Paradex Ethereum signature")?;
    let mut r_bytes = [0u8; 32];
    for (byte, pair) in r_bytes.iter_mut().zip(r.as_bytes().as_chunks::<2>().0) {
        *byte = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| "Invalid Paradex Ethereum signature")?,
            16,
        )
        .map_err(|_| "Invalid Paradex Ethereum signature")?;
    }
    let order = U256::from_str_radix(CURVE_ORDER, 10).expect("curve order");
    // 2^256 - (2^256 mod order), represented without overflowing U256.
    let limit = U256::MAX - (U256::MAX % order);
    let private = (0u32..)
        .find_map(|index| {
            let index_bytes = index.to_be_bytes();
            let index_start = index_bytes.iter().position(|b| *b != 0).unwrap_or(3);
            let mut material = r_bytes.to_vec();
            material.extend_from_slice(&index_bytes[index_start..]);
            let digest = U256::from_be_bytes(sha256(&material));
            (digest < limit).then_some(digest % order)
        })
        .ok_or("Paradex key derivation failed")?;
    let private = secret(&private.to_string())?;
    let public = starknet::public_key(&private);
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
        get_selector_from_name("initialize").expect("ASCII selector"),
        Felt::TWO,
        public,
        Felt::ZERO,
    ];
    let address = get_contract_address(public, proxy, &calldata, Felt::ZERO);
    Ok((private, address, format!("{address:#x}")))
}
