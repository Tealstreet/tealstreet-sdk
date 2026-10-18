# Native depth sequence input

Maintained `applyBinanceDepthUpdate` uses Number and Number.isFinite on `u`, `U`
and `pu`. JSON.parse of raw `1e400` and `-1e400` produces infinities. A nonfinite
previous ID falls back to the contiguous first-ID check. A nonfinite final ID
returns `gap`, requiring a new snapshot. The existing maintained 3,080-case
`apply-depth.mjs` oracle includes raw signed overflow literals for all three IDs.

The native producer graph previously projected depth data through JSON output
before calling that correct shared algorithm. JSON.stringify converted infinities
to null, so a contiguous previous-ID overflow caused a false gap, and a final-ID
overflow was mistaken for an old update. The affected seams are:

- `NativeListenerOperations::apply`, used by Futures, PM and Spot.
- Inverse `Depth::apply`, used for live and replayed updates.
- Inverse `Depth::fetch`'s buffered-update selection.

All three now use the existing shared `to_retained_input` / `into_value` data
bridge. Its Number representation preserves raw JSON overflow for the existing
map-based helper. The shared depth algorithms, their native numeric operations
and public APIs remain the same.

The two new `public_listener_aliases` tests construct actual native ExchangeOwner
instances with native_dependencies and core public sockets. Terminal HTTP is the
only substituted request operation. They deliver raw text frames, hold/release
real snapshot futures, and check callback amounts, callback counts and snapshot
request counts. Twelve previous-ID cases cover three families, buffered/live
updates and both overflow signs; three final-ID cases require a held resnapshot.
The inverse integration test uses the actual public constructor/socket and covers
four buffered/live, signed previous-ID cases, then a held final-ID resnapshot in
each. Every loop has an executed-case assertion.

These three tests compile and fail by named assertion on master `1669d0a622`.
`scripts/depth-overflow-controls.py` independently restores each of the three old
production projections, requires the corresponding named compiled failure, then
restores exact source bytes and requires GREEN. Its buffered-only control must
reach the buffered=true assertion after the live cases pass. Compiler errors do
not count as controls. The complete existing TS helper oracle and socket target
remain separate verification; package/workflow target selectors are unchanged.
