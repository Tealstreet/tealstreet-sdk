//! Counts of released upstream interests on public-only engines. Topic identities
//! stay inside this tracker; HTTP and journal output contain counts only.
mod legacy;
use crate::accounts::now_ms;
use safe_cex_engine::SocketId;
use safe_cex_engine::socket_lifecycle::{FeedId, InterestObservation, InterestObserver};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct PublicTopics(Mutex<State>);

#[derive(Default)]
struct State {
    next: u64,
    engines: HashMap<u64, EngineTopics>,
    venues: BTreeMap<String, Venue>,
}

struct EngineTopics {
    attached: bool,
    venue: String,
    topics: HashMap<(Source, String), Class>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Source {
    Managed(FeedId),
    Transport(SocketId),
}

struct Registration {
    tracker: Arc<PublicTopics>,
    id: u64,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.tracker.observe(self.id, InterestObservation::Dropped);
    }
}

#[derive(Default)]
struct Venue {
    engines: usize,
    books: HashMap<String, usize>,
    candles: HashMap<(String, String), usize>,
    other: usize,
    total: usize,
    peak_total: usize,
    peak_at: u64,
}

#[derive(Clone)]
enum Class {
    Book(String),
    Candle(String, String),
    Other,
}

fn classify(venue: &str, key: &str) -> Class {
    if venue == "hyperliquid" || venue == "hyperliquidspot" {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(key) else {
            return Class::Other;
        };
        let sub = &value["subscription"];
        if let Some(coin) = sub["coin"].as_str() {
            match sub["type"].as_str() {
                Some("l2Book" | "bbo") => return Class::Book(coin.into()),
                Some("candle") => {
                    if let Some(interval) = sub["interval"].as_str() {
                        return Class::Candle(coin.into(), interval.into());
                    }
                }
                _ => {}
            }
        }
    }
    Class::Other
}

fn change<K: std::hash::Hash + Eq>(map: &mut HashMap<K, usize>, key: K, added: bool) {
    if added {
        *map.entry(key).or_default() += 1;
    } else if let std::collections::hash_map::Entry::Occupied(mut entry) = map.entry(key) {
        *entry.get_mut() -= 1;
        if *entry.get() == 0 {
            entry.remove();
        }
    }
}

impl Venue {
    fn update(&mut self, class: Class, added: bool) {
        match class {
            Class::Book(symbol) => change(&mut self.books, symbol, added),
            Class::Candle(symbol, interval) => change(&mut self.candles, (symbol, interval), added),
            Class::Other => {
                if added {
                    self.other += 1;
                } else {
                    self.other -= 1;
                }
            }
        }
        if added {
            self.total += 1;
        } else {
            self.total -= 1;
        }
        if self.total > self.peak_total {
            self.peak_total = self.total;
            self.peak_at = now_ms();
        }
    }

