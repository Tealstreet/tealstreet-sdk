//! The writer RPC fixtures are the contract TS encoders and every writer host test
//! against: each request's command must decode as an engine host command.
use safe_cex_engine::command::decode_host_input;
use serde_json::Value;

const FIXTURES: &str = include_str!("../schema/writer-rpc/fixtures.json");

#[test]
fn every_fixture_request_command_decodes_as_a_host_command() {
    let fixtures: Value = serde_json::from_str(FIXTURES).unwrap();
    let requests = fixtures["requests"].as_array().unwrap();
    assert!(!requests.is_empty());
    for request in requests {
        // The host stamps the completion id; the RPC never carries one.
        let mut command = request["command"].clone();
        command["command"] = 1.into();
        let command = command.to_string();
        decode_host_input(&command).unwrap_or_else(|error| panic!("{command}: {error}"));
    }
}
