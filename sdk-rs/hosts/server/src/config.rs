//! The environment surface; see DESIGN.md "Environment".
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub region: Option<String>,
    pub release: Option<String>,
    pub engine_threads: usize,
    pub max_connections: usize,
    /// Unauthenticated public-data sockets per client IP.
    pub max_public_per_ip: usize,
    pub idle_timeout: Duration,
    pub resume_grace: Duration,
    pub account_flush: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        let number = |name: &str, default: usize| -> Result<usize, String> {
            var(name).map_or(Ok(default), |value| {
                value
                    .parse()
                    .ok()
                    .filter(|parsed| *parsed > 0)
                    .ok_or(format!("{name} must be a positive integer"))
            })
        };
        let threads = std::thread::available_parallelism().map_or(2, usize::from);
        Ok(Self {
            port: u16::try_from(number("TEALSTREET_PORT", 23456)?)
                .map_err(|_| "TEALSTREET_PORT is out of range".to_owned())?,
            region: var("CONNECT_REGION"),
            release: var("TEALSTREET_RELEASE"),
            engine_threads: number("TEALSTREET_ENGINE_THREADS", threads)?,
            max_connections: number("TEALSTREET_MAX_CONNECTIONS", 4096)?,
            max_public_per_ip: 8,
            idle_timeout: Duration::from_secs(30),
            resume_grace: Duration::from_secs(60),
            account_flush: Duration::from_millis(60),
        })
    }
}
