//! GRVT's fixed EIP-712 order schema; integer units never pass through floats.
use safe_cex_crypto::{eip712, ethereum};
use serde::Serialize;

const LEG_TYPE: &[u8] =
    b"OrderLeg(uint256 assetID,uint64 contractSize,uint64 limitPrice,bool isBuyingContract)";
const ORDER_TYPE: &[u8] = b"Order(uint64 subAccountID,bool isMarket,uint8 timeInForce,bool postOnly,bool reduceOnly,OrderLeg[] legs,uint32 nonce,int64 expiration)OrderLeg(uint256 assetID,uint64 contractSize,uint64 limitPrice,bool isBuyingContract)";
const BUILDER_ORDER_TYPE: &[u8] = b"OrderWithBuilderFee(uint64 subAccountID,bool isMarket,uint8 timeInForce,bool postOnly,bool reduceOnly,OrderLeg[] legs,address builder,uint32 builderFee,uint32 nonce,int64 expiration)OrderLeg(uint256 assetID,uint64 contractSize,uint64 limitPrice,bool isBuyingContract)";

#[derive(Clone, Debug)]
pub struct OrderLeg {
    pub asset_id: [u8; 32],
    pub contract_size: u64,
    pub limit_price: u64,
    pub is_buying: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct BuilderFee {
    pub address: [u8; 20],
    pub fee: u32,
}

pub struct Order<'a> {
    pub sub_account_id: u64,
    pub is_market: bool,
    pub time_in_force: u8,
    pub post_only: bool,
    pub reduce_only: bool,
    pub legs: &'a [OrderLeg],
    pub builder: Option<BuilderFee>,
    pub nonce: u32,
    pub expiration: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Signature {
    pub r: String,
    pub s: String,
    pub v: u8,
}

pub fn signer_address(private_key: &str) -> Result<String, String> {
    ethereum::private_key_address(private_key)
}

pub fn expiration(now_ms: u64) -> Result<i64, String> {
    now_ms
        .checked_add(3_600_000)
        .and_then(|value| value.checked_mul(1_000_000))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| "GRVT order expiration exceeds int64".into())
}

/// Exact decimal-to-native conversion, including scientific notation from f64 formatting.
pub fn scaled_u64(value: &str, decimals: i32) -> Result<u64, String> {
    let invalid = || "Invalid GRVT unsigned decimal".to_owned();
    if value.is_empty() || !(-255..=255).contains(&decimals) {
        return Err(invalid());
    }
    let (mantissa, exponent) = match value.find(['e', 'E']) {
        Some(index) => (
            &value[..index],
            value[index + 1..].parse::<i32>().map_err(|_| invalid())?,
        ),
        None => (value, 0),
    };
    let mut digits = Vec::with_capacity(mantissa.len());
    let mut fractional = None;
    for byte in mantissa.bytes() {
        if byte == b'.' && fractional.is_none() {
            fractional = Some(0i64);
        } else if byte.is_ascii_digit() {
            digits.push(byte - b'0');
            if let Some(count) = fractional.as_mut() {
                *count += 1;
            }
        } else {
            return Err(invalid());
        }
    }
    if digits.is_empty() {
        return Err(invalid());
    }
    let mut shift = i64::from(decimals) + i64::from(exponent) - fractional.unwrap_or(0);
    while shift < 0 && digits.last() == Some(&0) {
        digits.pop();
        shift += 1;
    }
    if digits.iter().all(|digit| *digit == 0) {
        return Ok(0);
    }
    if shift < 0 {
        return Err("GRVT decimal exceeds native precision".into());
    }
    if shift > 19 {
        return Err("GRVT decimal exceeds uint64".into());
    }
    let mut integer = 0u64;
    for digit in digits {
        integer = integer
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(digit)))
            .ok_or_else(|| "GRVT decimal exceeds uint64".to_owned())?;
    }
    for _ in 0..shift {
        integer = integer
            .checked_mul(10)
            .ok_or_else(|| "GRVT decimal exceeds uint64".to_owned())?;
    }
    Ok(integer)
}

/// REST percentages use four decimal places in the signed uint32 fee.
pub fn builder_fee(percent: &str) -> Result<u32, String> {
    u32::try_from(scaled_u64(percent, 4)?).map_err(|_| "GRVT builder fee exceeds uint32".into())
}

