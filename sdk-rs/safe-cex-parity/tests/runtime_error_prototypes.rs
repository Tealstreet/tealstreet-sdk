//! The production module graph remains indexable with the real Error factory.
use std::path::Path;

#[test]
fn production_error_factory_stays_in_the_full_rust_index() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let index = safe_cex_parity::rust_index(workspace)
        .expect("the real production module graph must remain indexable");
    assert!(
        index
            .public
            .contains("safe-cex-exchange/src/js_runtime/value.rs::error_object")
    );
}
