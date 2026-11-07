//! The candle cache, run as the `candle-cache-owner` child over its JSON-lines
//! protocol (the candle-cache crate's `src/owner.rs`). A separate process keeps
//! that crate's exact-number and ordered-map serde_json features out of the
//! engine build. The owner asks for each cache miss with an `upstream` line; the
//! request's own fetcher answers it.

use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    pin::Pin,
    process::Stdio,
    rc::Rc,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};

pub type Fetcher = Rc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<Value, String>>>>>;

pub struct FetchRequest {
    pub cache_key: String,
    pub interval: String,
    pub from_ms: f64,
    pub to_ms: f64,
    pub limit: f64,
    pub now_ms: f64,
}

struct Pending {
    fetcher: Fetcher,
    reply: oneshot::Sender<Result<Value, String>>,
}

pub struct CandleCache {
    lines: mpsc::UnboundedSender<String>,
    pending: RefCell<HashMap<u64, Pending>>,
    next: Cell<u64>,
}

impl CandleCache {
    /// Starts the owner and waits for its `ready` line. Any later exit ends this
    /// process: requests cannot be served without the cache.
    pub async fn spawn(executable: &std::path::Path) -> Result<Rc<Self>, String> {
        let mut child = tokio::process::Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                format!(
                    "could not start candle cache owner {}: {error}",
                    executable.display()
                )
            })?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (lines, mut outbox) = mpsc::unbounded_channel::<String>();
        tokio::task::spawn_local(async move {
            while let Some(mut line) = outbox.recv().await {
                line.push('\n');
                if let Err(error) = stdin.write_all(line.as_bytes()).await {
                    owner_failed(&format!("write failed: {error}"));
                }
            }
        });
        let mut reader = BufReader::new(stdout).lines();
        match reader.next_line().await {
            Ok(Some(line))
                if serde_json::from_str::<Value>(&line).ok() == Some(json!({"kind": "ready"})) => {}
            other => return Err(format!("candle cache owner did not start: {other:?}")),
        }
        let cache = Rc::new(Self {
            lines,
            pending: RefCell::new(HashMap::new()),
            next: Cell::new(0),
        });
        let owner = cache.clone();
        tokio::task::spawn_local(async move {
            loop {
                match reader.next_line().await {
                    Ok(Some(line)) => owner.receive(&line),
                    Ok(None) => owner_failed("stdout closed"),
                    Err(error) => owner_failed(&format!("read failed: {error}")),
                }
            }
        });
        tokio::task::spawn_local(async move {
            let status = child.wait().await;
            owner_failed(&format!("exited ({status:?})"));
        });
        Ok(cache)
    }

    fn receive(&self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            owner_failed("sent a malformed line");
        };
        match message["kind"].as_str() {
            Some("upstream") => {
                let id = message["id"].clone();
                let fetcher = message["requestId"].as_u64().and_then(|request| {
                    self.pending
                        .borrow()
                        .get(&request)
                        .map(|p| p.fetcher.clone())
                });
                let Some(fetcher) = fetcher else {
                    self.send(
                        json!({"kind": "upstreamResult", "id": id, "error": "Request cancelled"}),
                    );
                    return;
                };
                let lines = self.lines.clone();
                let options = message["options"].clone();
                tokio::task::spawn_local(async move {
                    let reply = match fetcher(options).await {
                        Ok(candles) => {
                            json!({"kind": "upstreamResult", "id": id, "candles": candles})
                        }
                        Err(error) => json!({"kind": "upstreamResult", "id": id, "error": error}),
                    };
                    let _ = lines.send(reply.to_string());
                });
            }
            Some("result") => {
                let Some(pending) = message["id"]
                    .as_u64()
                    .and_then(|id| self.pending.borrow_mut().remove(&id))
                else {
                    return;
                };
                let result = match message["error"].as_str() {
                    Some(error) => Err(error.to_owned()),
                    None => Ok(message["candles"].clone()),
                };
                let _ = pending.reply.send(result);
            }
            _ => owner_failed("sent an unknown message"),
        }
    }

    fn send(&self, message: Value) {
        let _ = self.lines.send(message.to_string());
    }

    /// `CandleCache.fetch`; the owner's error text when it or the fetcher failed.
    pub async fn fetch(&self, request: FetchRequest, fetcher: Fetcher) -> Result<Value, String> {
        let id = self.next.get();
        self.next.set(id + 1);
        let (reply, result) = oneshot::channel();
        self.pending
            .borrow_mut()
            .insert(id, Pending { fetcher, reply });
        // A dropped caller (client gone) releases its slot; the owner's late result is ignored.
        let guard = Forget { cache: self, id };
        self.send(json!({
            "kind": "fetch",
            "id": id,
            "request": {
                "cacheKey": request.cache_key,
                "interval": request.interval,
                "fromMs": request.from_ms,
                "toMs": request.to_ms,
                "limit": request.limit,
                "nowMs": request.now_ms,
            },
        }));
        let result = result
            .await
            .unwrap_or_else(|_| Err("Native cache request cancelled".into()));
        std::mem::forget(guard);
        result
    }
}

struct Forget<'a> {
    cache: &'a CandleCache,
    id: u64,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        self.cache.pending.borrow_mut().remove(&self.id);
    }
}

fn owner_failed(reason: &str) -> ! {
    eprintln!("[candle-server] candle cache owner {reason}; exiting");
    std::process::exit(1);
}
