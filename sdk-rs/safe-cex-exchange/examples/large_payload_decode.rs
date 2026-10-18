#[cfg(unix)]
#[path = "../tests/support/large_payload_decode.rs"]
mod large_payload_decode;

#[cfg(unix)]
fn main() {
    large_payload_decode::recorded();
}

#[cfg(not(unix))]
fn main() {
    panic!("recorded native decode profiling requires a Unix host");
}
