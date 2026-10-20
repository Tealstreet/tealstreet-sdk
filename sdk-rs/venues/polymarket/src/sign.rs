//! CLOB V2 wire signing. Contract/schema oracle: official clob-client-v2 v1.1.0
//! (f3e1a05f), src/order-utils/exchangeOrderBuilderV2.ts and src/signing/:
//! <https://github.com/Polymarket/clob-client-v2/tree/v1.1.0/src>.
//! No transport, clocks, random salt generation, or JSON reconstruction occurs here.
use alloy_primitives::U256;
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::{
    Encoding,
    eip712::hash_prehashed,
    ethereum::{get_address, keccak256, private_key_address, sign_digest},
    hex, hmac_sha256,
};
use safe_cex_engine::Method;
use serde::Deserialize;

const ORDER_TYPE: &str = "Order(uint256 salt,address maker,address signer,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint8 side,uint8 signatureType,uint256 timestamp,bytes32 metadata,bytes32 builder)";
const DOMAIN_TYPE: &str =
    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";
const EXCHANGE_NAME: &str = "Polymarket CTF Exchange";
pub const STANDARD_EXCHANGE: &str = "0xE111180000d2663C0091e4f400237545B87B996B";
pub const NEG_RISK_EXCHANGE: &str = "0xe2222d279d744050d28e00520010520000310F59";

/// Credential carriers may be deserialized from venue responses but never serialized wholesale.
/// ```compile_fail
/// use safe_cex_venue_polymarket::sign::Credentials;
/// fn accidental_log(credentials: &Credentials) {
///     let _ = serde_json::to_string(credentials);
/// }
/// ```
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
}

/// Exact unsigned V2 message. Full-width decimal identifiers never pass through f64.
pub struct OrderSigning {
    pub salt: String,
    pub maker: String,
    pub signer: String,
    pub token_id: String,
    pub maker_amount: String,
    pub taker_amount: String,
    pub side: u8,
    pub signature_type: u8,
    /// Milliseconds, unlike the seconds in L1/L2 authentication.
    pub timestamp: String,
    pub metadata: [u8; 32],
    pub builder: [u8; 32],
}

fn uint(value: &str) -> Result<[u8; 32], String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Polymarket uint256 must be unsigned decimal".into());
    }
    U256::from_str_radix(value, 10)
        .map(|v| v.to_be_bytes())
        .map_err(|_| "Polymarket uint256 overflow".into())
}

fn address(value: &str) -> Result<[u8; 32], String> {
    // CLOB uses Ethereum hex addresses; do not accept the primitive's legacy ICAP path.
    let body = value.strip_prefix("0x").unwrap_or(value);
    if body.len() != 40 || !body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Polymarket requires a 20-byte address".into());
    }
    let checked = get_address(value)?;
    let mut word = [0; 32];
    for (index, byte) in word[12..].iter_mut().enumerate() {
        *byte = u8::from_str_radix(&checked[2 + index * 2..4 + index * 2], 16)
            .map_err(|_| "invalid address")?;
    }
    Ok(word)
}

fn words_hash(words: &[[u8; 32]]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(words.len() * 32);
    for word in words {
        bytes.extend_from_slice(word);
    }
    keccak256(&bytes)
}

fn domain(neg_risk: bool) -> Result<[u8; 32], String> {
    Ok(words_hash(&[
        keccak256(DOMAIN_TYPE.as_bytes()),
        keccak256(EXCHANGE_NAME.as_bytes()),
        keccak256(b"2"),
        uint("137")?,
        address(if neg_risk {
            NEG_RISK_EXCHANGE
        } else {
            STANDARD_EXCHANGE
        })?,
    ]))
}

fn contents(order: &OrderSigning) -> Result<[u8; 32], String> {
    if order.side > 1 || order.signature_type > 3 {
        return Err("invalid Polymarket side/signature type".into());
    }
    Ok(words_hash(&[
        keccak256(ORDER_TYPE.as_bytes()),
        uint(&order.salt)?,
        address(&order.maker)?,
        address(&order.signer)?,
        uint(&order.token_id)?,
        uint(&order.maker_amount)?,
        uint(&order.taker_amount)?,
        uint(&order.side.to_string())?,
        uint(&order.signature_type.to_string())?,
        uint(&order.timestamp)?,
        order.metadata,
        order.builder,
    ]))
}

