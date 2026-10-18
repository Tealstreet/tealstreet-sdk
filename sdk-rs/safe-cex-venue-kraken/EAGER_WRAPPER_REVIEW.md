# Public async wrapper timing

The Kraken entry in `parity/PUBLIC_ASYNC_WRAPPER_CENSUS.md` is the deferred-JS
family. Both `invoke` and `invoke_unit` used to call the assembled member inside
the returned Rust future. A caller that never polled it therefore started no
operation, unlike the maintained TypeScript async arrow.

Both helpers now call the actual member immediately and capture its native
`Result<Js, Thrown>`. They return a waiter on that same canonical Promise. Error
projection stays inside the waiter. The member's existing Promise driver owns
its continuation; an extra `start_eager` or venue-specific spawn would introduce
a second Promise unnecessarily. This is the existing-Promise family described
by the shared eager-start census, rather than the typed-native-body family.

`tests/exchange_binding.rs` has four real production-constructor regressions:
Futures typed HTTP, Spot typed OHLCV HTTP, held signed leverage response followed
by an unpolled-waiter drop, and synchronous rejection reporting followed by a
rejected waiter. The held response registers and wakes the real runtime waker.
HTTP checks pump interceptor jobs without polling the SDK waiter: Axios's
physical send is not claimed to run before its own async interceptors. Dropping
the waiter still permits the leverage update and its original success log.

`eager_wrapper_live_source_prefix_and_detached_continuation` executes
`scripts/eager-wrapper-source.cjs`. It selects the unchanged live TypeScript
`fetchTickers`, Spot `fetchOHLCV` and `setLeverage` arrows using the TypeScript AST,
then replaces only their terminal HTTP, Store and diagnostic dependencies. It
checks immediate HTTP calls, a discarded Promise's continuation, and rejection
reporting before returning the Promise. Source hashes are emitted in its receipt.

Run from `sdk-rs` with `SAFE_CEX_TYPESCRIPT_PATH` pointing at installed TypeScript:

```
cargo test --locked --jobs 1 -p safe-cex-venue-kraken --test exchange_binding
```

Replacing only `binding/exchange.rs` with the exact pre-fix master bytes produces
four named compiled failures while the maintained-source witness passes. Restore
the reviewed helper bodies and all four pass. This closes the common SDK wrapper
startup defect, not all deeper method-specific wire, borrowed-host, Store or
shared-output-domain claims in the six assigned Kraken timing rows. The
underlying generic free-function APIs retain their explicit borrowed futures.
