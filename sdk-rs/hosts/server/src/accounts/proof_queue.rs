use super::InitPayload;
use crate::delegated::probe::ProbeRefusal;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const BACKLOG: usize = 4;
type Queues = Arc<Mutex<HashMap<[u8; 32], QueueEntry>>>;

#[derive(Default)]
pub(super) struct ProofQueues(Queues);

struct QueueEntry {
    queue: Arc<Queue>,
    reservations: usize,
}

struct Queue {
    capacity: Arc<Semaphore>,
    active: Arc<Semaphore>,
}

pub(super) struct Reservation {
    queues: Queues,
    identity: [u8; 32],
    #[cfg(test)]
    retirement_pause: Option<tests::RetirementPause>,
    queue: Arc<Queue>,
    _capacity: OwnedSemaphorePermit,
}

pub(super) struct ActiveProof {
    _active: OwnedSemaphorePermit,
    _reservation: Reservation,
}

impl ProofQueues {
    pub(super) fn reserve(
        &self,
        user: &str,
        init: &InitPayload,
    ) -> Result<Reservation, ProbeRefusal> {
        let identity = identity(user, init);
        let mut queues = self.0.lock().unwrap();
        let entry = queues.entry(identity).or_insert_with(|| QueueEntry {
            queue: Arc::new(Queue {
                capacity: Arc::new(Semaphore::new(1 + BACKLOG)),
                active: Arc::new(Semaphore::new(1)),
            }),
            reservations: 0,
        });
        let capacity = entry
            .queue
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| ProbeRefusal::TRANSIENT)?;
        entry.reservations += 1;
        Ok(Reservation {
            queues: self.0.clone(),
            identity,
            #[cfg(test)]
            retirement_pause: None,
            queue: entry.queue.clone(),
            _capacity: capacity,
        })
    }
}

impl Reservation {
    pub(super) async fn start(self) -> Result<ActiveProof, ProbeRefusal> {
        let active = self
            .queue
            .active
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ProbeRefusal::TRANSIENT)?;
        Ok(ActiveProof {
            _active: active,
            _reservation: self,
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut queues = self.queues.lock().unwrap();
        // Retire under the admission lock: concurrent destructors can still hold
        // queue Arcs after this lock is released, without owning live work.
        let entry = queues.get_mut(&self.identity).unwrap();
        entry.reservations -= 1;
        if entry.reservations == 0 {
            queues.remove(&self.identity);
        }
    }
}

fn identity(user: &str, init: &InitPayload) -> [u8; 32] {
    let venue = init.exchange_name.as_str();
    let family = match venue {
        "bybit" | "bybitspot" | "bybitinverse" | "bybitdemo" => "bybit",
        "binance" | "binancespot" | "binanceinverse" | "binancepm" | "binanceoptions" => "binance",
        "okx" | "okxeu" | "okxdemo" | "okxeudemo" => "okx",
        "bitget" | "bitgetinverse" | "bitgetusdc" => "bitget",
        "kucoin" | "kucoinspot" => "kucoin",
        "hyperliquid" | "hyperliquidspot" => "hyperliquid",
        "coinbase" | "coinbasespot" => "coinbase",
        _ => venue,
    };
    let network = if family == "okx" && (venue.contains("demo") || init.testnet == Some(true)) {
        "demo"
    } else if init.testnet == Some(true) {
        "testnet"
    } else if venue == "bybitdemo" {
        "demo"
    } else {
        "mainnet"
    };
    let site = if family == "bybit" && init.bybit_site_id.as_deref() == Some("BRA_BTL") {
        "br"
    } else if matches!(venue, "okxeu" | "okxeudemo") {
        "eu"
    } else {
        "global"
    };
    let trim = safe_cex_venues::signer_setup_policy(venue)
        .primary
        .and_then(|policy| policy.trim_credential)
        == Some(true);
    let key = if trim { init.key.trim() } else { &init.key };
    // Queue identity excludes labels, signing mode, routes, settings and secrets.
    let mut bytes = b"tealstreet/proof-queue/v1".to_vec();
    for field in [user, family, network, site, key] {
        bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    safe_cex_crypto::sha256(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    // Fields drop after Reservation::drop unlocks. Pause before the queue Arc
    // drops to select the concurrent final-retirement interleaving.
    pub(super) struct RetirementPause(Arc<Barrier>);

    impl Drop for RetirementPause {
        fn drop(&mut self) {
            self.0.wait();
        }
    }

    fn init() -> InitPayload {
        InitPayload {
            exchange_name: "bybit".into(),
            key: "synthetic-queue-key".into(),
            ..Default::default()
        }
    }

    fn retire_pair(queues: &ProofQueues) {
        let pause = Arc::new(Barrier::new(2));
        let mut first = queues.reserve("synthetic-user", &init()).unwrap();
        let mut second = queues.reserve("synthetic-user", &init()).unwrap();
        first.retirement_pause = Some(RetirementPause(pause.clone()));
        second.retirement_pause = Some(RetirementPause(pause));
        std::thread::scope(|scope| {
            scope.spawn(move || drop(first));
            scope.spawn(move || drop(second));
        });
    }

    #[test]
    fn sequential_retirement_removes_idle_queue() {
        let queues = ProofQueues::default();
        let first = queues.reserve("synthetic-user", &init()).unwrap();
        let second = queues.reserve("synthetic-user", &init()).unwrap();
        drop(first);
        assert_eq!(queues.0.lock().unwrap().len(), 1);
        drop(second);
        assert!(queues.0.lock().unwrap().is_empty());
    }

    #[test]
    fn concurrent_final_retirement_removes_idle_queue() {
        let queues = ProofQueues::default();
        retire_pair(&queues);
        assert!(
            queues.0.lock().unwrap().is_empty(),
            "the final concurrent retirements must remove the idle queue"
        );
    }

    #[test]
    fn concurrent_retirement_preserves_a_live_reservation() {
        let queues = ProofQueues::default();
        let remaining = queues.reserve("synthetic-user", &init()).unwrap();
        retire_pair(&queues);
        assert_eq!(queues.0.lock().unwrap().len(), 1);
        let mut waiting = Vec::new();
        for _ in 0..BACKLOG {
            waiting.push(queues.reserve("synthetic-user", &init()).unwrap());
        }
        assert!(queues.reserve("synthetic-user", &init()).is_err());
        drop(waiting);
        assert_eq!(queues.0.lock().unwrap().len(), 1);
        drop(remaining);
        assert!(queues.0.lock().unwrap().is_empty());
    }
}
