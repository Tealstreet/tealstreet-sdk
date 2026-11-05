use safe_cex_exchange_server::accounts::Registry;
use safe_cex_exchange_server::auth::{PROXY_PUBLIC_KEY, Verifier};
use safe_cex_exchange_server::config::Config;
use safe_cex_exchange_server::http::{App, drain, router};
use safe_cex_exchange_server::state::{Log, Metrics};
use safe_cex_exchange_server::threads::EngineThreads;
use std::net::SocketAddr;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    // A malformed proxy or tunnel override refuses here, not on the first account.
    let bases = safe_cex_native::resolve::route_bases()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let threads = EngineThreads::start(config.engine_threads)?;
        let registry = Arc::new(
            Registry::new(threads, config.account_flush, config.resume_grace)
                .with_router_lease(config.router_lease.clone()),
        );
        let app = Arc::new(App {
            registry,
            metrics: Arc::new(Metrics::default()),
            verifier: Arc::new(Verifier::new(PROXY_PUBLIC_KEY)?),
            config: config.clone(),
            started: std::time::Instant::now(),
            public_per_ip: Default::default(),
            closing: tokio::sync::watch::channel(false).0,
        });
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
        Log::info(&format!("route bases: {bases}"));
        Log::info(&format!("listening on {}", config.port));
        let draining = app.clone();
        axum::serve(
            listener,
            router(app.clone()).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {
                _ = terminate.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            Log::info("draining");
            drain(&draining).await;
        })
        .await?;
        Ok(())
    })
}
