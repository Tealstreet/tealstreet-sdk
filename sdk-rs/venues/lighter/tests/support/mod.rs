#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_lighter::{Lighter, Options};
pub const NOW: f64 = 1_700_000_000_000.0;
pub const META: &str = r#"{"code":200,"order_book_details":[{"market_id":1,"symbol":"ETH","status":"active","market_type":"perp","supported_size_decimals":3,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250},{"market_id":2,"symbol":"BTC","status":"active","market_type":"perp","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250}]}"#;
pub const ACCOUNT: &str = r#"{"code":200,"accounts":[{"index":718895,"collateral":"1000","total_asset_value":"1000","available_balance":"900","positions":[{"market_id":1,"position":"2","sign":1,"avg_entry_price":"100","unrealized_pnl":"0","realized_pnl":"3","initial_margin_fraction":1000,"margin_mode":0,"liquidation_price":"50"}]}]}"#;
pub const OPEN: &str = r#"{"code":200,"orders":[{"order_index":"42","market_id":1,"status":"open","type":"limit","time_in_force":1,"is_ask":false,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2","price":"90"}]}"#;
pub fn options() -> Options {
    Options {
        application_id: Some("718895".into()),
        api_key_index: Some("1".into()),
        secret: Some(format!("01{}", "00".repeat(39))),
        ..Options::default()
    }
}
pub fn reply(e: &mut Engine<Lighter>, r: &HttpRequest, status: u16, body: &str) {
    reply_at(e, NOW, r, status, body)
}
pub fn reply_at(e: &mut Engine<Lighter>, now: f64, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
pub fn message(e: &mut Engine<Lighter>, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}
pub fn requests(e: &mut Engine<Lighter>) -> Vec<HttpRequest> {
    e.drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn ready() -> (Engine<Lighter>, SocketId) {
    ready_with(options())
}
pub fn ready_with(options: Options) -> (Engine<Lighter>, SocketId) {
    let mut e = Engine::new(Lighter::new(options));
    e.start(NOW);
    let mut private = None;
    for _ in 0..30 {
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        actions.sort_by_key(|a| !matches!(a, Action::Http(r) if r.url.contains("exchangeStats")));
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    let body = if r.url.contains("orderBookDetails") || r.url.contains("orderBooks")
                    {
                        META
                    } else if r.url.contains("exchangeStats") {
                        r#"{"code":200,"order_book_stats":[{"market_id":1,"last_trade_price":"100","mark_price":"100","index_price":"100"},{"market_id":2,"last_trade_price":"200","mark_price":"200","index_price":"200"}]}"#
                    } else if r.url.contains("accountActiveOrders") {
                        OPEN
                    } else if r.url.contains("/account?") {
                        ACCOUNT
                    } else {
                        panic!("unhandled bootstrap {}", r.url)
                    };
                    reply(&mut e, &r, 200, body);
                }
                Action::WsConnect { socket, .. } => {
                    private = Some(socket);
                    e.handle(NOW, Event::WsOpened { socket });
                }
                _ => {}
            }
        }
    }
    (e, private.expect("private socket"))
}
pub fn fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + size;
    }
    0
}
