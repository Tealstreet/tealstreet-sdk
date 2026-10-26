//! Watch Binance USD-M live through the native host:
//!
//!   cargo run -p safe-cex-native --example watch -- BTCUSDT ETHUSDT
use safe_cex_engine::wire::{book_top, section, sections};
use safe_cex_engine::{Command, Engine, Notice};
use safe_cex_native::{Host, NetworkTransport};
use safe_cex_venue_binance::{BinanceFutures, Options};
use std::time::{Duration, Instant};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let symbols: Vec<String> = std::env::args().skip(1).collect();
    let engine = Engine::new(
        BinanceFutures::new(Options {
            public_data_only: true,
            ..Options::default()
        }),
        Some(safe_cex_native::production_entropy()),
    )
    .expect("production entropy configured");
    let (commands, inbox) = tokio::sync::mpsc::unbounded_channel();
    for symbol in &symbols {
        let _ = commands.send(Command::SubscribeOrderBook {
            symbol: symbol.clone(),
        });
    }
    let started = Instant::now();
    let mut frames = 0u64;
    let mut bytes = 0usize;
    let mut last_report = Instant::now();
    let host = Host::new(engine, NetworkTransport::new());
    let run = host.run(
        inbox,
        Duration::from_millis(250),
        |frame| {
            frames += 1;
            bytes += frame.len();
            if last_report.elapsed() < Duration::from_secs(1) {
                return;
            }
            last_report = Instant::now();
            for (kind, payload) in sections(frame) {
                if kind == section::BOOK
                    && let Some((market, bid, ask)) = book_top(payload)
                {
                    println!("market {market}: {bid} / {ask}");
                }
            }
            println!(
                "{:.0}s: {frames} frames, {:.1} KB/s",
                started.elapsed().as_secs_f64(),
                bytes as f64 / 1024.0 / started.elapsed().as_secs_f64()
            );
        },
        |notice| {
            if let Notice::Event(event) = notice {
                eprintln!("[{:?}] {}: {}", event.level, event.code, event.message);
            }
        },
    );
    // Keep `commands` alive for the session; Ctrl-C ends it.
    tokio::select! {
        _ = run => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    drop(commands);
}
