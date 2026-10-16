use safe_cex_transport::signing::RemoteSignerRequest;
fn dispatch(request: RemoteSignerRequest) {
    match request {
        RemoteSignerRequest::Http(_) => (),
    }
}
fn main() {}
