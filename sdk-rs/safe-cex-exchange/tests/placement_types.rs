#[test]
fn venue_identity_and_preflight_payload_cannot_be_bypassed() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/order_cross_venue.rs");
    cases.compile_fail("tests/ui/batch_cross_venue.rs");
    cases.compile_fail("tests/ui/order_private_payload.rs");
    cases.compile_fail("tests/ui/order_not_clone.rs");
}
