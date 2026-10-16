use safe_cex_venue_weex::selection::get_desired_ticker_topics;
use safe_cex_venue_weex::subscriptions::*;
use serde_json::Value;
use std::{path::Path, process::Command};

#[derive(Debug)]
struct Packet {
    method: WeexSubscriptionMethod,
    topics: Vec<Vec<u16>>,
    id: f64,
}

struct Host {
    available: Vec<Vec<u16>>,
    active: Vec<Vec<u16>>,
    clocks: Vec<f64>,
    clock_index: usize,
    failures: Value,
    disconnect_after_send: bool,
    events: Vec<&'static str>,
    packets: Vec<Packet>,
    logs: Vec<String>,
}

fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}

fn strings(value: &Value) -> Vec<Vec<u16>> {
    serde_json::from_value(value.clone()).unwrap()
}

fn now(host: &mut Host, _: &mut WeexSubscriptionState) -> Result<f64, String> {
    host.events.push("now");
    if host.failures["clock"] == true {
        return Err("TypeError".into());
    }
    let now = host.clocks[host.clock_index.min(host.clocks.len() - 1)];
    host.clock_index += 1;
    Ok(now)
}

fn desired_topics(
    host: &mut Host,
    state: &mut WeexSubscriptionState,
) -> Result<Vec<Vec<u16>>, String> {
    host.events.push("desired");
    if host.failures["desired"] == true {
        return Err("TypeError".into());
    }
    Ok(get_desired_ticker_topics(
        &host.available,
        &host.active,
        state.topics.len(),
    ))
}

fn log_warning(
    host: &mut Host,
    _: &mut WeexSubscriptionState,
    message: &str,
) -> Result<(), String> {
    host.events.push("log");
    host.logs.push(message.to_owned());
    if host.failures["log"] == true {
        return Err("RangeError".into());
    }
    Ok(())
}

fn send(
    host: &mut Host,
    state: &mut WeexSubscriptionState,
    method: WeexSubscriptionMethod,
    topics: &[Vec<u16>],
    id: f64,
) -> Result<(), String> {
    host.events.push("packet");
    host.packets.push(Packet {
        method,
        topics: topics.to_vec(),
        id,
    });
    let index = host.packets.len() as u64;
    if host.failures["json"].as_u64() == Some(index) {
        return Err("TypeError".into());
    }
    host.events.push("send");
    if host.disconnect_after_send {
        state.is_connected = false;
    }
    if host.failures["send"].as_u64() == Some(index) {
        return Err("RangeError".into());
    }
    Ok(())
}

fn method(action: &Value) -> WeexSubscriptionMethod {
    if action["method"] == "UNSUBSCRIBE" {
        WeexSubscriptionMethod::Unsubscribe
    } else {
        WeexSubscriptionMethod::Subscribe
    }
}

