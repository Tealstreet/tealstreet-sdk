//! WEEX signs its private socket upgrade itself (`connect_signed`), so a
//! refusal fails the connect before it ever opens. A credential that can never
//! sign must still stop there, instead of reconnecting for ever.
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{Action, Event, Notice, Response, TimerId};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
mod support;
use support::PreparedActions;
use support::setup::Options;

const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"90000"}]"#;
const BOOK_TICKERS: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"89999","askPrice":"90001"}]"#;

/// Signs REST through the trusted provider; refuses every private upgrade.
struct NoUpgrade {
    inner: Provider,
    error: SignError,
    upgrades: Arc<AtomicUsize>,
}
impl Signer for NoUpgrade {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_ready(&request).expect("immediate");
        Box::pin(std::future::ready(result))
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if request.context.method == "WS" {
            self.upgrades.fetch_add(1, Ordering::Relaxed);
            Some(Err(self.error))
        } else {
            self.inner.sign_ready(request)
        }
    }
}

/// Ten simulated minutes: every timer fires in order, and each market read is
/// answered. Returns the upgrade signing attempts and visible failures.
fn upgrades_in_ten_minutes(error: SignError) -> (usize, usize) {
    let upgrades = Arc::new(AtomicUsize::new(0));
    let counter = upgrades.clone();
    let mut engine = support::setup::engine_with_provider(
        Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            private_ws_headers: true,
            ..Options::default()
        },
        move |inner| {
            Arc::new(NoUpgrade {
                inner,
                error,
                upgrades: counter,
            })
        },
    );
    engine.start(0.0);
    let mut timers: Vec<(f64, TimerId)> = Vec::new();
    let mut failures = 0;
    let mut now = 0.0;
    loop {
        let mut requests = Vec::new();
        for action in engine.prepared_actions(now) {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => timers.retain(|(_, t)| *t != timer),
                Action::Http(request) => requests.push(request),
                Action::Notice(Notice::StartupFailed { .. }) => failures += 1,
                _ => {}
            }
        }
        for request in requests {
            let body = [
                ("exchangeInfo", INFO),
                ("24hr", DAILY),
                ("bookTicker", BOOK_TICKERS),
            ]
            .into_iter()
            .find_map(|(path, body)| request.url.contains(path).then_some(body))
            .unwrap_or("{}");
            engine.handle(
                now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        if engine.has_actions() {
            continue;
        }
        timers.sort_by(|a, b| b.0.total_cmp(&a.0));
        let Some((at, timer)) = timers.pop() else {
            break;
        };
        if at > 600_000.0 {
            break;
        }
        now = at.max(now);
        engine.handle(now, Event::TimerFired { timer });
    }
    (upgrades.load(Ordering::Relaxed), failures)
}

#[test]
fn an_upgrade_whose_key_is_missing_is_tried_once_and_never_reconnects() {
    let (upgrades, failures) = upgrades_in_ten_minutes(SignError::MissingKey);
    assert_eq!(upgrades, 1, "the signed connect is never retried");
    assert_eq!(failures, 1, "one visible terminal failure");
}
