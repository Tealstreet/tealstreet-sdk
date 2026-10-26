//! Complete source-owned exchange metadata, separate from credential/routing projections.
#[path = "exchange_metadata_generated.rs"]
mod generated;
use crate::object_map::JsObjectMap;
pub use generated::{ExchangeFeatures, ExchangeMetadata};
use indexmap::IndexMap;
use serde::Deserialize;
use std::sync::OnceLock;

struct Snapshot {
    metadata: JsObjectMap<ExchangeMetadata>,
    unified: ExchangeMetadata,
}
#[derive(Deserialize)]
#[serde(remote = "Snapshot")]
struct SnapshotWire {
    metadata: JsObjectMap<ExchangeMetadata>,
    unified: ExchangeMetadata,
}
impl<'de> Deserialize<'de> for Snapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        SnapshotWire::deserialize(crate::strict_decode::object(deserializer))
    }
}
fn snapshot() -> &'static Snapshot {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        serde_json::from_str(include_str!("exchange_metadata.json"))
            .expect("generated metadata conforms to its source-owned schema")
    })
}
pub fn exchange_metadata() -> JsObjectMap<ExchangeMetadata> {
    snapshot().metadata.clone()
}
pub fn get_exchange_metadata(exchange: &str, is_unified: bool) -> Result<ExchangeMetadata, String> {
    if exchange == "bybit" && is_unified {
        return Ok(snapshot().unified.clone());
    }
    snapshot()
        .metadata
        .get(exchange)
        .cloned()
        .ok_or_else(|| format!("Unknown exchange metadata: {exchange}"))
}
pub fn is_exchange_supported_on_mobile(exchange: &str) -> bool {
    snapshot()
        .metadata
        .get(exchange)
        .is_some_and(|metadata| metadata.supported_on_mobile)
}
pub fn exchange_pretty_names() -> JsObjectMap<String> {
    snapshot()
        .metadata
        .iter()
        .map(|(name, metadata)| {
            (
                name.clone(),
                if metadata.pretty_name.is_empty() {
                    name.clone()
                } else {
                    metadata.pretty_name.clone()
                },
            )
        })
        .collect()
}
pub fn latency_domain_to_exchanges() -> IndexMap<String, Vec<String>> {
    let mut map: IndexMap<String, Vec<String>> = IndexMap::new();
    for (name, metadata) in &snapshot().metadata {
        if let Some(domain) = metadata
            .latency_domain
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            map.entry(domain.clone()).or_default().push(name.clone());
        }
    }
    map
}
pub fn get_unique_latency_domains() -> Vec<String> {
    latency_domain_to_exchanges().into_keys().collect()
}
