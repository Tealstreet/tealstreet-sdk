use safe_cex_transport::RestResponseBody;
fn main() {
    let body = RestResponseBody::Encoded(b"null".to_vec());
    let _: &[u8] = &body;
}
