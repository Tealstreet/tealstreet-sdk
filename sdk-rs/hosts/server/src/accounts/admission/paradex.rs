//! Captured public account/subkey facts; proof replies cannot replace either.
use super::*;

pub(super) fn projection(user: &str, init: &InitPayload) -> Result<Projection, ProbeRefusal> {
    let account = canonical(
        init.paradex_address
            .as_deref()
            .ok_or(ProbeRefusal::INVALID)?,
    )?;
    let public = canonical(
        init.subkey_address
            .as_deref()
            .ok_or(ProbeRefusal::INVALID)?,
    )?;
    let expected_key = if init.signing_mode.as_deref() == Some("delegated") {
        &public
    } else {
        &account
    };
    if canonical(&init.key)? != *expected_key {
        return Err(ProbeRefusal::INVALID);
    }
    let facts = json!({
        "venue":"paradex",
        "network":if init.testnet == Some(true) { "testnet" } else { "mainnet" },
        "credential":{"kind":"stark", "account":account, "publicKey":public}
    });
    Projection::from_json(user, &facts.to_string()).map_err(|_| ProbeRefusal::INVALID)
}

// Native Paradex accepts decimal and either hex prefix. Normalize bounded public
// bytes here; Projection retains field-range, nonzero and on-curve validation.
fn canonical(value: &str) -> Result<String, ProbeRefusal> {
    let (digits, radix) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or((value, 10u16), |digits| (digits, 16u16));
    if value.len() > 128 || digits.is_empty() {
        return Err(ProbeRefusal::INVALID);
    }
    let mut bytes = [0u8; 32];
    for digit in digits.bytes() {
        let mut carry = char::from(digit)
            .to_digit(u32::from(radix))
            .ok_or(ProbeRefusal::INVALID)? as u16;
        for byte in bytes.iter_mut().rev() {
            let next = u16::from(*byte) * radix + carry;
            *byte = next as u8;
            carry = next >> 8;
        }
        if carry != 0 {
            return Err(ProbeRefusal::INVALID);
        }
    }
    Ok(format!("0x{}", safe_cex_crypto::hex(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_integer_spellings_normalize_without_reduction_or_truncation() {
        for (text, expected) in [
            ("291", format!("0x{:0>64}", "123")),
            ("0X000123", format!("0x{:0>64}", "123")),
            ("000291", format!("0x{:0>64}", "123")),
            ("0xAbCd", format!("0x{:0>64}", "abcd")),
            (
                "115792089237316195423570985008687907853269984665640564039457584007913129639935",
                format!("0x{}", "f".repeat(64)),
            ),
        ] {
            assert_eq!(canonical(text).unwrap(), expected, "{text}");
        }
        for text in [
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
            "0x10000000000000000000000000000000000000000000000000000000000000000",
            "",
            "0x",
            "0X",
            "-1",
            "+1",
            " 1",
            "1 ",
            "1.0",
            "1e3",
            "0xgg",
            "0b1",
            "١",
        ] {
            assert!(canonical(text).is_err(), "{text}");
        }
        assert!(canonical(&"0".repeat(129)).is_err());
    }

    #[test]
    fn projection_keeps_both_public_init_layouts_and_owner_network_scope() {
        let public_hex = "0x01ef15c18599971b7beced415a40f0c7deacfd9b0d1819e03d723d8bc943cfca";
        let public_decimal =
            "874739451078007766457464989774322083649278607533249481151382481072868806602";
        for delegated in [false, true] {
            let mut init = InitPayload {
                exchange_name: "paradex".into(),
                paradex_address: Some("291".into()),
                subkey_address: Some(public_decimal.into()),
                key: if delegated { public_hex } else { "0X0123" }.into(),
                signing_mode: Some(if delegated { "delegated" } else { "cosigned" }.into()),
                ..Default::default()
            };
            let expected = json!({
                "venue":"paradex", "network":"mainnet", "credential":{
                    "kind":"stark", "account":format!("0x{:0>64}", "123"),
                    "publicKey":public_hex
                }
            });
            let actual = projection("alice", &init).unwrap();
            assert_eq!(
                actual.digest(),
                Projection::from_json("alice", &expected.to_string())
                    .unwrap()
                    .digest()
            );
            assert_ne!(actual.digest(), projection("bob", &init).unwrap().digest());
            init.testnet = Some(true);
            assert_ne!(
                actual.digest(),
                projection("alice", &init).unwrap().digest()
            );
            init.key = if delegated { "0x123" } else { public_hex }.into();
            assert!(projection("alice", &init).is_err());
        }
    }
}
