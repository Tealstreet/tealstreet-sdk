use safe_cex_num::{
    js_number_output,
    numeric_ingress::{
        JsNumberInput, JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
    },
};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
struct Envelope<T> {
    data: T,
}
impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for Envelope<T>
{
    fn check_number_output(&self) {
        js_number_output!(self, struct { data });
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CowProfile {
    api_base_url: std::borrow::Cow<'static, str>,
    ws_url: std::borrow::Cow<'static, str>,
    signer_chain_id: f64,
}
impl JsNumberOutput for CowProfile {
    fn check_number_output(&self) {
        js_number_output!(self, struct { api_base_url, ws_url, signer_chain_id });
    }
}
#[derive(Serialize)]
struct BorrowedProfile<'a> {
    name: &'a str,
    number: f64,
}
impl JsNumberOutput for BorrowedProfile<'_> {
    fn check_number_output(&self) {
        js_number_output!(self, struct { name, number });
    }
}
fn main() {
    for (raw, expected) in [
        ("-0", (-0.0f64).to_bits()),
        ("1e400", f64::INFINITY.to_bits()),
        ("-1e-400", (-0.0f64).to_bits()),
    ] {
        let value: Envelope<JsNumberInput> = js_from_str(&format!(r#"{{"data":{raw}}}"#)).unwrap();
        assert_eq!(value.data.decode::<f64>().unwrap().to_bits(), expected);
        let raw_profile = format!(
            r#"{{"apiBaseUrl":"https://api.example","wsUrl":"wss://ws.example","signerChainId":{raw}}}"#
        );
        let parsed: serde_json::Value = serde_json::from_str(&raw_profile).unwrap();
        for profile in [
            js_from_str::<CowProfile>(&raw_profile).unwrap(),
            js_from_value::<CowProfile>(parsed.clone()).unwrap(),
            js_from_value_ref::<CowProfile>(&parsed).unwrap(),
        ] {
            assert_eq!(
                profile.signer_chain_id.to_bits(),
                expected,
                "Cow profile retains Number bits"
            );
            assert_eq!(profile.api_base_url, "https://api.example");
            assert_eq!(profile.ws_url, "wss://ws.example");
            let borrowed = CowProfile {
                api_base_url: std::borrow::Cow::Borrowed("api"),
                ws_url: std::borrow::Cow::Borrowed("ws"),
                signer_chain_id: profile.signer_chain_id,
            };
            borrowed.check_number_output();
            BorrowedProfile {
                name: borrowed.api_base_url.as_ref(),
                number: borrowed.signer_chain_id,
            }
            .check_number_output();
        }
    }
}