#[test]
fn subscription_state_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/subscriptions-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected_ids = [
        "operation/basic",
        "operation/unsubscribe",
        "operation/expired_boundary",
        "operation/budget239",
        "operation/budget240",
        "operation/logged240",
        "operation/pruning_reopens",
        "operation/repeated_cap",
        "operation/nan_id",
        "operation/infinite_id",
        "operation/signed_zero_id",
        "operation/throw_clock",
        "operation/throw_log",
        "operation/throw_json",
        "operation/throw_send",
        "batch/disconnected_track",
        "batch/empty",
        "batch/size1",
        "batch/size50",
        "batch/size51",
        "batch/size101",
        "batch/no_track",
        "batch/exhausted",
        "batch/throw_send",
        "batch/disconnect_during_first",
        "reconcile/connected_replace",
        "reconcile/disconnected_replace",
        "reconcile/unchanged",
        "reconcile/blocked_both",
        "reconcile/only_unsub_allowed",
        "reconcile/empty_desired",
        "reconcile/empty_old",
        "reconcile/throw_desired",
        "reconcile/throw_unsub",
        "reconcile/throw_sub",
        "ticker/basic",
        "ticker/disconnected",
        "ticker/blocked",
        "ticker/throw_desired",
        "ticker/throw_send",
        "resubscribe/basic",
        "resubscribe/empty",
        "resubscribe/disconnected",
        "resubscribe/throw_send",
        "subscribe/basic",
        "subscribe/duplicate",
        "subscribe/disconnected",
        "subscribe/blocked",
        "subscribe/throw_desired",
        "subscribe/throw_send",
        "unsubscribe/basic",
        "unsubscribe/disconnected",
        "unsubscribe/missing",
        "unsubscribe/blocked",
        "unsubscribe/throw_send",
        "changed/disposed",
        "changed/connected",
        "changed/disconnected",
        "changed/throw_desired",
    ];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 59, "subscriptions::case_count");
    let mut calls = 0;
    for (index, case) in cases.iter().enumerate() {
        let id = expected_ids[index];
        assert_eq!(case["id"], id, "subscriptions::case_identity");
        let input = &case["input"];
        let mut state = WeexSubscriptionState {
            topics: strings(&input["topics"]),
            ticker_topics: strings(&input["tickers"]),
            is_connected: input["connected"].as_bool().unwrap(),
            is_disposed: input["disposed"].as_bool().unwrap(),
            operation_timestamps: input["timestamps"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect(),
            has_logged_limit: input["logged"].as_bool().unwrap(),
            subscription_id: number(&input["subscriptionId"]),
        };
        if id == "operation/signed_zero_id" {
            assert_eq!(
                state.subscription_id.to_bits(),
                0x8000_0000_0000_0000,
                "subscriptions::native_negative_zero_input"
            );
        }
        let mut host = Host {
            available: strings(&input["available"]),
            active: strings(&input["active"]),
            clocks: input["clock"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect(),
            clock_index: 0,
            failures: input["failures"].clone(),
            disconnect_after_send: input["disconnectAfterSend"].as_bool().unwrap(),
            events: Vec::new(),
            packets: Vec::new(),
            logs: Vec::new(),
        };
        let runtime = WeexSubscriptionRuntime {
            now,
            desired_topics,
            log_warning,
            send,
        };
        let actions = case["actions"].as_array().unwrap();
        let mut outcomes: Vec<Result<Option<bool>, String>> = Vec::new();
        for action in actions {
            let topics = strings(&action["topics"]);
            let topic: Vec<u16> = serde_json::from_value(action["topic"].clone()).unwrap();
            let result = match action["name"].as_str().unwrap() {
                "sendSubscriptionOperation" => send_subscription_operation(
                    &mut host,
                    &mut state,
                    method(action),
                    &topics,
                    &runtime,
                )
                .map(Some),
                "subscribeTopics" => subscribe_topics(
                    &mut host,
                    &mut state,
                    &topics,
                    action["track"].as_bool().unwrap_or(true),
                    &runtime,
                )
                .map(|()| None),
                "reconcileTickerTopics" => {
                    reconcile_ticker_topics(&mut host, &mut state, &runtime).map(|()| None)
                }
                "subscribeTickerTopics" => {
                    subscribe_ticker_topics(&mut host, &mut state, &runtime).map(|()| None)
                }
                "resubscribeTopics" => {
                    resubscribe_topics(&mut host, &mut state, &runtime).map(|()| None)
                }
                "subscribeTopic" => {
                    subscribe_topic(&mut host, &mut state, topic, &runtime).map(|()| None)
                }
                "unsubscribeTopic" => {
                    unsubscribe_topic(&mut host, &mut state, &topic, &runtime).map(|()| None)
                }
                "handleActiveTickerSymbolsChanged" => {
                    handle_active_ticker_symbols_changed(&mut host, &mut state, &runtime)
                        .map(|()| None)
                }
                other => panic!("unexpected action {other}"),
            };
            calls += 1;
            let failed = result.is_err();
            outcomes.push(result);
            if failed {
                break;
            }
        }
        let expected_results = case["results"].as_array().unwrap();
        assert_eq!(
            outcomes.len(),
            expected_results.len(),
            "subscriptions::{id}::result_count"
        );
        for (outcome, expected) in outcomes.iter().zip(expected_results) {
            match outcome {
                Ok(Some(value)) => assert_eq!(
                    Some(*value),
                    expected["boolean"].as_bool(),
                    "subscriptions::{id}::result"
                ),
                Ok(None) => assert_eq!(
                    expected["unit"].as_bool(),
                    Some(true),
                    "subscriptions::{id}::result"
                ),
                Err(error) => assert_eq!(
                    Some(error.as_str()),
                    expected["error"].as_str(),
                    "subscriptions::{id}::result"
                ),
            }
        }
        let expected = &case["state"];
        assert_eq!(
            state.topics,
            strings(&expected["topics"]),
            "subscriptions::{id}::state_topics"
        );
        assert_eq!(
            state.ticker_topics,
            strings(&expected["tickers"]),
            "subscriptions::{id}::state_tickers"
        );
        assert_eq!(
            state.is_connected,
            expected["connected"].as_bool().unwrap(),
            "subscriptions::{id}::state_connected"
        );
        assert_eq!(
            state.is_disposed,
            expected["disposed"].as_bool().unwrap(),
            "subscriptions::{id}::state_disposed"
        );
        let expected_timestamps: Vec<u64> = expected["timestamps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| number(item).to_bits())
            .collect();
        assert_eq!(
            state
                .operation_timestamps
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_timestamps,
            "subscriptions::{id}::state_timestamps"
        );
        assert_eq!(
            state.has_logged_limit,
            expected["logged"].as_bool().unwrap(),
            "subscriptions::{id}::state_logged"
        );
        assert_eq!(
            state.subscription_id.to_bits(),
            number(&expected["subscriptionId"]).to_bits(),
            "subscriptions::{id}::state_id"
        );
        let packets = case["packets"].as_array().unwrap();
        assert_eq!(
            host.packets.len(),
            packets.len(),
            "subscriptions::{id}::packet_count"
        );
        for (packet, expected) in host.packets.iter().zip(packets) {
            let expected_method = match expected["method"].as_str().unwrap() {
                "SUBSCRIBE" => WeexSubscriptionMethod::Subscribe,
                "UNSUBSCRIBE" => WeexSubscriptionMethod::Unsubscribe,
                other => panic!("unexpected packet method {other}"),
            };
            assert_eq!(
                packet.method, expected_method,
                "subscriptions::{id}::packet_method"
            );
            assert_eq!(
                packet.topics,
                strings(&expected["topics"]),
                "subscriptions::{id}::packet_topics"
            );
            assert_eq!(
                packet.id.to_bits(),
                number(&expected["id"]).to_bits(),
                "subscriptions::{id}::packet_id"
            );
        }
        let logs = case["logs"].as_array().unwrap();
        assert_eq!(
            host.logs.len(),
            logs.len(),
            "subscriptions::{id}::log_count"
        );
        for (message, expected) in host.logs.iter().zip(logs) {
            assert_eq!(
                message,
                expected["message"].as_str().unwrap(),
                "subscriptions::{id}::log_message"
            );
            assert_eq!(
                expected["severity"], "warning",
                "subscriptions::{id}::warning_capability"
            );
        }
        let events: Vec<&str> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        assert_eq!(host.events, events, "subscriptions::{id}::effects");
        if id == "batch/size101" {
            assert_eq!(
                host.packets
                    .iter()
                    .map(|packet| packet.topics.len())
                    .collect::<Vec<_>>(),
                [50, 50, 1],
                "subscriptions::independent_batch_lengths"
            );
        }
        if id == "subscribe/basic" {
            assert_eq!(state.topics.len(), 11, "subscriptions::tracked_topic_added");
            assert_eq!(
                state.ticker_topics.len(),
                89,
                "subscriptions::reduced_ticker_capacity"
            );
        }
    }
    assert_eq!(calls, 60, "subscriptions::independent_call_count");
}
