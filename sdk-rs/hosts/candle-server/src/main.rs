//! Native candle server: the HTTP contract of NX
//! `mobile-services/candle-server/src/server.ts`, served directly at
//! `data.tealstreet.io/candles` (the retired `workers/data-candles` routing,
//! closed-range Cache-Control and public `verbose` stripping are folded in).
//! Upstream data comes from public-only native engines; candles go through the
//! `candle-cache-owner` Redis/memory cache.

mod cache;
mod calendar;
mod handlers;
mod health;
mod json;
mod pool;
mod routes;

use std::{future::Future, pin::Pin, rc::Rc};
use tokio::sync::{mpsc, oneshot};

/// One HTTP answer, built on the local thread that owns the engines.
pub struct Reply {
    pub status: u16,
    pub body: String,
    /// Seconds of public caching; `None` sends `no-store`.
    pub cache_ttl: Option<u64>,
}

impl Reply {
    pub fn json(status: u16, body: json::Json) -> Self {
        Self {
            status,
            body: body.render(),
            cache_ttl: None,
        }
    }
}

pub struct App {
    pub pool: pool::Pool,
    pub cache: Rc<cache::CandleCache>,
    pub redis: health::RedisHealth,
    pub started: std::time::Instant,
}

type LocalJob = Box<dyn FnOnce(Rc<App>) -> Pin<Box<dyn Future<Output = Reply>>> + Send>;

pub struct Job {
    run: LocalJob,
    reply: oneshot::Sender<Reply>,
}

/// Runs `job` against the local [`App`]; HTTP tasks are Send, the engines are not.
#[derive(Clone)]
pub struct Local(mpsc::UnboundedSender<Job>);

impl Local {
    pub async fn run(&self, run: LocalJob) -> Reply {
        let (reply, result) = oneshot::channel();
        if self.0.send(Job { run, reply }).is_err() {
            return routes::internal_error();
        }
        result.await.unwrap_or_else(|_| routes::internal_error())
    }
}

fn read_positive_int(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .map_or(fallback, |value| value.floor() as u64)
}

fn owner_executable() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("CANDLE_CACHE_OWNER_BIN")
        && !path.is_empty()
    {
        return path.into();
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("candle-cache-owner")))
        .unwrap_or_else(|| "candle-cache-owner".into())
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let local = tokio::task::LocalSet::new();
    if let Err(error) = local.block_on(&runtime, serve()) {
        eprintln!("[candle-server] {error}");
        std::process::exit(1);
    }
}

async fn serve() -> Result<(), String> {
    let port = std::env::var("TEALSTREET_CANDLE_PORT").unwrap_or_else(|_| "23457".into());
    let port: u16 = port
        .parse()
        .map_err(|_| format!("TEALSTREET_CANDLE_PORT is not a port: {port}"))?;
    let cache = cache::CandleCache::spawn(&owner_executable()).await?;
    let app = Rc::new(App {
        pool: pool::Pool::new(
            read_positive_int("CANDLE_MAX_RESIDENT_EXCHANGES", 8) as usize,
            read_positive_int("CANDLE_EXCHANGE_IDLE_TTL_MS", 600_000) as f64,
        ),
        cache,
        redis: health::RedisHealth::from_env(),
        started: std::time::Instant::now(),
    });
    let (jobs, mut inbox) = mpsc::unbounded_channel::<Job>();
    let worker = app.clone();
    tokio::task::spawn_local(async move {
        while let Some(job) = inbox.recv().await {
            let app = worker.clone();
            tokio::task::spawn_local(async move {
                let reply = (job.run)(app).await;
                let _ = job.reply.send(reply);
            });
        }
    });
    let sweeper = app.clone();
    tokio::task::spawn_local(async move {
        let mut tick = tokio::time::interval(pool::SWEEP_INTERVAL);
        loop {
            tick.tick().await;
            sweeper.pool.sweep_idle();
        }
    });
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|error| format!("could not listen on port {port}: {error}"))?;
    eprintln!("[candle-server] Listening on http://0.0.0.0:{port}");
    axum::serve(listener, routes::router(Local(jobs)))
        .with_graceful_shutdown(async {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            eprintln!("[candle-server] Shutting down...");
        })
        .await
        .map_err(|error| error.to_string())
}
