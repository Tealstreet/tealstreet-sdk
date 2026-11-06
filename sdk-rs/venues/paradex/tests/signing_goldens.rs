//! Exact `/auth` headers and signed write bodies, captured before signing moved
//! to the provider: both key forms on both networks must stay byte-identical.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, HttpRequest, SocketChannel};
use safe_cex_venue_paradex::Options;
use serde_json::{Value, json};
use support::*;

const STARK_KEY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";
const ETHEREUM_KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";

struct Network {
    name: &'static str,
    testnet: bool,
    config: &'static str,
}

const NETWORKS: [Network; 2] = [
    Network {
        name: "prod",
        testnet: false,
        config: r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET","l1_chain_id":"1","paraclear_account_hash":"0x123","paraclear_account_proxy_hash":"0x456"}"#,
    },
    Network {
        name: "testnet",
        testnet: true,
        config: r#"{"starknet_chain_id":"PRIVATE_SN_POTC_SEPOLIA","l1_chain_id":"11155111","paraclear_account_hash":"0x123","paraclear_account_proxy_hash":"0x456"}"#,
    },
];

fn private(key: &str, network: &Network) -> (Harness, HttpRequest) {
    let mut h = Harness::keyed(
        Options {
            testnet: network.testnet,
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        key,
    );
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply("/system/config", network.config);
    let auth = h.take("/auth/");
    h.respond(auth.clone(), 200, r#"{"jwt_token":"jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    (h, auth)
}

fn order(kind: OrderType, client: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTC-USD-PERP","type":{},"side":"buy","amount":1.5,"price":100.1,"clientOrderId":"{client}"}}"#,
        serde_json::to_string(&kind).unwrap()
    ))
    .unwrap()
}

fn wire(name: &str, request: &HttpRequest) -> Value {
    json!({
        "name": name,
        "method": format!("{:?}", request.method),
        "url": request.url,
        "headers": request.headers.iter().map(|(n, v)| json!([n, v])).collect::<Vec<_>>(),
        "body": request.body,
    })
}

fn writes(h: &mut Harness) -> Vec<Value> {
    let mut out = vec![];
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order(OrderType::Limit, "one")],
    });
    let request = h.take("/orders/batch");
    out.push(wire("batch-one", &request));
    h.respond(
        request,
        200,
        r#"{"orders":[{"id":"id-one","client_id":"one"}]}"#,
    );
    h.command(Command::PlaceOrders {
        command: 2,
        orders: ["a", "b", "c"]
            .map(|client| order(OrderType::Limit, client))
            .to_vec(),
    });
    let request = h.take("/orders/batch");
    out.push(wire("batch-three", &request));
    h.respond(
        request,
        200,
        r#"{"orders":[{"id":"id-a","client_id":"a"},{"id":"id-b","client_id":"b"},{"id":"id-c","client_id":"c"}]}"#,
    );
    h.command(Command::PlaceOrders {
        command: 3,
        orders: vec![order(OrderType::Market, "market")],
    });
    let request = h.take("/orders");
    out.push(wire("market", &request));
    h.respond(request, 200, r#"{"id":"id-market","client_id":"market"}"#);
    let mut bracket = order(OrderType::Limit, "entry");
    bracket.take_profit = Some(110.);
    bracket.stop_loss = Some(90.);
    bracket.client_order_id = None;
    h.command(Command::PlaceOrders {
        command: 4,
        orders: vec![bracket],
    });
    let entry = h.take("/orders");
    out.push(wire("bracket-entry", &entry));
    let client: Value = serde_json::from_str(entry.body.as_deref().unwrap()).unwrap();
    let client = client["client_id"].as_str().unwrap().to_owned();
    h.respond(
        entry,
        200,
        &format!(r#"{{"id":"entry","client_id":"{client}"}}"#),
    );
    for (name, reply) in [
        ("bracket-take-profit", r#"{"id":"tp","client_id":""}"#),
        ("bracket-stop-loss", r#"{"id":"sl"}"#),
    ] {
        let request = h.take("/orders");
        out.push(wire(name, &request));
        h.respond(request, 200, reply);
    }
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    h.e.handle(h.now, Event::WsMessage { socket: private, text: r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"SELL","size":"2","remaining_size":"2","price":"120","flags":[],"instruction":"GTC","created_at":1700000000000}}}"# });
    h.collect();
    h.command(Command::AmendOrder {
        command: 5,
        id: "123".into(),
        price: Some(121.5),
        amount: Some(3.),
    });
    let request = h.take("/orders/123");
    out.push(wire("amend", &request));
    h.respond(request, 200, r#"{"id":"123"}"#);
    out
}

fn capture() -> Value {
    let mut cases = vec![];
    for network in &NETWORKS {
        for (form, key) in [("stark", STARK_KEY), ("ethereum", ETHEREUM_KEY)] {
            let (mut h, auth) = private(key, network);
            let mut requests = vec![wire("auth", &auth)];
            requests.extend(writes(&mut h));
            cases.push(json!({
                "network": network.name,
                "form": form,
                "now": h.now,
                "requests": requests,
            }));
        }
    }
    json!({ "cases": cases })
}

#[test]
fn auth_headers_and_signed_write_bodies_match_the_pre_provider_goldens() {
    let expected: Value = serde_json::from_str(include_str!("signing-goldens.json")).unwrap();
    assert_eq!(capture(), expected);
}
