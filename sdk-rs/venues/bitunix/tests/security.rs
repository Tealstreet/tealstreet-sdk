use safe_cex_venue_bitunix::Options;

/// Construction shape: the venue receives opaque setup identities only. The
/// exhaustive pattern fails to compile if a raw key or secret field returns.
#[test]
fn options_carry_no_raw_credential_fields() {
    let Options {
        testnet: _,
        public_data_only: _,
        credentials,
        read_credentials,
    } = Options::default();
    assert!(credentials.is_none() && read_credentials.is_none());
}
