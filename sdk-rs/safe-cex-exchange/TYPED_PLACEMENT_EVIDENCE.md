# Typed placement evidence

`ValidatedOrder<V>` and `ValidatedBatch<V>` carry the concrete venue type. Their
payloads and constructors are private to the crate. Typed `Exchange` placement
hooks require `Self: Sized`; all remaining dynamic venue operations retain the
object-safe `Exchange` surface.

The raw SDK entrypoints validate the **target** base, then invoke a private
binding dispatch. A registered binding downcasts the target before constructing
its venue-specific token. A mismatched registration returns an error and never
invokes the handler. The whole batch is validated before either native bulk or
sequential placement starts. `assert_caller_client_order_ids` returns unit and
cannot mint a token for any venue.

## Positive checks

`cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange --all-features`
completed successfully: 78 tests passed, 3 explicitly ignored source-oracle tests,
including the compiled toy venue doctest and the trybuild runner. The runner
checks three separate compiler diagnostics against committed `.stderr` files:

- `order_cross_venue.rs`: E0308, expected `ValidatedOrder<B>`, found `ValidatedOrder<A>`.
- `batch_cross_venue.rs`: E0308, expected `ValidatedBatch<B>`, found `ValidatedBatch<A>`.
- `order_private_payload.rs`: E0616, assignment to the private `order` field.

These checks use pinned trybuild 1.0.116; no inert diagnostic-code suffix is used.

`core_contract::dynamic_erasure_checks_identity_before_minting_typed_payloads`
keeps both A and B concrete types in scope. B deliberately carries A's single
and bulk binding: both reject before A's handler receives a token. Correct A
and B registrations succeed. Calling through A's base with target B still uses
B's caller-ID validation, and rejects an invalid ID before any write.

`native_bulk_registration_receives_target_typed_batch_after_whole_preflight`
proves an invalid second order prevents the native bulk handler from running;
a valid batch invokes it once. It also checks default and explicit empty
placement brands. The existing sequential test verifies the same preflight
policy before the first single-order write.

The migrated dropped-nuke regression now verifies a dropped pending cancellation
future does not permanently latch `is_nuking`: the next call reaches cancellation
again. Existing position/worker cancellation regressions were preserved.

## Measured negative controls

Each UI input was changed separately, the unchanged trybuild runner was run, and
the input was restored before the final positive run:

1. Order: changed only the source token type from `ValidatedOrder<A>` to
   `ValidatedOrder<B>`. Compilation succeeded and trybuild **failed** with
   `Expected test case to fail to compile, but it succeeded.`
2. Batch: changed only `ValidatedBatch<A>` to `ValidatedBatch<B>`. Compilation
   succeeded and trybuild **failed** with the same explicit message.
3. Privacy: removed the private-field assignment, retaining the valid token/raw
   argument types. Compilation succeeded and trybuild **failed** with that message.

Logs: `/tmp/p4a-negative-order_cross_venue.txt`,
`/tmp/p4a-negative-batch_cross_venue.txt`,
`/tmp/p4a-negative-order_private_payload.txt`. Restored positive UI and core
contract logs are `/tmp/p4a-typed-positive.txt`; the complete positive suite log
is `/tmp/p4a-typed-full.txt`. All three original UI inputs and their diagnostic
snapshots are restored.

## Mapping and lint checks

The restored parity gate passes: 6,901 source rows and 933 public Rust items.
Before the exact `capabilities.rs::sealed::Sealed` representation-glue exemption
was supplied, the same gate rejected that precise public item as unrepresented.
Only that private-module sealing supertrait is exempted; all placement and brand
behavior is mapped to the corresponding source rows. This also demonstrates the
invention gate can fail for this newly added glue, without relaxing the gate.

`cargo clippy --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange --all-features --all-targets -- -D warnings`
passes. Logs are `/tmp/p4a-typed-parity.txt` and `/tmp/p4a-typed-clippy.txt`.
