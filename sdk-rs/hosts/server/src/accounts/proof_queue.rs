use super::InitPayload;
use crate::delegated::probe::ProbeRefusal;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const BACKLOG: usize = 4;
type Queues = Arc<Mutex<HashMap<[u8; 32], Arc<Queue>>>>;

#[derive(Default)]
pub(super) struct ProofQueues(Queues);

struct Queue {
    capacity: Arc<Semaphore>,
    active: Arc<Semaphore>,
}

pub(super) struct Reservation {
    queues: Queues,
    identity: [u8; 32],
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
        let queue = queues.entry(identity).or_insert_with(|| {
            Arc::new(Queue {
                capacity: Arc::new(Semaphore::new(1 + BACKLOG)),
                active: Arc::new(Semaphore::new(1)),
            })
        });
        let capacity = queue
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| ProbeRefusal::TRANSIENT)?;
        Ok(Reservation {
            queues: self.0.clone(),
            identity,
            queue: queue.clone(),
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
        // Only the map and this reservation remain; no waiter or worker owns it.
        if Arc::strong_count(&self.queue) == 2 {
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
