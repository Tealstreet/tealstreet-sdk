//! Fully populated runtime shape-validation fixture, matching store.stub.ts.
use safe_cex_types::StoreData;
use std::sync::LazyLock;

pub static STUB_STORE: LazyLock<StoreData> = LazyLock::new(|| {
    serde_json::from_str(include_str!("store_stub.json")).expect("typed stubStore shape")
});
