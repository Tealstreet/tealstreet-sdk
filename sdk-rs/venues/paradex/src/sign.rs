use alloy_primitives::U256;
use safe_cex_crypto::{eip712, ethereum, sha256, starknet};
use serde::{Deserialize, Serialize};
use starknet_crypto::{Felt, pedersen_hash};

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
    key: Option<zeroize::Zeroizing<[u8; 32]>>,
    public_key: Option<String>,
    address_felt: Felt,
    address: String,
    subkey: String,
    chain_id: Felt,
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

fn chain_id(input: &str) -> Result<Felt, String> {
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
            key: Some(zeroize::Zeroizing::new(key.to_bytes_be())),
            public_key: None,
            address_felt,
            address,
            subkey: subkey.into(),
            chain_id,
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn delegated(
        key: &str,
        address: &str,
        subkey: &str,
        config: &SystemConfig,
    ) -> Result<Self, String> {
        let address_felt = felt(address).map_err(|_| "Invalid Paradex account address")?;
        if key.is_empty()
            || address_felt == Felt::ZERO
            || felt(subkey).map_err(|_| "Invalid Paradex subkey address")? == Felt::ZERO
        {
            return Err("Invalid Paradex account or subkey address".into());
        }
        Ok(Self {
            key: None,
            public_key: Some(key.into()),
            address_felt,
            address: address.to_lowercase(),
            subkey: subkey.into(),
            chain_id: chain_id(&config.starknet_chain_id)?,
        })
    }
    pub fn payload_request(&self, hash: &str) -> Option<safe_cex_engine::PayloadSignRequest> {
        Some(safe_cex_engine::PayloadSignRequest {
            exchange_name: "paradex",
            key: self.public_key.as_ref()?.clone(),
            payload: hash.into(),
            scheme: "stark",
            encoding: "felt-pair",
        })
    }
    pub fn is_delegated(&self) -> bool {
        self.public_key.is_some()
    }
    pub fn subkey(&self) -> &str {
        &self.subkey
    }

    fn signature(&self, definition: &str, values: &[Felt]) -> Result<String, String> {
        let hash = typed_hash(self.chain_id, self.address_felt, definition, values)?;
        let Some(key) = self.key.as_ref() else {
            return Ok(format!("{hash:#x}"));
        };
        let (r, s) = starknet::sign(&hash, &Felt::from_bytes_be(key))
            .map_err(|_| "Paradex signing failed")?;
        Ok(format!(
            "[\"{}\",\"{}\"]",
            U256::from_be_bytes(r.to_bytes_be()),
            U256::from_be_bytes(s.to_bytes_be())
        ))
    }

    pub fn auth(&self, now_ms: u64) -> Result<AuthSignature, String> {
        let timestamp = now_ms / 1000;
        let expiration = timestamp
            .checked_add(300)
            .ok_or("Invalid Paradex auth timestamp")?;
        let signature = self.signature(
            "Request(method:felt,path:felt,body:felt,timestamp:felt,expiration:felt)",
            &[
                short_string("POST").expect("short string"),
                short_string("/v1/auth").expect("short string"),
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
        let (definition, values) =
            order_message(now_ms, market, side, order_type, size, price, id)?;
        let signature = self.signature(definition, &values)?;
        Ok(OrderSignature {
            signature,
            signature_timestamp: now_ms,
        })
    }
}

/// StarkNet typed-data message hash over the Paradex domain, account and struct.
fn typed_hash(
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

/// The `Order` / `ModifyOrder` struct the local signer and `paradex.order.1` share.
#[allow(clippy::too_many_arguments)]
fn order_message(
    now_ms: u64,
    market: &str,
    side: &str,
    order_type: &str,
    size: &str,
    price: Option<&str>,
    id: Option<&str>,
) -> Result<(&'static str, Vec<Felt>), String> {
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
    let mut values = vec![
        Felt::from(now_ms),
        word(market)?,
        side,
        word(order_type)?,
        felt(&size)?,
        felt(&price)?,
    ];
    Ok(match id {
        Some(id) => {
            values.push(word(id)?);
            (
                "ModifyOrder(timestamp:felt,market:felt,side:felt,orderType:felt,size:felt,price:felt,id:felt)",
                values,
            )
        }
        None => (
            "Order(timestamp:felt,market:felt,side:felt,orderType:felt,size:felt,price:felt)",
            values,
        ),
    })
}

/// Order types the venue places; anything else is refused remotely.
const ORDER_TYPES: &[&str] = &[
    "LIMIT",
    "MARKET",
    "STOP_LIMIT",
    "STOP_MARKET",
    "STOP_LOSS_LIMIT",
    "STOP_LOSS_MARKET",
    "TAKE_PROFIT_LIMIT",
    "TAKE_PROFIT_MARKET",
];

/// `paradex.order.1`: Stark signature over the `Order`/`ModifyOrder` typed-data
/// hash, carried as the 32-byte big-endian digest. Auth requests are not covered.
pub struct OrderRecipe;
impl safe_cex_engine::signing_remote::RemoteRecipe for OrderRecipe {
    fn id(&self) -> &'static str {
        "paradex.order.1"
    }
    fn algorithm(&self) -> safe_cex_engine::signing::Algorithm {
        safe_cex_engine::signing::Algorithm::StarkDigest
    }
    fn role(&self) -> Option<safe_cex_engine::signing::ExtraKey> {
        None
    }
    fn canonicalize(
        &self,
        inputs: &serde_json::Value,
    ) -> Result<Vec<u8>, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError::Refused;
        let object = inputs.as_object().ok_or(Refused)?;
        const FIELDS: &[&str] = &[
            "chainId",
            "account",
            "timestamp",
            "market",
            "side",
            "orderType",
            "size",
            "price",
            "id",
        ];
        if object.keys().any(|key| !FIELDS.contains(&key.as_str())) {
            return Err(Refused);
        }
        let text = |name: &str| match object.get(name) {
            None => Ok(None),
            Some(value) => value.as_str().map(Some).ok_or(Refused),
        };
        let required = |name: &str| text(name)?.filter(|v| !v.is_empty()).ok_or(Refused);
        let timestamp = required("timestamp")?;
        if timestamp.len() > 16 || !timestamp.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Refused);
        }
        let order_type = required("orderType")?;
        if !ORDER_TYPES.contains(&order_type) {
            return Err(Refused);
        }
        let market = required("market")?;
        if !market
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Refused);
        }
        let account = felt(required("account")?).map_err(|_| Refused)?;
        if account == Felt::ZERO {
            return Err(Refused);
        }
        let (definition, values) = order_message(
            timestamp.parse().map_err(|_| Refused)?,
            market,
            required("side")?,
            order_type,
            required("size")?,
            text("price")?,
            text("id")?,
        )
        .map_err(|_| Refused)?;
        let hash = typed_hash(
            chain_id(required("chainId")?).map_err(|_| Refused)?,
            account,
            definition,
            &values,
        )
        .map_err(|_| Refused)?;
        Ok(hash.to_bytes_be().to_vec())
    }
}
pub static ORDER: OrderRecipe = OrderRecipe;
pub static REMOTE_RECIPES: &[&dyn safe_cex_engine::signing_remote::RemoteRecipe] = &[&ORDER];

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

