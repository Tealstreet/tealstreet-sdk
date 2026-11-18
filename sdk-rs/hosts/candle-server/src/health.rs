//! `/health`'s Redis probe, after NX `candle-server/src/cache.ts`
//! `checkCandleCacheRedis`: a SET/GET/DEL round trip under one 3 s deadline.
//! `reachable` is what pages: an outright connection failure is unreachable at
//! once, a missed deadline only after three probes in a row.

use redis::aio::MultiplexedConnection;
use std::{
    cell::{Cell, RefCell},
    time::Duration,
};
use tokio::time::Instant;

const TIMEOUT: Duration = Duration::from_secs(3);
const UNREACHABLE_AFTER_FAILURES: u32 = 3;

pub struct RedisStatus {
    pub configured: bool,
    pub connected: bool,
    pub reachable: bool,
    pub kind: Option<&'static str>,
    pub error: Option<String>,
}

pub struct RedisHealth {
    client: Option<Result<redis::Client, String>>,
    connection: RefCell<Option<MultiplexedConnection>>,
    failures: Cell<u32>,
    probes: Cell<u64>,
}

/// Never let REDIS_URL credentials reach a response or a log line.
fn redact(message: &str) -> String {
    let mut out = String::new();
    let mut rest = message;
    while let Some(start) = rest.find("redis") {
        let (head, tail) = rest.split_at(start);
        out.push_str(head);
        let scheme = if tail.starts_with("rediss://") {
            "rediss://"
        } else if tail.starts_with("redis://") {
            "redis://"
        } else {
            out.push_str("redis");
            rest = &tail[5..];
            continue;
        };
        let after = &tail[scheme.len()..];
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        match after[..end].rfind('@') {
            Some(at) => {
                out.push_str(scheme);
                out.push_str("***");
                rest = &after[at..];
            }
            None => {
                out.push_str(scheme);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

impl RedisHealth {
    pub fn from_env() -> Self {
        let client = std::env::var("REDIS_URL")
            .ok()
            .filter(|url| !url.is_empty())
            .map(|url| redis::Client::open(url).map_err(|error| redact(&error.to_string())));
        Self {
            client,
            connection: RefCell::new(None),
            failures: Cell::new(0),
            probes: Cell::new(0),
        }
    }

    fn fail(&self, kind: &'static str, error: String) -> RedisStatus {
        self.failures.set(self.failures.get() + 1);
        *self.connection.borrow_mut() = None;
        eprintln!("[candle-server] Redis cache health failed reason={error}");
        RedisStatus {
            configured: true,
            connected: false,
            reachable: kind != "unreachable" && self.failures.get() < UNREACHABLE_AFTER_FAILURES,
            kind: Some(kind),
            error: Some(error),
        }
    }

    pub async fn probe(&self) -> RedisStatus {
        let client = match &self.client {
            None => {
                return RedisStatus {
                    configured: false,
                    connected: false,
                    reachable: false,
                    kind: None,
                    error: None,
                };
            }
            Some(Err(error)) => return self.fail("unreachable", error.clone()),
            Some(Ok(client)) => client,
        };
        let deadline = Instant::now() + TIMEOUT;
        let cached = self.connection.borrow().clone();
        let mut connection = match cached {
            Some(connection) => connection,
            None => {
                match tokio::time::timeout_at(deadline, client.get_multiplexed_async_connection())
                    .await
                {
                    Ok(Ok(connection)) => {
                        *self.connection.borrow_mut() = Some(connection.clone());
                        connection
                    }
                    Ok(Err(error)) => return self.fail("unreachable", redact(&error.to_string())),
                    Err(_) => {
                        return self.fail(
                            "unreachable",
                            format!("Redis did not become ready after {}ms", TIMEOUT.as_millis()),
                        );
                    }
                }
            }
        };
        self.probes.set(self.probes.get() + 1);
        let key = format!(
            "tealstreet:candle-server:health:{}-{}-{}",
            std::process::id(),
            crate::pool::now_ms(),
            self.probes.get()
        );
        let value = crate::pool::now_ms().to_string();
        let round_trip = async {
            redis::cmd("SET")
                .arg(&key)
                .arg(&value)
                .arg("PX")
                .arg(TIMEOUT.as_millis().max(1000) as u64)
                .query_async::<()>(&mut connection)
                .await?;
            let observed: Option<String> = redis::cmd("GET")
                .arg(&key)
                .query_async(&mut connection)
                .await?;
            redis::cmd("DEL")
                .arg(&key)
                .query_async::<i64>(&mut connection)
                .await?;
            Ok::<_, redis::RedisError>(observed)
        };
        match tokio::time::timeout_at(deadline, round_trip).await {
            Ok(Ok(observed)) if observed.as_deref() == Some(value.as_str()) => {
                self.failures.set(0);
                RedisStatus {
                    configured: true,
                    connected: true,
                    reachable: true,
                    kind: None,
                    error: None,
                }
            }
            Ok(Ok(_)) => self.fail("mismatch", "Redis health readback mismatch".into()),
            Ok(Err(error)) => {
                // A socket-level failure is unambiguous; anything else escalates.
                let kind = if error.is_io_error()
                    || error.is_connection_dropped()
                    || error.is_connection_refusal()
                {
                    "unreachable"
                } else {
                    "slow"
                };
                self.fail(kind, redact(&error.to_string()))
            }
            Err(_) => self.fail(
                "slow",
                format!("Redis health timed out after {}ms", TIMEOUT.as_millis()),
            ),
        }
    }
}

/// Resident set size in MB (Linux; 0 elsewhere).
pub fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<f64>().ok())
        .map_or(0., |pages| (pages * 4096. / 1024. / 1024.).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_never_survive_redaction() {
        assert_eq!(
            redact("connect redis://user:secret@host:6379 failed"),
            "connect redis://***@host:6379 failed"
        );
        assert_eq!(redact("rediss://:pw@h:1"), "rediss://***@h:1");
        assert_eq!(redact("redis://host:6379"), "redis://host:6379");
        assert_eq!(redact("redisReachable"), "redisReachable");
    }

    #[test]
    fn an_unconfigured_redis_is_neither_connected_nor_reachable() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let health = RedisHealth {
            client: None,
            connection: RefCell::new(None),
            failures: Cell::new(0),
            probes: Cell::new(0),
        };
        let status = runtime.block_on(health.probe());
        assert!(!status.configured && !status.connected && !status.reachable);
    }

    #[test]
    fn a_refused_connection_is_unreachable_at_once() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let health = RedisHealth {
            client: Some(Ok(redis::Client::open("redis://127.0.0.1:1").unwrap())),
            connection: RefCell::new(None),
            failures: Cell::new(0),
            probes: Cell::new(0),
        };
        let status = runtime.block_on(health.probe());
        assert_eq!(
            (status.configured, status.reachable, status.kind),
            (true, false, Some("unreachable"))
        );
    }
}
