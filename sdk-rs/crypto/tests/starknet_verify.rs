use safe_cex_crypto::starknet;
use starknet_crypto::Felt;

#[test]
fn fixed_width_verification_and_malformed_inputs() {
    let key = Felt::from(7u8);
    let message = Felt::from(99u8);
    let (r, s) = starknet::sign(&message, &key).unwrap();
    let mut signature = r.to_bytes_be().to_vec();
    signature.extend_from_slice(&s.to_bytes_be());
    let public = starknet::public_key(&key).to_bytes_be();
    assert!(starknet::verify(
        &message.to_bytes_be(),
        &public,
        &signature
    ));
    assert!(!starknet::verify(
        &Felt::from(100u8).to_bytes_be(),
        &public,
        &signature
    ));
    assert!(!starknet::verify(
        &message.to_bytes_be(),
        &starknet::public_key(&Felt::from(8u8)).to_bytes_be(),
        &signature
    ));
    for invalid in [&[][..], &[0; 64][..], &[255; 64][..], &signature[..63]] {
        assert!(!starknet::verify(&message.to_bytes_be(), &public, invalid));
    }
    for invalid in [&[][..], &[0; 32][..], &[255; 32][..]] {
        assert!(!starknet::verify(
            &message.to_bytes_be(),
            invalid,
            &signature
        ));
        assert!(!starknet::verify(invalid, &public, &signature));
    }
}
