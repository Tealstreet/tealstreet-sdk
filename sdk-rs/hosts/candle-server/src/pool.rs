//! Bounded pool of public, REST-only native engines, after the TS
//! `exchange-pool.ts`: one resident per venue, LRU past the cap, an idle sweep,
//! and in-flight requests holding a resident against eviction.
//!
//! Engines run public-only with the public feed disabled, so no venue socket is
//! opened; startup still loads markets, which many venues need to resolve a
//! candle instrument.

use safe_cex_engine::model::Market;
use safe_cex_engine::wire::{section, sections};
use safe_cex_engine::{Command, Engine, Notice, Venue, log::Level};
use safe_cex_native::{Host, HostEvent, NetworkTransport};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};

const STARTUP_WAIT: Duration = Duration::from_secs(30);
const FLUSH_EVERY: Duration = Duration::from_millis(50);
const MARKETS_RELOAD_MIN_INTERVAL_MS: f64 = 60_000.;
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// Candle command deadline; the engine's own reads time out well inside it.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

pub fn now_ms() -> f64 {
    safe_cex_native::now_ms()
}

/// The owner spells integers as floats (`5000.0`); JSON from JS never does,
/// and the command decoder's integer fields refuse them.
fn js_integer(value: &Value) -> Value {
    match value.as_f64() {
        Some(number) if number.fract() == 0. && number.abs() <= 9_007_199_254_740_991. => {
            Value::from(number as i64)
        }
        _ => value.clone(),
    }
}

/// What the engine reports back, shared with its host's callbacks.
#[derive(Default)]
struct Shared {
    markets: RefCell<Rc<Vec<Market>>>,
    markets_loaded: Cell<bool>,
    error_seq: Cell<u64>,
    last_error: RefCell<Option<String>>,
    pending: RefCell<HashMap<u32, oneshot::Sender<Result<String, String>>>>,
    frames: RefCell<Option<watch::Sender<u64>>>,
}

impl Shared {
    fn on_frame(&self, frame: &[u8], store: &safe_cex_engine::Store) {
        if sections(frame).any(|(kind, _)| kind == section::MARKETS) {
            *self.markets.borrow_mut() = Rc::new(store.markets().to_vec());
        }
        self.markets_loaded.set(store.loaded().markets);
        if let Some(frames) = &*self.frames.borrow() {
            frames.send_modify(|count| *count += 1);
        }
    }

    fn on_notice(&self, notice: Notice) {
        match notice {
            Notice::CommandDone { command, result } => self.settle(command, Ok(result)),
            Notice::CommandFailed { command, error } => self.settle(command, Err(error.message)),
            // An error-level event is the venue's failure signal, like the TS
            // emitter 'error' channel the pool latches.
            Notice::Event(event) if event.level == Level::Error => {
                self.error_seq.set(self.error_seq.get() + 1);
                eprintln!(
                    "[candle-server] exchange reported an error code={} message={}",
                    event.code, event.message
                );
                *self.last_error.borrow_mut() = Some(event.message);
            }
            Notice::StartupFailed { error } => {
                self.error_seq.set(self.error_seq.get() + 1);
                eprintln!("[candle-server] exchange startup failed: {}", error.message);
                *self.last_error.borrow_mut() = Some(error.message);
            }
            _ => {}
        }
    }

    fn settle(&self, command: u32, result: Result<String, String>) {
        if let Some(reply) = self.pending.borrow_mut().remove(&command) {
            let _ = reply.send(result);
        }
    }
}

pub struct Resident {
    pub name: String,
    shared: Rc<Shared>,
    commands: mpsc::UnboundedSender<Command>,
    task: tokio::task::JoinHandle<()>,
    frames: watch::Receiver<u64>,
    next_command: Cell<u32>,
    in_flight: Cell<u32>,
    last_used: Cell<f64>,
    last_markets_load: Cell<f64>,
    settled: watch::Receiver<bool>,
}

impl Resident {
    pub fn markets(&self) -> Rc<Vec<Market>> {
        self.shared.markets.borrow().clone()
    }

