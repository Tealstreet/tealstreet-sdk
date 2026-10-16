# Order helper verification

The four tests in `order_operations_tests.rs` pass in the actual crate with
`cargo test --locked -p safe-cex-exchange --all-features --lib -- --include-ignored`.
They check mapper-field lookup, sentinel refusal, JavaScript coercion, and
field-presence shape validation. Earlier isolated rustc runs compiled the actual
helper source against Cargo-built dependencies while the Store contract arrived.

Negative controls in the isolated source harness:

- Removed the sentinel-ID guard: `client_lookup_rejects_sentinel_ids_even_if_venue_matches`
  failed. Restored source passed.
- Removed the stop-loss presence guard: `regular_client_id_shape_checks_presence_not_numeric_truthiness`
  failed on an explicitly present zero stop loss. Restored source passed.

The two snapshot helper tests and five `core_snapshots` integration tests pass
in the actual crate. They cover map duplicate/order semantics, shared waiter
wakes, disposal after fetch, bounded retries, optional empty symbol scopes,
loaded-position gating, retained rejected fetches, and the disposed-origin versus
existing-waiter result distinction. Integration tests use the shared test fixture
implementing the actual `safe_cex_store::Store` contract.

Snapshot helper negative controls in an isolated actual-source harness:

- Removed replacement of a duplicate market ID: the map order/last-value test failed.
- Woke only the first shared fetch waiter: the multi-waiter test failed.

Both restored versions passed. Stateful placement/cancellation/nuke verification
is owned by the complete exchange test suite; this file makes no independent
claim that those paths were exercised by the snapshot tests.