pub fn order_signer(private_key: &str, maker: &str, signature_type: u8) -> Result<String, String> {
    match signature_type {
        0..=2 => private_key_address(private_key),
        3 => get_address(maker),
        _ => Err("invalid Polymarket signature type".into()),
    }
}

pub fn order_signature(
    order: &OrderSigning,
    private_key: &str,
    neg_risk: bool,
) -> Result<String, String> {
    let expected = order_signer(private_key, &order.maker, order.signature_type)?;
    if address(&order.signer)? != address(&expected)? {
        return Err("Polymarket signer does not match account signature type".into());
    }
    let app_domain = domain(neg_risk)?;
    let content_hash = contents(order)?;
    if order.signature_type != 3 {
        return sign_digest(&hash_prehashed(app_domain, content_hash), private_key);
    }
    // ERC-7739 nested typed data, not an EOA signature over the ordinary Order hash.
    let nested_type = format!(
        "TypedDataSign(Order contents,string name,string version,uint256 chainId,address verifyingContract,bytes32 salt){ORDER_TYPE}"
    );
    let nested = words_hash(&[
        keccak256(nested_type.as_bytes()),
        content_hash,
        keccak256(b"DepositWallet"),
        keccak256(b"1"),
        uint("137")?,
        address(&order.signer)?,
        [0; 32],
    ]);
    let inner = sign_digest(&hash_prehashed(app_domain, nested), private_key)?;
    // inner65 || appDomain32 || contentsHash32 || UTF8 contentsType || uint16_BE len.
    Ok(format!(
        "{inner}{}{}{}{:04x}",
        hex(&app_domain),
        hex(&content_hash),
        hex(ORDER_TYPE.as_bytes()),
        ORDER_TYPE.len()
    ))
}

pub fn l1_signature(private_key: &str, server_seconds: u64, nonce: u64) -> Result<String, String> {
    l1_chain(private_key, server_seconds, nonce, 137)
}

fn l1_chain(private_key: &str, seconds: u64, nonce: u64, chain: u64) -> Result<String, String> {
    let domain = words_hash(&[
        keccak256(b"EIP712Domain(string name,string version,uint256 chainId)"),
        keccak256(b"ClobAuthDomain"),
        keccak256(b"1"),
        uint(&chain.to_string())?,
    ]);
    let message = words_hash(&[
        keccak256(b"ClobAuth(address address,string timestamp,uint256 nonce,string message)"),
        address(&private_key_address(private_key)?)?,
        keccak256(seconds.to_string().as_bytes()),
        uint(&nonce.to_string())?,
        keccak256(b"This message attests that I control the given wallet"),
    ]);
    sign_digest(&hash_prehashed(domain, message), private_key)
}

pub fn l2_signature(
    secret: &str,
    seconds: u64,
    method: &str,
    path: &str,
    body: &str,
) -> Result<String, String> {
    if secret.is_empty() {
        return Err("Polymarket API secret is empty".into());
    }
    let mut normalized = secret.replace('-', "+").replace('_', "/");
    normalized.extend(std::iter::repeat_n('=', (4 - normalized.len() % 4) % 4));
    let key = STANDARD
        .decode(normalized)
        .map_err(|_| "invalid Polymarket base64 secret")?;
    if key.is_empty() {
        return Err("Polymarket API secret is empty".into());
    }
    let encoded = hmac_sha256(
        &key,
        format!("{seconds}{method}{path}{body}").as_bytes(),
        Encoding::Base64,
    );
    Ok(encoded.replace('+', "-").replace('/', "_"))
}

pub fn l2_headers(
    credentials: &Credentials,
    signer_address: &str,
    seconds: u64,
    method: Method,
    path: &str,
    body: &str,
) -> Result<Vec<(&'static str, String)>, String> {
    if credentials.api_key.is_empty() || credentials.passphrase.is_empty() {
        return Err("incomplete Polymarket API credentials".into());
    }
    Ok(vec![
        ("POLY_ADDRESS", get_address(signer_address)?),
        (
            "POLY_SIGNATURE",
            l2_signature(&credentials.secret, seconds, method.as_str(), path, body)?,
        ),
        ("POLY_TIMESTAMP", seconds.to_string()),
        ("POLY_API_KEY", credentials.api_key.clone()),
        ("POLY_PASSPHRASE", credentials.passphrase.clone()),
        ("Content-Type", "application/json".into()),
    ])
}