fn ethereum_digest(chain: &str) -> Result<[u8; 32], String> {
    let chain = unsigned(chain).map_err(|_| "Invalid Paradex Ethereum chain ID")?;
    // EIP712Domain(string name,string version,uint256 chainId), with no
    // verifyingContract field. Each string occupies its hashed 32-byte word.
    let mut domain = [0u8; 128];
    domain[..32].copy_from_slice(&ethereum::keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId)",
    ));
    domain[32..64].copy_from_slice(&ethereum::keccak256(b"Paradex"));
    domain[64..96].copy_from_slice(&ethereum::keccak256(b"1"));
    domain[96..].copy_from_slice(&chain.to_be_bytes::<32>());
    let mut constant = [0u8; 64];
    constant[..32].copy_from_slice(&ethereum::keccak256(b"Constant(string action)"));
    constant[32..].copy_from_slice(&ethereum::keccak256(b"STARK Key"));
    Ok(eip712::hash_prehashed(
        ethereum::keccak256(&domain),
        ethereum::keccak256(&constant),
    ))
}

fn derive_ethereum(key: &str, config: &SystemConfig) -> Result<(Felt, Felt, String), String> {
    let chain = config
        .l1_chain_id
        .as_deref()
        .ok_or("Missing Paradex Ethereum chain ID")?;
    let digest = ethereum_digest(chain)?;
    let seed = ethereum::sign_digest(&digest, key).map_err(|_| "Invalid Paradex Ethereum key")?;
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
        selector("initialize").expect("ASCII selector"),
        Felt::TWO,
        public,
        Felt::ZERO,
    ];
    let address = account_address(public, proxy, &calldata);
    Ok((private, address, format!("{address:#x}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe_config() -> SystemConfig {
        SystemConfig {
            starknet_chain_id: "PRIVATE_SN_POTC_SEPOLIA".into(),
            l1_chain_id: None,
            paraclear_account_hash: None,
            paraclear_account_proxy_hash: None,
        }
    }
    const ACCOUNT: &str = "0x0129f5ed7e7e3b5a8c9a3e2f1d0c4b6a7e8f9d0c1b2a39485766554433221100";
    fn recipe_inputs(id: Option<&str>) -> serde_json::Value {
        let mut inputs = serde_json::json!({
            "chainId": "PRIVATE_SN_POTC_SEPOLIA", "account": ACCOUNT, "timestamp": "1700000000123",
            "market": "BTC-USD-PERP", "side": "BUY", "orderType": "LIMIT",
            "size": "0.25", "price": "64000.5"
        });
        if let Some(id) = id {
            inputs["id"] = id.into();
        }
        inputs
    }

    #[test]
    fn order_recipe_digest_is_the_local_signers_typed_hash() {
        use safe_cex_engine::signing_remote::RemoteRecipe;
        // Delegated credentials hold no key and return the bare typed hash.
        let hashed = Credentials::delegated("0x1", ACCOUNT, "0x2", &recipe_config()).unwrap();
        for id in [None, Some("1700000000000201")] {
            let expected = match id {
                None => hashed.order(
                    1700000000123,
                    "BTC-USD-PERP",
                    "BUY",
                    "LIMIT",
                    "0.25",
                    Some("64000.5"),
                ),
                Some(id) => hashed.modify(
                    1700000000123,
                    "BTC-USD-PERP",
                    "BUY",
                    "LIMIT",
                    "0.25",
                    "64000.5",
                    id,
                ),
            }
            .unwrap()
            .signature;
            let digest = ORDER.canonicalize(&recipe_inputs(id)).unwrap();
            assert_eq!(digest.len(), 32);
            assert_eq!(
                format!("{:#x}", Felt::from_bytes_be_slice(&digest)),
                expected
            );
        }
        // Signing that digest gives the venue's own signature bytes.
        let key = format!("0x{}5", "0".repeat(63));
        let signer = Credentials::new(&key, ACCOUNT, "0x2", &recipe_config()).unwrap();
        let signed = signer
            .order(
                1700000000123,
                "BTC-USD-PERP",
                "BUY",
                "LIMIT",
                "0.25",
                Some("64000.5"),
            )
            .unwrap()
            .signature;
        let digest = ORDER.canonicalize(&recipe_inputs(None)).unwrap();
        let (r, s) =
            starknet::sign(&Felt::from_bytes_be_slice(&digest), &felt(&key).unwrap()).unwrap();
        assert_eq!(
            signed,
            format!(
                "[\"{}\",\"{}\"]",
                U256::from_be_bytes(r.to_bytes_be()),
                U256::from_be_bytes(s.to_bytes_be())
            )
        );
    }

    #[test]
    fn order_recipe_refuses_inputs_outside_its_allowlist() {
        use safe_cex_engine::signing::SignError::Refused;
        use safe_cex_engine::signing_remote::RemoteRecipe;
        for (field, value) in [
            ("orderType", serde_json::json!("TWAP")),
            ("side", serde_json::json!("HOLD")),
            ("market", serde_json::json!("BTC/USD")),
            ("timestamp", serde_json::json!("17e11")),
            ("account", serde_json::json!("0x0")),
            ("size", serde_json::json!("lots")),
            ("chainId", serde_json::json!("")),
            ("price", serde_json::json!(1)),
            ("subject", serde_json::json!("0x3")),
        ] {
            let mut inputs = recipe_inputs(None);
            inputs[field] = value;
            assert_eq!(ORDER.canonicalize(&inputs), Err(Refused), "{field}");
        }
        assert_eq!(REMOTE_RECIPES[0].id(), "paradex.order.1");
    }

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
            let digest = ethereum_digest(&row.chain).unwrap();
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
            let error = ethereum_digest(chain).unwrap_err();
            assert_eq!(error, "Invalid Paradex Ethereum chain ID");
        }
    }
}