pub fn parse_address(value: &str) -> Result<[u8; 20], String> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 40 || !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid GRVT builder address".into());
    }
    let checked = ethereum::get_address(value)?;
    let mut address = [0u8; 20];
    for (slot, pair) in address
        .iter_mut()
        .zip(checked.as_bytes()[2..].as_chunks::<2>().0)
    {
        *slot = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(address)
}

pub fn parse_asset_id(value: &str) -> Result<[u8; 32], String> {
    let mut word = [0u8; 32];
    if let Some(hex) = value.strip_prefix("0x") {
        if hex.is_empty() || hex.len() > 64 {
            return Err("Invalid GRVT uint256 asset ID".into());
        }
        for (index, byte) in hex.bytes().rev().enumerate() {
            word[31 - index / 2] |= hex_digit(byte)? << ((index % 2) * 4);
        }
    } else {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("Invalid GRVT uint256 asset ID".into());
        }
        for digit in value.bytes() {
            let mut carry = u16::from(digit - b'0');
            for byte in word.iter_mut().rev() {
                let product = u16::from(*byte) * 10 + carry;
                *byte = product as u8;
                carry = product >> 8;
            }
            if carry != 0 {
                return Err("GRVT asset ID exceeds uint256".into());
            }
        }
    }
    Ok(word)
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("Invalid GRVT hexadecimal integer".into()),
    }
}

fn word(integer: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&integer.to_be_bytes());
    bytes
}

fn append(words: &mut Vec<u8>, integer: u64) {
    words.extend_from_slice(&word(integer));
}

pub(crate) fn digest(order: &Order<'_>, chain_id: u64) -> Result<[u8; 32], String> {
    if order.legs.is_empty()
        || order.legs.iter().any(|leg| leg.contract_size == 0)
        || !(1..=4).contains(&order.time_in_force)
        || order.expiration <= 0
    {
        return Err("Invalid GRVT signed order fields".into());
    }
    let mut domain = Vec::with_capacity(128);
    domain.extend_from_slice(&ethereum::keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId)",
    ));
    domain.extend_from_slice(&ethereum::keccak256(b"GRVT Exchange"));
    domain.extend_from_slice(&ethereum::keccak256(b"0"));
    append(&mut domain, chain_id);

    let mut leg_hashes = Vec::with_capacity(order.legs.len() * 32);
    for leg in order.legs {
        let mut encoded = Vec::with_capacity(160);
        encoded.extend_from_slice(&ethereum::keccak256(LEG_TYPE));
        encoded.extend_from_slice(&leg.asset_id);
        append(&mut encoded, leg.contract_size);
        append(&mut encoded, leg.limit_price);
        append(&mut encoded, u64::from(leg.is_buying));
        leg_hashes.extend_from_slice(&ethereum::keccak256(&encoded));
    }
    let mut encoded = Vec::with_capacity(352);
    encoded.extend_from_slice(&ethereum::keccak256(if order.builder.is_some() {
        BUILDER_ORDER_TYPE
    } else {
        ORDER_TYPE
    }));
    append(&mut encoded, order.sub_account_id);
    append(&mut encoded, u64::from(order.is_market));
    append(&mut encoded, u64::from(order.time_in_force));
    append(&mut encoded, u64::from(order.post_only));
    append(&mut encoded, u64::from(order.reduce_only));
    encoded.extend_from_slice(&ethereum::keccak256(&leg_hashes));
    if let Some(builder) = order.builder {
        let mut address = [0u8; 32];
        address[12..].copy_from_slice(&builder.address);
        encoded.extend_from_slice(&address);
        append(&mut encoded, u64::from(builder.fee));
    }
    append(&mut encoded, u64::from(order.nonce));
    append(&mut encoded, order.expiration as u64);
    Ok(eip712::hash_prehashed(
        ethereum::keccak256(&domain),
        ethereum::keccak256(&encoded),
    ))
}

pub fn sign_order(
    order: &Order<'_>,
    private_key: &str,
    testnet: bool,
) -> Result<Signature, String> {
    sign_digest(
        &digest(order, if testnet { 326 } else { 325 })?,
        private_key,
    )
}