/// Official limit-builder rounding: nearest price precision, floor shares to 2dp,
/// then collateral to price precision + 2dp. Both assets use six base-unit decimals.
pub fn limit_amounts(
    side: u8,
    size: f64,
    price: f64,
    tick: f64,
) -> Result<(String, String), String> {
    let price_dp = match tick {
        0.1 => 1,
        0.01 => 2,
        0.005 | 0.001 => 3,
        0.0025 | 0.0001 => 4,
        _ => return Err("unsupported Polymarket tick size".into()),
    };
    if side > 1
        || !size.is_finite()
        || size <= 0.0
        || !price.is_finite()
        || price < tick
        || price > 1.0 - tick
    {
        return Err("invalid Polymarket limit amount/price".into());
    }
    let price_factor = 10f64.powi(price_dp);
    let rounded_price = if decimals(price) > price_dp as usize {
        ((price + f64::EPSILON) * price_factor).round() / price_factor
    } else {
        price
    };
    let shares = if decimals(size) > 2 {
        (size * 100.0).floor() / 100.0
    } else {
        size
    };
    let amount_dp = price_dp + 2;
    let mut collateral = shares * rounded_price;
    if decimals(collateral) > amount_dp as usize {
        let factor = 10f64.powi(amount_dp + 4);
        collateral = (collateral * factor).ceil() / factor;
        if decimals(collateral) > amount_dp as usize {
            let factor = 10f64.powi(amount_dp);
            collateral = (collateral * factor).floor() / factor;
        }
    }
    let shares = micro(shares)?;
    let collateral = micro(collateral)?;
    Ok(if side == 0 {
        (collateral, shares)
    } else {
        (shares, collateral)
    })
}

fn decimals(value: f64) -> usize {
    let value = value.to_string();
    value
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len())
}

