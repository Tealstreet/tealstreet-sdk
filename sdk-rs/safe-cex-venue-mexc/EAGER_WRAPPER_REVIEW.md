# MEXC mixed async boundary

Baseline: `c337bab6e6016409cdd7bb9b862074dd47f29ae7`.
Maintained source: `MexcExchange` in `packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts`.
The wrapper census classifies this implementation as mixed native/assembly.

The actual constructed `Exchange` trait previously deferred the metadata Store
notification, private leverage request, typed update's cancellation and unit
cancel-all request until polling the SDK waiter. Four independent compiled tests
failed by named behavioral assertions on the baseline. The maintained-source
oracle executes the four original field initializers, with actual DefaultStore
for metadata/cancel-all and explicit terminal dependencies for updateOrder.
It confirms the immediate prefixes and ignored-Promise continuations.

Native metadata/leverage now uses `eager::start_eager` and retains the existing
constructed receiver, BaseExchange, REST adapter and shared symbol-settings map directly, without JSON
or a detached state snapshot. `updateFetchedLeverage` remains the same Store
mutation, accepting the retained base handle. The private response uses the
existing source-await reaction boundary: ready responses cannot commit or notify
before the call returns. Public-only fallback still commits synchronously and
returns rejection for the original throwing subscriber. A separate compiled regression drops both the SDK owner and waiter while HTTP
is held: the source receiver stays alive through the continuation and releases
after completion. The native unit oracle
retains its distinct 21/7 fallback values and original final subscriber cause;
only its explicit lazy-prefix expectations change.

The assembly `typed` and `unit` helpers synchronously create their existing
MethodObserver. Its host task already starts eagerly and owns completion, so
there is no second spawn. Tests hold cancellation responses, drop the SDK waiter,
release HTTP and require the real Store cancellation/replacement wire effects.
The login signer's previous manual first poll is replaced with the shared typed
boundary, retaining its existing signer call. The socket awaits that already-fenced
waiter directly, replacing the former outer reaction instead of doubling it.
The unchanged socket test requires the ready login after reconnected and exactly
one continuation turn.

Scope: these changes cover the assigned updateSymbolMetadata timing finding and
the shared assembly adapters. They do not claim all other direct native readers,
per-method source awaits, socket timing, or Hyperliquid Realm timing are closed.

Verification commands (from sdk-rs, one build at a time):

- `cargo test --locked -p safe-cex-venue-mexc --jobs 1 --lib --test exchange_trait`
- `cargo test --locked -p safe-cex-venue-mexc --jobs 1 --test socket_assembly`
- `cargo clippy --locked -p safe-cex-venue-mexc --jobs 1 --lib --tests -- -D warnings`
- `cargo test --locked -p safe-cex-exchange --all-features --jobs 1 --test cancellable_future_inventory --test override_contract`
- `cargo run --locked -p safe-cex-parity --jobs 1 --bin parity-check --`

Node-based tests require SAFE_CEX_TYPESCRIPT_PATH naming the installed compiler
and its installed oracle dependencies. No wall-clock timeout is used to establish
prefix order. The terminal held-response fixture retains and wakes its real task
waker on release. Unit leaf-oracle fixtures use a queued executor instead of
panicking on the now-required shared Promise driver; they are not binding proof.
