//! Preconfigured Binance account: FRAME_SERVER_TOKEN is mandatory; optional
//! BINANCE_API_KEY / BINANCE_API_SECRET enable private commands. Bind defaults
//! to loopback. Run with `cargo run -p safe-cex-native --example frame-server`.
use safe_cex_engine::Engine;
use safe_cex_native::{FrameServer, Host, NetworkTransport, ServerConfig};
use safe_cex_venue_binance::{BinanceFutures, Options};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("FRAME_SERVER_TOKEN")?;
    let bind = std::env::var("FRAME_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:9001".into());
    let api_key = std::env::var("BINANCE_API_KEY").unwrap_or_default();
    let api_secret = std::env::var("BINANCE_API_SECRET").unwrap_or_default();
    let public_data_only = api_key.is_empty() && api_secret.is_empty();
    if api_key.is_empty() != api_secret.is_empty() {
        return Err("both Binance credentials are required".into());
    }
    let engine = Engine::new(BinanceFutures::new(Options {
        key: (!public_data_only).then_some(api_key),
        secret: (!public_data_only).then_some(api_secret),
        public_data_only,
        ..Options::default()
    }));
    tokio::task::LocalSet::new()
        .run_until(async move {
            let mut server = FrameServer::new(ServerConfig::default())?;
            server.add_account(
                "binance",
                &token,
                Host::new(engine, NetworkTransport::new()),
            )?;
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            eprintln!(
                "frame server listening on {} (account binance)",
                listener.local_addr()?
            );
            server
                .serve(listener, async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await
        })
        .await?;
    Ok(())
}