fn micro(value: f64) -> Result<String, String> {
    if !value.is_finite() || value <= 0.0 {
        return Err("Polymarket rounded amount is zero/invalid".into());
    }
    // Fixed formatting avoids scientific notation and conversion through a u64.
    let decimal = format!("{value:.6}");
    let digits = decimal.replace('.', "");
    let word = uint(&digits)?;
    if word == [0; 32] {
        return Err("Polymarket rounded amount is zero".into());
    }
    Ok(digits.trim_start_matches('0').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Public Hardhat development key from the official client's signing tests.
    const KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const ADDRESS: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
    // Independent oracle: installed official @polymarket/clob-client-v2 1.1.0
    // ExchangeOrderBuilderV2 + ethers Wallet. These are NOT adapter outputs.
    #[test]
    fn v2_order_and_erc7739_official_client_vectors() {
        let vectors = [
            (
                false,
                0,
                "0xd92cf958f3e59ce31e13440281bfb353c00dfec42162626bbedfae6c1e5621df",
                "0x25b4dcdc1ae0465f546c3b3f7d17f3e5f8cc7aa9ffec7a9645e5d0efb6d8533c74849b0c9bce02c0a7fd898906539e5820ce524c42760baa487cf90970cf42e31c",
            ),
            (
                false,
                1,
                "0xe4abca3511c72e451ce7979336b3b24867dbcd608f2cf2b583c16921f35a1251",
                "0xd13e01afc591b18c89757147354a5ad20ff0e1e828da3a18c4f0317d587c16dd3b719cc7cbbef51f95bb8181f294f5538f2f2739f593f15a6ac507cd8d24a6471c",
            ),
            (
                false,
                2,
                "0x5f63c04bc3e14f3273490d4dadb81b4a6a3918796d6b9fbc31bfef0b4c7d77cc",
                "0xaf327c78450908b3cee8e5bf2c7d197d57f8d8f69bcdf147201e33d555f38be25ff062a13545dc150f460b2e234b29495402f455b7ea36105f05641080d2c5db1b",
            ),
            (
                false,
                3,
                "0xf0cbfa3cdffa5588aa535db29326c3bdbee5f9e6ecff4ab80a1a0fa64f4bcbef",
                "0x26ae590a3e70af5ac74822cc06e0cfc4ebf6b843729531f7f627b97de6a7a57d2a9d96e4af8d0e6bf385343f4f0a3afcec6bd05209740313725b5e37adc716731b3264e159346253e26a64e00b69032db0e7d32f94628de3e6eecb50304d7af3d26f024a84e14f6044ccada4cbe1493c0a166b8639565ccaf381c4ecb1d817945c4f726465722875696e743235362073616c742c61646472657373206d616b65722c61646472657373207369676e65722c75696e7432353620746f6b656e49642c75696e74323536206d616b6572416d6f756e742c75696e743235362074616b6572416d6f756e742c75696e743820736964652c75696e7438207369676e6174757265547970652c75696e743235362074696d657374616d702c62797465733332206d657461646174612c62797465733332206275696c6465722900ba",
            ),
            (
                true,
                0,
                "0x96fc29b1d64d94f7e465d88bf1986cf60e61cb4ebb3d766ac86d6dcdc8784c4b",
                "0x240d629c3fd31c9a671f6fcbeb7af38ea5bb98268f14ec7dca98cd967fe2794430a48ebdcda9b9869c17655e132654b284669c9b17c9cbebea7961106ce3ed5e1b",
            ),
            (
                true,
                1,
                "0xee126eeabb9851beebbf2c428eabc13d2054e128cda202f0b8515672562ac972",
                "0xd977107501bdc1f14b3080d81b4d5b28c4c9168eaf105bbacc0422540cbad8ab2aac457f127aabc458e8c447d3dbd0a52ab7eda7feae2d3beefb573afcde7a9a1b",
            ),
            (
                true,
                2,
                "0x5e94167161ac462d88fcb56e3d623aa65275793d389432ef8a7fbb15ecfa7288",
                "0xd7a3a3fd085fbfc5660cabb352dd2c01a95680ede9794a2835ee81ddefe716654059a156017ddd87bace422b99255dda9794e0b1ea2a864b261993caadc0ef821c",
            ),
            (
                true,
                3,
                "0x1cb786d013b31f79f456cdf5dc47bf209816a92d7c368748c7c32a2aa5da071a",
                "0x403cf9a4bc8b9bcbc7fa2afc6792b4a95265cc463ffa99d1c4abca7eea152e151a94062a3785e112afe3d18052249195f09b27897b058d38d4d0aff5c37dd8531c9b858f53327b0bd13af8ec14cfb35234fb9eb7b0504d1a4e61f433840d30e81a5ae0ee660a92a78c91a2e7e2212197f0a2f1c730d93ffea1dec3bdb6336d79864f726465722875696e743235362073616c742c61646472657373206d616b65722c61646472657373207369676e65722c75696e7432353620746f6b656e49642c75696e74323536206d616b6572416d6f756e742c75696e743235362074616b6572416d6f756e742c75696e743820736964652c75696e7438207369676e6174757265547970652c75696e743235362074696d657374616d702c62797465733332206d657461646174612c62797465733332206275696c6465722900ba",
            ),
        ];
        for (neg_risk, signature_type, hash, signature) in vectors {
            let maker = if signature_type == 0 {
                ADDRESS
            } else {
                "0x1111111111111111111111111111111111111111"
            };
            let order = OrderSigning {
                salt: "479249096354".into(),
                maker: maker.into(),
                signer: order_signer(KEY, maker, signature_type).unwrap(),
                token_id:
                    "115792089237316195423570985008687907853269984665640564039457584007913129639934"
                        .into(),
                maker_amount: "617280".into(),
                taker_amount: "1234560".into(),
                side: u8::from(neg_risk),
                signature_type,
                timestamp: "1780449126930".into(),
                metadata: [0xab; 32],
                builder: [0xcd; 32],
            };
            assert_eq!(
                format!(
                    "0x{}",
                    hex(&hash_prehashed(
                        domain(neg_risk).unwrap(),
                        contents(&order).unwrap()
                    ))
                ),
                hash
            );
            assert_eq!(order_signature(&order, KEY, neg_risk).unwrap(), signature);
            assert_ne!(order_signature(&order, KEY, !neg_risk).unwrap(), signature);
        }
    }

    #[test]
    fn l1_auth_official_client_vectors() {
        assert_eq!(
            l1_signature(KEY, 1700000000, 23).unwrap(),
            "0x8e803fe7db6838a932b1e0f0a36e55964fb8b4fae6842472eddb5bbb850154bc2bcb0ca68e5843aa178169b5ad9238ea9d56834fd3eb19885e48c7a4c53a40bd1c"
        );
        // Literal upstream tests/signing/eip712.test.ts vector, chain Amoy.
        assert_eq!(
            l1_chain(KEY, 10000000, 23, 80002).unwrap(),
            "0xf62319a987514da40e57e2f4d7529f7bac38f0355bd88bb5adbb3768d80de6c1682518e0af677d5260366425f4361e7b70c25ae232aff0ab2331e2b164a1aedc1b"
        );
        assert_ne!(
            l1_signature(KEY, 1700000000, 23).unwrap(),
            l1_signature(KEY, 1700000001, 23).unwrap()
        );
        assert_ne!(
            l1_signature(KEY, 1700000000, 23).unwrap(),
            l1_signature(KEY, 1700000000, 24).unwrap()
        );
    }

    #[test]
    fn l2_upstream_vector_exact_body_and_urlsafe_padding() {
        let secret = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        assert_eq!(
            l2_signature(
                secret,
                1000000,
                "test-sign",
                "/orders",
                r#"{"hash": "0x123"}"#
            )
            .unwrap(),
            "ZwAdJKvoYRlEKDkNMwd5BuwNNtg93kNaR_oU2HrfVvc="
        );
        let padded = l2_signature(
            "-__-AQ==",
            1700000000,
            "POST",
            "/order",
            r#"{"amount":"12"}"#,
        )
        .unwrap();
        assert_eq!(
            l2_signature("-__-AQ", 1700000000, "POST", "/order", r#"{"amount":"12"}"#).unwrap(),
            padded
        );
        assert_eq!(
            l2_signature("+//+AQ", 1700000000, "POST", "/order", r#"{"amount":"12"}"#).unwrap(),
            padded
        );
        assert_ne!(
            l2_signature(
                "-__-AQ",
                1700000000,
                "POST",
                "/order",
                r#"{ "amount":"12"}"#
            )
            .unwrap(),
            padded
        );
        assert_ne!(
            l2_signature(
                "-__-AQ",
                1700000000,
                "POST",
                "/order?a=1",
                r#"{"amount":"12"}"#
            )
            .unwrap(),
            padded
        );
        assert!(padded.ends_with('='));
        assert!(!padded.contains('+') && !padded.contains('/'));
        for bad in ["", "!", "a", "===="] {
            assert!(l2_signature(bad, 1, "GET", "/", "").is_err());
        }
    }

    #[test]
    fn l2_headers_contain_signer_wallet_and_exact_timestamp() {
        let credentials = Credentials {
            api_key: "test-api-key".into(),
            secret: "-__-AQ".into(),
            passphrase: "test-passphrase".into(),
        };
        let headers = l2_headers(
            &credentials,
            ADDRESS,
            1700000000,
            Method::Delete,
            "/order",
            r#"{"orderID":"test-order"}"#,
        )
        .unwrap();
        assert_eq!(
            headers,
            vec![
                ("POLY_ADDRESS", ADDRESS.to_string()),
                (
                    "POLY_SIGNATURE",
                    l2_signature(
                        &credentials.secret,
                        1700000000,
                        "DELETE",
                        "/order",
                        r#"{"orderID":"test-order"}"#
                    )
                    .unwrap()
                ),
                ("POLY_TIMESTAMP", "1700000000".into()),
                ("POLY_API_KEY", "test-api-key".into()),
                ("POLY_PASSPHRASE", "test-passphrase".into()),
                ("Content-Type", "application/json".into()),
            ]
        );
        assert!(l2_headers(&credentials, "invalid", 1, Method::Get, "/data/orders", "").is_err());
        let mut missing = credentials.clone();
        missing.api_key.clear();
        assert!(l2_headers(&missing, ADDRESS, 1, Method::Get, "/data/orders", "").is_err());
    }

    #[test]
    fn unsigned_decimal_and_account_type_fail_closed() {
        for bad in [
            "",
            "-1",
            "+1",
            "0x01",
            "1.0",
            " 1",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ] {
            assert!(uint(bad).is_err(), "{bad}");
        }
        assert_eq!(
            uint("115792089237316195423570985008687907853269984665640564039457584007913129639935")
                .unwrap(),
            [255; 32]
        );
        assert!(order_signer(KEY, ADDRESS, 4).is_err());
        assert!(l1_signature("0x00", 1, 0).is_err());
        let order = OrderSigning {
            salt: "1".into(),
            maker: ADDRESS.into(),
            signer: "0x1111111111111111111111111111111111111111".into(),
            token_id: "1".into(),
            maker_amount: "1".into(),
            taker_amount: "1".into(),
            side: 0,
            signature_type: 1,
            timestamp: "1".into(),
            metadata: [0; 32],
            builder: [0; 32],
        };
        assert!(order_signature(&order, KEY, false).is_err());
    }

    #[test]
    fn amounts_official_client_rounding_matrix() {
        let vectors: &[(f64, u8, f64, f64, &str, &str)] = &[
            (0.1, 0, 1.23456, 0.56789, "738000", "1230000"),
            (0.1, 0, 1.13, 0.29, "339000", "1130000"),
            (
                0.1,
                0,
                99999999.999,
                0.3333333,
                "29999999997000",
                "99999999990000",
            ),
            (0.1, 0, 0.01, 0.5, "5000", "10000"),
            (0.1, 0, 12.34, 0.5, "6170000", "12340000"),
            (0.1, 0, 0.03, 0.1, "3000", "30000"),
            (0.1, 1, 1.23456, 0.56789, "1230000", "738000"),
            (0.1, 1, 1.13, 0.29, "1130000", "339000"),
            (
                0.1,
                1,
                99999999.999,
                0.3333333,
                "99999999990000",
                "29999999997000",
            ),
            (0.1, 1, 0.01, 0.5, "10000", "5000"),
            (0.1, 1, 12.34, 0.5, "12340000", "6170000"),
            (0.1, 1, 0.03, 0.1, "30000", "3000"),
            (0.01, 0, 1.23456, 0.56789, "701100", "1230000"),
            (0.01, 0, 1.13, 0.29, "327700", "1130000"),
            (
                0.01,
                0,
                99999999.999,
                0.3333333,
                "32999999996700",
                "99999999990000",
            ),
            (0.01, 0, 0.01, 0.5, "5000", "10000"),
            (0.01, 0, 12.34, 0.5, "6170000", "12340000"),
            (0.01, 0, 0.03, 0.1, "3000", "30000"),
            (0.01, 1, 1.23456, 0.56789, "1230000", "701100"),
            (0.01, 1, 1.13, 0.29, "1130000", "327700"),
            (
                0.01,
                1,
                99999999.999,
                0.3333333,
                "99999999990000",
                "32999999996700",
            ),
            (0.01, 1, 0.01, 0.5, "10000", "5000"),
            (0.01, 1, 12.34, 0.5, "12340000", "6170000"),
            (0.01, 1, 0.03, 0.1, "30000", "3000"),
            (0.005, 0, 1.23456, 0.56789, "698640", "1230000"),
            (0.005, 0, 1.13, 0.29, "327700", "1130000"),
            (
                0.005,
                0,
                99999999.999,
                0.3333333,
                "33299999996670",
                "99999999990000",
            ),
            (0.005, 0, 0.01, 0.5, "5000", "10000"),
            (0.005, 0, 12.34, 0.5, "6170000", "12340000"),
            (0.005, 0, 0.03, 0.1, "3000", "30000"),
            (0.005, 1, 1.23456, 0.56789, "1230000", "698640"),
            (0.005, 1, 1.13, 0.29, "1130000", "327700"),
            (
                0.005,
                1,
                99999999.999,
                0.3333333,
                "99999999990000",
                "33299999996670",
            ),
            (0.005, 1, 0.01, 0.5, "10000", "5000"),
            (0.005, 1, 12.34, 0.5, "12340000", "6170000"),
            (0.005, 1, 0.03, 0.1, "30000", "3000"),
            (0.0025, 0, 1.23456, 0.56789, "698517", "1230000"),
            (0.0025, 0, 1.13, 0.29, "327700", "1130000"),
            (
                0.0025,
                0,
                99999999.999,
                0.3333333,
                "33329999996666",
                "99999999990000",
            ),
            (0.0025, 0, 0.01, 0.5, "5000", "10000"),
            (0.0025, 0, 12.34, 0.5, "6170000", "12340000"),
            (0.0025, 0, 0.03, 0.1, "3000", "30000"),
            (0.0025, 1, 1.23456, 0.56789, "1230000", "698517"),
            (0.0025, 1, 1.13, 0.29, "1130000", "327700"),
            (
                0.0025,
                1,
                99999999.999,
                0.3333333,
                "99999999990000",
                "33329999996666",
            ),
            (0.0025, 1, 0.01, 0.5, "10000", "5000"),
            (0.0025, 1, 12.34, 0.5, "12340000", "6170000"),
            (0.0025, 1, 0.03, 0.1, "30000", "3000"),
            (0.001, 0, 1.23456, 0.56789, "698640", "1230000"),
            (0.001, 0, 1.13, 0.29, "327700", "1130000"),
            (
                0.001,
                0,
                99999999.999,
                0.3333333,
                "33299999996670",
                "99999999990000",
            ),
            (0.001, 0, 0.01, 0.5, "5000", "10000"),
            (0.001, 0, 12.34, 0.5, "6170000", "12340000"),
            (0.001, 0, 0.03, 0.1, "3000", "30000"),
            (0.001, 1, 1.23456, 0.56789, "1230000", "698640"),
            (0.001, 1, 1.13, 0.29, "1130000", "327700"),
            (
                0.001,
                1,
                99999999.999,
                0.3333333,
                "99999999990000",
                "33299999996670",
            ),
            (0.001, 1, 0.01, 0.5, "10000", "5000"),
            (0.001, 1, 12.34, 0.5, "12340000", "6170000"),
            (0.001, 1, 0.03, 0.1, "30000", "3000"),
            (0.0001, 0, 1.23456, 0.56789, "698517", "1230000"),
            (0.0001, 0, 1.13, 0.29, "327700", "1130000"),
            (
                0.0001,
                0,
                99999999.999,
                0.3333333,
                "33329999996666",
                "99999999990000",
            ),
            (0.0001, 0, 0.01, 0.5, "5000", "10000"),
            (0.0001, 0, 12.34, 0.5, "6170000", "12340000"),
            (0.0001, 0, 0.03, 0.1, "3000", "30000"),
            (0.0001, 1, 1.23456, 0.56789, "1230000", "698517"),
            (0.0001, 1, 1.13, 0.29, "1130000", "327700"),
            (
                0.0001,
                1,
                99999999.999,
                0.3333333,
                "99999999990000",
                "33329999996666",
            ),
            (0.0001, 1, 0.01, 0.5, "10000", "5000"),
            (0.0001, 1, 12.34, 0.5, "12340000", "6170000"),
            (0.0001, 1, 0.03, 0.1, "30000", "3000"),
        ];
        for &(tick, side, size, price, maker, taker) in vectors {
            assert_eq!(
                limit_amounts(side, size, price, tick).unwrap(),
                (maker.to_string(), taker.to_string()),
                "tick={tick} size={size} price={price} side={side}"
            );
        }
        for (size, price, tick) in [
            (0.001, 0.5, 0.01),
            (f64::NAN, 0.5, 0.01),
            (1.0, f64::INFINITY, 0.01),
            (1.0, 0.5, 0.02),
            (1.0, 0.999, 0.01),
        ] {
            assert!(limit_amounts(0, size, price, tick).is_err());
        }
    }
}

safe_cex_crypto::erase_credentials_on_drop!(Credentials, api_key, secret, passphrase);