    pub fn markets_loaded(&self) -> bool {
        self.shared.markets_loaded.get()
    }

    pub fn error_seq(&self) -> u64 {
        self.shared.error_seq.get()
    }

    pub fn last_error(&self) -> Option<String> {
        self.shared.last_error.borrow().clone()
    }

    pub fn knows_symbol(&self, symbol: &str) -> bool {
        self.markets().iter().any(|market| market.symbol == symbol)
    }

    async fn command(&self, mut json: serde_json::Map<String, Value>) -> Result<String, String> {
        let id = self.next_command.get();
        self.next_command.set(id.wrapping_add(1).max(1));
        json.insert("command".into(), id.into());
        let command = safe_cex_engine::command::decode_command(&Value::Object(json).to_string())
            .map_err(|error| error.to_string())?;
        let (reply, result) = oneshot::channel();
        self.shared.pending.borrow_mut().insert(id, reply);
        if self.commands.send(command).is_err() {
            self.shared.pending.borrow_mut().remove(&id);
            return Err("Exchange engine stopped".into());
        }
        match tokio::time::timeout(COMMAND_TIMEOUT, result).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("Exchange engine stopped".into()),
            Err(_) => {
                self.shared.pending.borrow_mut().remove(&id);
                Err("Exchange command timed out".into())
            }
        }
    }

    /// `fetchOHLCV({symbol, interval, from, to, limit})` with the cache's options.
    pub async fn fetch_candles(&self, symbol: &str, options: &Value) -> Result<Value, String> {
        let mut json = serde_json::Map::new();
        json.insert("op".into(), "fetchCandles".into());
        json.insert("symbol".into(), symbol.into());
        for field in ["interval", "limit", "from", "to"] {
            if let Some(value) = options.get(field).filter(|value| !value.is_null()) {
                json.insert(field.into(), js_integer(value));
            }
        }
        let result = self.command(json).await?;
        serde_json::from_str(&result).map_err(|error| error.to_string())
    }

    /// Re-read markets at most once a minute; resolves to whether they are loaded.
    pub async fn reload_markets(&self) -> bool {
        if now_ms() - self.last_markets_load.get() < MARKETS_RELOAD_MIN_INTERVAL_MS {
            return self.markets_loaded();
        }
        self.last_markets_load.set(now_ms());
        let mut frames = self.frames.clone();
        frames.mark_unchanged();
        let mut json = serde_json::Map::new();
        json.insert("op".into(), "refreshMarkets".into());
        match self.command(json).await {
            Ok(_) => {
                // The refreshed catalogue reaches us with the next flush frame.
                let _ = tokio::time::timeout(FLUSH_EVERY * 4, frames.changed()).await;
            }
            Err(error) => {
                eprintln!(
                    "[candle-server] {} market reload failed: {error}",
                    self.name
                );
                self.shared.markets_loaded.set(false);
            }
        }
        self.markets_loaded()
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Launch {
    shared: Rc<Shared>,
}

impl safe_cex_venues::Visit for Launch {
    type Output = (mpsc::UnboundedSender<Command>, tokio::task::JoinHandle<()>);

    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output {
        let (commands, inbox) = mpsc::unbounded_channel();
        let host = Host::new(engine, NetworkTransport::new());
        // Historical REST only: optional polling (tickers, reconciliation) stays paused.
        let _ = host
            .event_sender()
            .send(HostEvent::PollScaleChanged(f64::INFINITY));
        let frames = self.shared.clone();
        let notices = self.shared;
        let task = tokio::task::spawn_local(async move {
            host.run_with_store(
                inbox,
                FLUSH_EVERY,
                move |frame, store| frames.on_frame(frame, store),
                move |notice| notices.on_notice(notice),
            )
            .await;
        });
        (commands, task)
    }
}

/// A request's hold on a resident; eviction skips it until dropped.
pub struct Held(Rc<Resident>);

impl std::ops::Deref for Held {
    type Target = Resident;
    fn deref(&self) -> &Resident {
        &self.0
    }
}

impl Held {
    /// The resident itself, for callbacks that run inside this hold.
    pub fn engine(&self) -> Rc<Resident> {
        self.0.clone()
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.0.in_flight.set(self.0.in_flight.get() - 1);
        self.0.last_used.set(now_ms());
    }
}

pub struct Pool {
    residents: RefCell<HashMap<String, Rc<Resident>>>,
    pub max_resident: usize,
    pub idle_ttl_ms: f64,
}

impl Pool {
    pub fn new(max_resident: usize, idle_ttl_ms: f64) -> Self {
        Self {
            residents: RefCell::new(HashMap::new()),
            max_resident,
            idle_ttl_ms,
        }
    }

    pub fn size(&self) -> usize {
        self.residents.borrow().len()
    }

    /// The venue's resident, created (and its markets awaited) on first use.
    pub async fn hold(&self, venue: &str) -> Result<Held, String> {
        let existing = self.residents.borrow().get(venue).cloned();
        let resident = match existing {
            Some(resident) => resident,
            None => self.create(venue)?,
        };
        resident.in_flight.set(resident.in_flight.get() + 1);
        resident.last_used.set(now_ms());
        let held = Held(resident);
        let mut settled = held.settled.clone();
        let _ = tokio::time::timeout(STARTUP_WAIT, settled.wait_for(|done| *done)).await;
        Ok(held)
    }

    fn create(&self, venue: &str) -> Result<Rc<Resident>, String> {
        let options: safe_cex_venues::EngineOptions = serde_json::from_str(
            r#"{"publicDataOnly":true,"publicFeed":{"mode":"disabled"},"logLevel":"warn"}"#,
        )
        .map_err(|error| error.to_string())?;
        let shared = Rc::new(Shared::default());
        let (frame_tx, frames) = watch::channel(0_u64);
        *shared.frames.borrow_mut() = Some(frame_tx);
        let (commands, task) = safe_cex_venues::build(
            venue,
            options,
            Some(safe_cex_native::production_entropy()),
            Launch {
                shared: shared.clone(),
            },
        )
        .map_err(|error| error.message)?;
        let (settle, settled) = watch::channel(false);
        let watcher = shared.clone();
        let mut ready = frames.clone();
        let name = venue.to_owned();
        tokio::task::spawn_local(async move {
            let errors = watcher.error_seq.get();
            let _ = tokio::time::timeout(STARTUP_WAIT, async {
                while !watcher.markets_loaded.get() && watcher.error_seq.get() == errors {
                    if ready.changed().await.is_err() {
                        return;
                    }
                }
            })
            .await;
            if !watcher.markets_loaded.get() {
                eprintln!(
                    "[candle-server] could not load markets, symbol precheck disabled for this venue exchange={name}"
                );
            }
            let _ = settle.send(true);
        });
        let now = now_ms();
        let resident = Rc::new(Resident {
            name: venue.to_owned(),
            shared,
            commands,
            task,
            frames,
            next_command: Cell::new(1),
            in_flight: Cell::new(0),
            last_used: Cell::new(now),
            last_markets_load: Cell::new(now),
            settled,
        });
        self.residents
            .borrow_mut()
            .insert(venue.to_owned(), resident.clone());
        self.enforce_cap(&resident);
        Ok(resident)
    }

    fn enforce_cap(&self, exempt: &Rc<Resident>) {
        let mut residents = self.residents.borrow_mut();
        while residents.len() > self.max_resident {
            // Every resident busy: overshoot the cap rather than block or dispose one.
            let Some(victim) = residents
                .values()
                .filter(|resident| !Rc::ptr_eq(resident, exempt) && resident.in_flight.get() == 0)
                .min_by(|a, b| a.last_used.get().total_cmp(&b.last_used.get()))
                .map(|resident| resident.name.clone())
            else {
                return;
            };
            residents.remove(&victim);
        }
    }

    pub fn sweep_idle(&self) {
        let now = now_ms();
        self.residents.borrow_mut().retain(|_, resident| {
            resident.in_flight.get() > 0 || now - resident.last_used.get() < self.idle_ttl_ms
        });
    }
}
