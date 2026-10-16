#[test]
fn request_query_values_cannot_enter_output_eligible_records() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/numeric_ingress_ui/query_value.rs");
}
