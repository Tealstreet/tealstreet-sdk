# Public subscription function review

The five TS `subscribe` bodies in `binance.ws-public.ts`,
`binancepm.ws-public.ts` and `binancespot.ws-public.ts` snapshot own topic keys,
read each current count, require a finite positive number and send one JSON
`SUBSCRIBE` frame per enabled topic, with IDs starting at 10. A send throw aborts
the loop. The maintained source oracle still executes all 5,505 cases across all
five classes, including nonnumeric counters, NaN, infinities, key order, removed
and activated entries, appended keys and send errors.

## Production callers and retired duplicates

`PublicState::subscribe` is the production caller of
`public_subscriptions::subscribe_binance_public`. All five `PublicKind` variants
use that same concrete state method. The private `subscribe` body implements the
shared encoder. The four family-named wrappers (`subscribe_binance_market`,
`subscribe_binance_pm_market`, `subscribe_binance_pm_public` and
`subscribe_binance_spot_public`) merely forwarded the same arguments; the only
callers were branches of `tests/public_subscriptions.rs`. A tracked Rust search
across the SDK, including registry, FFI/NAPI and wasm exports, found no production
caller or host export for those four generic wrappers. Retire them rather than
classifying a second test-only route. Keep all five TS manifest rows mapped to
the shared production wrapper and `PublicState::subscribe`, with explicit rename
provenance. No Cargo target, package command or workflow step is removed.

## Individual accepted roles and native execution

`subscribe_binance_public` forwards a typed key iterator and two capabilities;
`subscribe` encodes topic names and counts into outgoing frames. Neither decodes
serialized wire input or constructs a decoded record. Their distinct
`non-decoder` records retain exact compiler function APIs and specific reasons.
The classification receipts do not claim decoder execution or arbitrary generic
host coverage.

The native test
`public_subscription_receipts::production_native_subscription_frames_order_counts_reentry_and_send_failure`
constructs the actual `ExchangeOwner` with `native_dependencies` and opens each
of its five concrete public/market sockets through core `BaseWebSocket` and a
recording host. It observes exact frame bytes before any wire parser, integer-key
order, lazy count reads, key snapshot behavior during reentrant send, finite
count filtering, error name/message and early abort, and no sends for an empty
snapshot. The host adapter substitutes only physical socket/HTTP I/O. No venue
subscription operation is replaced.

`scripts/public-subscription-controls.py` corrupts each surviving production
function independently: a discarded key iterator before forwarding and a changed initial ID.
Each must compile and fail that named native wire assertion; exact source
restoration must pass. Compiler failures do not count as regression evidence.

## Scoped commands

Run Cargo from `sdk-rs`, serially, with the pinned toolchain:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-binance --features native --test private_classes public_subscription_receipts::production_native_subscription_frames_order_counts_reentry_and_send_failure -- --exact
cargo test --locked --jobs 1 -p safe-cex-venue-binance --test public_subscriptions -- --include-ignored
python3 safe-cex-venue-binance/scripts/public-subscription-controls.py
CARGO_TARGET_DIR="$PWD/target/accepted-inventory" node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-binance --jobs=1 --measure --module=public_subscriptions
```

The canonical measurement uses the maintained #951 module filter. It preserves
full compiler discovery and source authentication. Inspect actual receipt
failures: `--measure` returning zero alone is not success. This cohort does not
claim whole-Binance, alternate feature, wasm, or other module coverage.