    fn counts(&self) -> Counts {
        Counts {
            book: self.books.len(),
            candles: self.candles.len(),
            other: self.other,
            total: self.total,
            peak_total: self.peak_total,
            peak_at: self.peak_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub book: usize,
    pub candles: usize,
    pub other: usize,
    pub total: usize,
    pub peak_total: usize,
    pub peak_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub venues: BTreeMap<String, Counts>,
    pub checked_at: u64,
}

impl Snapshot {
    pub fn log_line(&self) -> String {
        let venues = self
            .venues
            .iter()
            .map(|(venue, c)| {
                format!(
                    "{venue} book={} candles={} other={} total={} peak={} peakAt={}",
                    c.book, c.candles, c.other, c.total, c.peak_total, c.peak_at,
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("public topics over 60s: {venues}")
    }
}

impl PublicTopics {
    /// Only attach to an engine built without credentials or a signer.
    pub fn observer(self: &Arc<Self>, venue: &str) -> InterestObserver {
        let id = {
            let mut state = self.0.lock().unwrap();
            state.next += 1;
            let id = state.next;
            state.venues.entry(venue.into()).or_default();
            state.engines.insert(
                id,
                EngineTopics {
                    attached: false,
                    venue: venue.into(),
                    topics: HashMap::new(),
                },
            );
            id
        };
        let registration = Registration {
            tracker: self.clone(),
            id,
        };
        InterestObserver::new(move |event| registration.tracker.observe(registration.id, event))
    }

    fn observe(&self, id: u64, event: InterestObservation<'_>) {
        let mut state = self.0.lock().unwrap();
        let State {
            engines, venues, ..
        } = &mut *state;
        let Some(engine) = engines.get_mut(&id) else {
            return;
        };
        let venue = venues.get_mut(&engine.venue).expect("registered venue");
        match event {
            InterestObservation::Attached => {
                if !engine.attached {
                    engine.attached = true;
                    venue.engines += 1;
                }
            }
            InterestObservation::Released {
                feed,
                subscribed,
                interest,
            } => {
                let key = (Source::Managed(feed), interest.key.to_string());
                if subscribed {
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        engine.topics.entry(key)
                    {
                        let class = classify(&engine.venue, &interest.key);
                        entry.insert(class.clone());
                        venue.update(class, true);
                    }
                } else if let Some(class) = engine.topics.remove(&key) {
                    venue.update(class, false);
                }
            }
            InterestObservation::Cleared { feed } => {
                engine.topics.retain(|(f, _), class| {
                    if *f != Source::Managed(feed) {
                        return true;
                    }
                    venue.update(class.clone(), false);
                    false
                });
            }
            InterestObservation::TransportSent { socket, text } => {
                for (topic, subscribed) in legacy::topics(&engine.venue, text) {
                    let key = (Source::Transport(socket), topic);
                    if subscribed {
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            engine.topics.entry(key)
                        {
                            entry.insert(Class::Other);
                            venue.update(Class::Other, true);
                        }
                    } else if let Some(class) = engine.topics.remove(&key) {
                        venue.update(class, false);
                    }
                }
            }
            InterestObservation::TransportClosed { socket } => {
                engine.topics.retain(|(source, _), class| {
                    if *source != Source::Transport(socket) {
                        return true;
                    }
                    venue.update(class.clone(), false);
                    false
                });
            }
            InterestObservation::Dropped => {
                for (_, class) in engine.topics.drain() {
                    venue.update(class, false);
                }
                if engine.attached {
                    venue.engines -= 1;
                }
                let name = engine.venue.clone();
                let unused = venue.engines == 0 && venue.peak_total == 0;
                engines.remove(&id);
                if unused {
                    venues.remove(&name);
                }
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.0.lock().unwrap();
        Snapshot {
            venues: state
                .venues
                .iter()
                .filter(|(_, v)| v.engines > 0)
                .map(|(name, venue)| (name.clone(), venue.counts()))
                .collect(),
            checked_at: now_ms(),
        }
    }
}

/// One server-owned task; each delay starts after the preceding observation.
pub async fn log_periodically(
    topics: Arc<PublicTopics>,
    closing: tokio::sync::watch::Receiver<bool>,
) {
    log_with(topics, closing, crate::state::Log::info).await;
}

async fn log_with(
    topics: Arc<PublicTopics>,
    mut closing: tokio::sync::watch::Receiver<bool>,
    mut emit: impl FnMut(&str),
) {
    loop {
        if *closing.borrow() {
            return;
        }
        tokio::select! {
            biased;
            _ = closing.changed() => return,
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                emit(&topics.snapshot().log_line());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_format_is_one_sorted_line_of_counts() {
        let snapshot = Snapshot {
            venues: BTreeMap::from([
                (
                    "hyperliquid".into(),
                    Counts {
                        book: 12,
                        candles: 7,
                        other: 4,
                        total: 35,
                        peak_total: 41,
                        peak_at: 123,
                    },
                ),
                (
                    "bybit".into(),
                    Counts {
                        book: 0,
                        candles: 0,
                        other: 3,
                        total: 3,
                        peak_total: 9,
                        peak_at: 100,
                    },
                ),
            ]),
            checked_at: 999,
        };
        assert_eq!(
            snapshot.log_line(),
            "public topics over 60s: bybit book=0 candles=0 other=3 total=3 peak=9 peakAt=100; hyperliquid book=12 candles=7 other=4 total=35 peak=41 peakAt=123"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn logger_reschedules_after_each_minute_and_stops_on_shutdown() {
        let topics = Arc::new(PublicTopics::default());
        let observer = topics.observer("hyperliquid");
        let lines = Arc::new(Mutex::new(Vec::new()));
        let output = lines.clone();
        let (closing, receiver) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(log_with(topics, receiver, move |line| {
            output.lock().unwrap().push(line.to_owned())
        }));
        tokio::task::yield_now().await;
        assert!(lines.lock().unwrap().is_empty());
        tokio::time::advance(Duration::from_secs(59)).await;
        tokio::task::yield_now().await;
        assert!(lines.lock().unwrap().is_empty());
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(lines.lock().unwrap().len(), 1);
        // A delayed turn logs once; there are no catch-up interval ticks.
        tokio::time::advance(Duration::from_secs(180)).await;
        tokio::task::yield_now().await;
        assert_eq!(lines.lock().unwrap().len(), 2);
        closing.send_replace(true);
        task.await.unwrap();
        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(lines.lock().unwrap().len(), 2);
        drop(observer);
    }

    #[test]
    fn legacy_topics_deduplicate_per_socket_and_clear_without_losing_peak() {
        let topics = Arc::new(PublicTopics::default());
        let observer = topics.observer("lighter");
        observer.observe(InterestObservation::Attached);
        for socket in [SocketId(1), SocketId(2)] {
            for _ in 0..2 {
                observer.observe(InterestObservation::TransportSent {
                    socket,
                    text: r#"{"type":"subscribe","channel":"market_stats/all"}"#,
                });
            }
        }
        assert_eq!(topics.snapshot().venues["lighter"].total, 2);
        observer.observe(InterestObservation::TransportSent {
            socket: SocketId(1),
            text: r#"{"type":"unsubscribe","channel":"market_stats/all"}"#,
        });
        observer.observe(InterestObservation::TransportClosed {
            socket: SocketId(2),
        });
        let c = &topics.snapshot().venues["lighter"];
        assert_eq!((c.other, c.total, c.peak_total), (0, 0, 2));
    }
}
