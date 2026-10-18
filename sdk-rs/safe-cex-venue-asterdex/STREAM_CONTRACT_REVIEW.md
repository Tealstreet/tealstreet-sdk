# Stream function contracts

The selected cohort covers thirteen `ws_helpers` functions, five private-stream
coercion helpers and the public-stream `string` helper. All nineteen are reviewed
decoders with compiled production-function observations; no non-decoder labels or
new input APIs are added. `public_depth_snapshot` and the existing connection URL
contract belong to separate cohorts.

The compiler-derived baseline reports all nineteen function rows as unclassified.
The reviewed corpus has 117 independently authored cases, checked against the
maintained public/private WebSocket methods, the actual exchange status mapper,
the maintained partial-depth regexp and JavaScript's string/number primitives.
The pure Rust coercion helpers implement those inlined source operations.

Native observations precede JSON projection: binary64 bits retain signed zero,
NaN and infinities; lookup variants distinguish absent and inherited rows; pointer
equality proves that the original ticker is retained; callback traces retain ping,
retry, snapshot and listen-key cleanup ordering. Subscription observations retain
message bytes, IDs and ordered topic mutations. Missing and null IDs, missing
kline timestamps, optional numeric patches and closed-status refusals are separate
observations. Inherited status values use the existing closed native OrderStatus
refusal boundary, rather than being reported as an accepted source order state.

Run the selected default-feature native cohort serially:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/path/to/typescript \
  node safe-cex-venue-asterdex/scripts/accepted-stream-contracts.mjs
```

`--preflight` checks only the compiled API classifications; it does not execute
native probes. The ordinary runner also replays source expectations and requires
a runtime receipt for every selected owner. Other owners, aliases, local-helper
policies, features and Wasm remain outside this scoped result.

The grouped control runner temporarily corrupts real production bodies, requires
a compiled named runtime failure for every one of the nineteen owners, restores
the exact original bytes in `finally`, then requires the original cohort to pass:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/absolute/path/to/typescript \
  python3 safe-cex-venue-asterdex/scripts/accepted-stream-controls.py
```

Control receipts and exact restoration hashes are written under the worktree's
`target/asterdex-stream-controls/`. A compiler error is not a successful control.
