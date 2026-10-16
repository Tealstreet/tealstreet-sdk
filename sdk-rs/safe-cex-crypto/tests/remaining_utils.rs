use safe_cex_crypto::{uuid::*, wallet_utils::*};
#[test]
fn native_wallet_key_address_and_invalid_scalar_refusal() {
    let key = format!("0x{:064x}", 1);
    let wallet = create_wallet_from_private_key(&key).unwrap();
    assert_eq!(
        wallet.address().to_checksum(None),
        "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf"
    );
    assert!(
        validate_wallet_and_private_key(&key, "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf")
            .unwrap()
    );
    assert!(
        validate_wallet_and_private_key(&key, "0X7E5F4552091A69125D5DFCB7B8C2659029395BDF")
            .unwrap()
    );
    assert!(
        !validate_wallet_and_private_key(&key, " 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf")
            .unwrap()
    );
    assert_eq!(
        create_wallet_from_private_key(&format!("{:064x}", 1))
            .unwrap()
            .address(),
        wallet.address()
    );
    for key in [
        "".into(),
        "0x01".into(),
        format!("0x{:064x}", 0),
        format!("0X{:064x}", 1),
        format!("0x{}", "ff".repeat(32)),
    ] {
        assert!(
            create_wallet_from_private_key(&key).is_err(),
            "invalid scalar {key}"
        );
    }
    assert!(validate_wallet_and_private_key("bad", "bad").is_err());
}
#[test]
fn native_uuid_forward_mask_removal_and_entropy_failure() {
    assert_eq!(
        uuid_with_entropy(|bytes| {
            assert_eq!(bytes.len(), 21);
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = i as u8;
            }
            Ok(())
        })
        .unwrap(),
        "useandom26T198340PX7"
    );
    assert_eq!(
        uuid_with_entropy(|bytes| {
            bytes.fill(8);
            Ok(())
        })
        .unwrap(),
        ""
    );
    assert_eq!(
        uuid_with_entropy(|bytes| {
            bytes.fill(255);
            Ok(())
        })
        .unwrap(),
        "t".repeat(21)
    );
    assert_eq!(
        uuid_with_entropy(|bytes| {
            bytes.fill(44);
            Ok(())
        })
        .unwrap(),
        ""
    );
    let value = uuid().unwrap();
    assert!(value.len() <= 21);
    assert!(value.bytes().all(|c| c.is_ascii_alphanumeric()));
    assert_eq!(
        uuid_with_entropy(|_| Err("entropy rejected".into())).unwrap_err(),
        "entropy rejected"
    );
}
