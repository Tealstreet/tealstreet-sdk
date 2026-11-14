//! Native writer fixtures consumed by the slice's real packed-frame regression.
pub use safe_cex_engine::{model, wire};
#[path = "../src/balance_output.rs"]
mod balance_output;
use model::{Balance, CoinBalance, CurrencyBalance};
use wire::{FrameWriter, section};

fn frame(
    balance: Option<&Balance>,
    coins: Option<&[CoinBalance]>,
    currencies: Option<&[CurrencyBalance]>,
    ready: bool,
) -> Vec<u8> {
    let mut w = FrameWriter::new(Vec::new(), 1_700_000_000_000.0);
    if let Some(balance) = balance {
        balance_output::write(&mut w, balance, coins, currencies);
    }
    w.begin(section::STATUS);
    w.u32(u32::from(ready));
    w.u32(0);
    w.f64s([0.0]);
    w.end();
    w.finish()
}
fn main() {
    let zero = Balance::default();
    let rich = Balance {
        used: 11.25,
        free: 22.5,
        total: 33.75,
        upnl: -4.5,
    };
    let coins: Vec<_> = [
        ("BTC", 0.0, 0.0),
        ("10", 3.0, 4.0),
        ("2", 5.0, 6.0),
        ("__proto__", 7.0, 8.0),
    ]
    .into_iter()
    .map(|(base, amount, notional)| CoinBalance {
        base: base.into(),
        amount,
        notional,
    })
    .collect();
    let currencies: Vec<_> = ["USD", "10", "2", "__proto__"]
        .into_iter()
        .enumerate()
        .map(|(i, currency)| CurrencyBalance {
            currency: currency.into(),
            balance: Balance {
                used: 13.0 + i as f64,
                free: 17.0,
                total: 19.0,
                upnl: -23.0,
            },
        })
        .collect();
    let rich_frame = frame(Some(&rich), Some(&coins), Some(&currencies), true);
    let mut store = safe_cex_engine::store::Store::default();
    store.set_balance(rich.clone());
    store.set_coin_balances(coins.clone());
    store.set_currency_balances(currencies.clone());
    let mut legacy = FrameWriter::new(Vec::new(), 1_700_000_000_000.0);
    store.write(&store.everything(), &mut legacy);
    let legacy = legacy.finish();
    for kind in [section::BALANCE, section::STRINGS] {
        assert_eq!(
            wire::sections(&rich_frame)
                .find(|(k, _)| *k == kind)
                .unwrap()
                .1,
            wire::sections(&legacy).find(|(k, _)| *k == kind).unwrap().1,
            "extraction changes section {kind}"
        );
    }
    let nonfinite = Balance {
        used: f64::NAN,
        free: f64::INFINITY,
        total: f64::NEG_INFINITY,
        upnl: -0.0,
    };
    let cases = [
        ("absent", frame(None, None, None, false)),
        ("unready", frame(Some(&zero), None, None, false)),
        ("zero", frame(Some(&zero), None, None, true)),
        ("rich", rich_frame),
        ("empty-maps", frame(Some(&zero), Some(&[]), Some(&[]), true)),
        ("empty-coins", frame(Some(&zero), Some(&[]), None, true)),
        (
            "currencies-only",
            frame(Some(&rich), None, Some(&currencies), true),
        ),
        ("nonfinite", frame(Some(&nonfinite), None, None, true)),
    ];
    let output: Vec<_> = cases
        .into_iter()
        .map(|(name, bytes)| serde_json::json!({"name": name, "bytes": bytes}))
        .collect();
    println!("{}", serde_json::to_string(&output).unwrap());
}