fn sign_digest(digest: &[u8; 32], private_key: &str) -> Result<Signature, String> {
    let signature = ethereum::sign_digest(digest, private_key)?;
    let raw = signature.strip_prefix("0x").unwrap_or(&signature);
    if raw.len() != 130 {
        return Err("Invalid GRVT Ethereum signature length".into());
    }
    Ok(Signature {
        r: format!("0x{}", &raw[..64]),
        s: format!("0x{}", &raw[64..128]),
        v: u8::from_str_radix(&raw[128..], 16)
            .map_err(|_| "Invalid GRVT Ethereum signature recovery ID".to_owned())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::TypedData as DynamicTypedData;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;

    fn leg() -> OrderLeg {
        OrderLeg {
            asset_id: parse_asset_id("79228162514264337593543950337").unwrap(),
            contract_size: 1_000_000_000,
            limit_price: 50_000_000_000,
            is_buying: true,
        }
    }

    fn order(legs: &[OrderLeg]) -> Order<'_> {
        Order {
            sub_account_id: 1_234_567_890,
            is_market: false,
            time_in_force: 1,
            post_only: false,
            reduce_only: false,
            legs,
            builder: None,
            nonce: 42,
            expiration: 1_700_000_000_000_000_000,
        }
    }

    fn raw_signature(signature: &Signature) -> String {
        format!("{}{}{:02x}", signature.r, &signature.s[2..], signature.v)
    }

    #[test]
    fn vendored_sdk_deterministic_order_and_builder_vectors() {
        // thirdparty/grvt-ts-sdk/tests/eip712-signing.test.ts uses default chain 1.
        let key = format!("0x{}", "01".repeat(32));
        let legs = [leg()];
        let mut order = order(&legs);
        assert_eq!(
            raw_signature(&sign_digest(&digest(&order, 1).unwrap(), &key).unwrap()),
            "0x0a721403575ee7bf1d3497c51d524de10a835b1f8a50c8db02093c6ee74aa5033f97459f629909a3d82e97ee0b653c08871481a71a0d8ecaf9878d2faf9e31e51c"
        );
        order.builder = Some(BuilderFee {
            address: parse_address("0x000000000000000000000000000000000000beef").unwrap(),
            fee: 100,
        });
        assert_eq!(
            raw_signature(&sign_digest(&digest(&order, 1).unwrap(), &key).unwrap()),
            "0x41162e249c152d579c97624668b29ae2280bb0309134436a39becef529f2a0ae211a649c18ef9d419df6a32a93f2d1852f81f60798150bc818fc39527586e1c21b"
        );
    }

    #[derive(Serialize)]
    struct TypedData<'a> {
        domain: Domain,
        types: Types,
        #[serde(rename = "primaryType")]
        primary_type: &'a str,
        message: Message,
    }
    #[derive(Serialize)]
    struct Domain {
        name: &'static str,
        version: &'static str,
        #[serde(rename = "chainId")]
        chain_id: u64,
    }
    #[derive(Serialize)]
    struct Field {
        name: &'static str,
        #[serde(rename = "type")]
        kind: &'static str,
    }
    #[derive(Serialize)]
    struct Types {
        #[serde(rename = "Order", skip_serializing_if = "Option::is_none")]
        order: Option<Vec<Field>>,
        #[serde(
            rename = "OrderWithBuilderFee",
            skip_serializing_if = "Option::is_none"
        )]
        builder_order: Option<Vec<Field>>,
        #[serde(rename = "OrderLeg")]
        leg: Vec<Field>,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Message {
        #[serde(rename = "subAccountID")]
        sub_account_id: String,
        is_market: bool,
        time_in_force: u8,
        post_only: bool,
        reduce_only: bool,
        legs: Vec<MessageLeg>,
        #[serde(skip_serializing_if = "Option::is_none")]
        builder: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        builder_fee: Option<u32>,
        nonce: u32,
        expiration: String,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct MessageLeg {
        #[serde(rename = "assetID")]
        asset_id: String,
        contract_size: String,
        limit_price: String,
        is_buying_contract: bool,
    }

    fn typed(order: &Order<'_>, chain_id: u64) -> String {
        let fields = |items: &[(&'static str, &'static str)]| -> Vec<Field> {
            items
                .iter()
                .map(|&(name, kind)| Field { name, kind })
                .collect()
        };
        let mut order_fields: Vec<Field> = fields(&[
            ("subAccountID", "uint64"),
            ("isMarket", "bool"),
            ("timeInForce", "uint8"),
            ("postOnly", "bool"),
            ("reduceOnly", "bool"),
            ("legs", "OrderLeg[]"),
        ]);
        if order.builder.is_some() {
            order_fields.extend(fields(&[("builder", "address"), ("builderFee", "uint32")]));
        }
        order_fields.extend(fields(&[("nonce", "uint32"), ("expiration", "int64")]));
        let is_builder = order.builder.is_some();
        serde_json::to_string(&TypedData {
            domain: Domain {
                name: "GRVT Exchange",
                version: "0",
                chain_id,
            },
            types: Types {
                order: (!is_builder).then(|| {
                    order_fields
                        .iter()
                        .map(|f| Field {
                            name: f.name,
                            kind: f.kind,
                        })
                        .collect()
                }),
                builder_order: is_builder.then_some(order_fields),
                leg: fields(&[
                    ("assetID", "uint256"),
                    ("contractSize", "uint64"),
                    ("limitPrice", "uint64"),
                    ("isBuyingContract", "bool"),
                ]),
            },
            primary_type: if is_builder {
                "OrderWithBuilderFee"
            } else {
                "Order"
            },
            message: Message {
                sub_account_id: order.sub_account_id.to_string(),
                is_market: order.is_market,
                time_in_force: order.time_in_force,
                post_only: order.post_only,
                reduce_only: order.reduce_only,
                legs: order
                    .legs
                    .iter()
                    .map(|l| MessageLeg {
                        asset_id: format!("0x{}", safe_cex_crypto::hex(&l.asset_id)),
                        contract_size: l.contract_size.to_string(),
                        limit_price: l.limit_price.to_string(),
                        is_buying_contract: l.is_buying,
                    })
                    .collect(),
                builder: order
                    .builder
                    .map(|b| format!("0x{}", safe_cex_crypto::hex(&b.address))),
                builder_fee: order.builder.map(|b| b.fee),
                nonce: order.nonce,
                expiration: order.expiration.to_string(),
            },
        })
        .unwrap()
    }

    #[test]
    fn exact_words_match_dynamic_eip712_with_uint_bounds_and_both_domains() {
        let key = format!("0x{}", "01".repeat(32));
        let signer: PrivateKeySigner = key.parse().unwrap();
        let legs = [
            leg(),
            OrderLeg {
                asset_id: [255; 32],
                contract_size: u64::MAX,
                limit_price: u64::MAX,
                is_buying: false,
            },
        ];
        for builder in [
            None,
            Some(BuilderFee {
                address: [0xab; 20],
                fee: u32::MAX,
            }),
        ] {
            let mut order = order(&legs);
            order.sub_account_id = u64::MAX;
            order.expiration = i64::MAX;
            order.nonce = u32::MAX;
            order.builder = builder;
            for testnet in [false, true] {
                let chain = if testnet { 326 } else { 325 };
                let typed: DynamicTypedData = serde_json::from_str(&typed(&order, chain)).unwrap();
                assert_eq!(
                    format!(
                        "0x{}",
                        safe_cex_crypto::hex(&digest(&order, chain).unwrap())
                    ),
                    format!("{:#x}", typed.eip712_signing_hash().unwrap())
                );
                assert_eq!(
                    raw_signature(&sign_order(&order, &key, testnet).unwrap()),
                    signer
                        .sign_dynamic_typed_data_sync(&typed)
                        .unwrap()
                        .to_string()
                );
            }
            assert_ne!(digest(&order, 325).unwrap(), digest(&order, 326).unwrap());
        }
    }

    #[test]
    fn every_order_field_and_leg_order_changes_the_signed_digest() {
        let legs = [leg()];
        let base = digest(&order(&legs), 325).unwrap();
        for field in 0..13 {
            let mut changed_legs = legs.clone();
            if field == 8 {
                changed_legs[0].asset_id[0] ^= 1;
            }
            if field == 9 {
                changed_legs[0].contract_size += 1;
            }
            if field == 10 {
                changed_legs[0].limit_price += 1;
            }
            if field == 11 {
                changed_legs[0].is_buying = false;
            }
            let mut changed = order(&changed_legs);
            match field {
                0 => changed.sub_account_id += 1,
                1 => changed.is_market = true,
                2 => changed.time_in_force = 3,
                3 => changed.post_only = true,
                4 => changed.reduce_only = true,
                5 => changed.nonce += 1,
                6 => changed.expiration += 1,
                7 => {
                    changed.builder = Some(BuilderFee {
                        address: [1; 20],
                        fee: 300,
                    })
                }
                12 => changed.legs = &[],
                _ => {}
            }
            if field == 12 {
                assert!(digest(&changed, 325).is_err());
            } else {
                assert_ne!(digest(&changed, 325).unwrap(), base, "field {field}");
            }
        }
        let mut second = leg();
        second.is_buying = false;
        let forward = [leg(), second.clone()];
        let reverse = [second, leg()];
        assert_ne!(
            digest(&order(&forward), 325).unwrap(),
            digest(&order(&reverse), 325).unwrap()
        );
        let mut builder_order = order(&legs);
        builder_order.builder = Some(BuilderFee {
            address: [1; 20],
            fee: 300,
        });
        let base = digest(&builder_order, 325).unwrap();
        builder_order.builder.as_mut().unwrap().fee += 1;
        assert_ne!(digest(&builder_order, 325).unwrap(), base);
        builder_order.builder.as_mut().unwrap().fee -= 1;
        builder_order.builder.as_mut().unwrap().address[0] ^= 1;
        assert_ne!(digest(&builder_order, 325).unwrap(), base);
    }

    #[test]
    fn decimal_conversion_preserves_native_units_and_refuses_rounding() {
        for (value, decimals, expected) in [
            ("0.03", 9, 30_000_000),
            ("1000", -3, 1),
            ("1e3", -3, 1),
            ("1e-9", 9, 1),
            ("1.2300", 2, 123),
            ("9007199254740993", 0, 9_007_199_254_740_993),
            ("18446744073.709551615", 9, u64::MAX),
            ("18446744073709551615", 0, u64::MAX),
            (".1", 1, 1),
            ("0e-999", 0, 0),
            ("12.3e2", 0, 1230),
        ] {
            assert_eq!(scaled_u64(value, decimals).unwrap(), expected, "{value}");
        }
        for value in [
            "",
            "-1",
            "+1",
            "NaN",
            "inf",
            " 1",
            "1 ",
            "1.2.3",
            ".",
            "1e",
            "1e1e1",
            "0.0000000001",
            "18446744073709551616",
            "18446744073.709551616",
            "1e1000",
        ] {
            assert!(scaled_u64(value, 9).is_err(), "{value}");
        }
        assert_eq!(builder_fee("0.03").unwrap(), 300);
        assert_eq!(builder_fee("429496.7295").unwrap(), u32::MAX);
        assert!(builder_fee("429496.7296").is_err());
        assert!(builder_fee("0.00001").is_err());
        assert_eq!(
            expiration(1_700_000_000_000).unwrap(),
            1_700_003_600_000_000_000
        );
        assert!(expiration(u64::MAX).is_err());
    }

    #[test]
    fn uint256_address_and_key_boundaries() {
        assert_eq!(parse_asset_id("0xf").unwrap()[31], 15);
        assert_eq!(
            parse_asset_id(&format!("0x{}", "f".repeat(64))).unwrap(),
            [255; 32]
        );
        assert_eq!(
            parse_asset_id(
                "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            )
            .unwrap(),
            [255; 32]
        );
        for invalid in [
            "",
            "0x",
            "0xz",
            "-1",
            "1.0",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ] {
            assert!(parse_asset_id(invalid).is_err(), "{invalid}");
        }
        assert!(parse_asset_id(&format!("0x1{}", "0".repeat(64))).is_err());
        assert!(parse_address("0xabc").is_err());
        assert!(parse_address("0x7e5f4552091a69125d5DfCb7b8C2659029395Bdf").is_err());
        let key = format!("0x{}", "01".repeat(32));
        assert_eq!(
            signer_address(&key).unwrap(),
            "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
        );
        assert!(signer_address("invalid").is_err());
        let legs = [leg()];
        let mut order = order(&legs);
        assert!(sign_order(&order, "invalid", false).is_err());
        order.time_in_force = 0;
        assert!(sign_order(&order, &key, false).is_err());
        order.time_in_force = 1;
        order.expiration = -1;
        assert!(sign_order(&order, &key, false).is_err());
        order.expiration = 1;
        order.legs = &[];
        assert!(sign_order(&order, &key, false).is_err());
    }
}
