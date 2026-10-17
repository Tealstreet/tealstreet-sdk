The shared Map/Set iterator cutover exposed one missing production adapter:
`exchange_runtime::dispose` passes
`pendingStopSnapshotRefreshTimers.values()` to `Host::for_of`. The former host
accepted raw maps and arrays, so even an empty iterator threw during disposal.
Native `start` and account tests consequently failed at their cleanup step.

`Host::for_of` now sends JS values through one GetIterator/next path. Its opaque
`MexcSymbolSettings` view retains its existing typed projection. The native path
retains live array/map iteration and closes an iterator on a body break or throw.
Empty and non-empty map witnesses exercise values, keys, entries and direct map
iteration; the latter checks deletion/insertion, native -0/NaN and object identity.
`iterator-consumer-controls.py` must compile an iterator-object rejection and fail
both named witnesses, then restore exact bytes and pass both.

The consumer review searched all venue Rust sources for collection method names,
aliased `::Map`/`::Set` variants, GetIterator, `for_of`, spread and Array.from. It
also searched every maintained exchange TypeScript values/keys/entries call.
Object.keys/values/entries return arrays and were distinguished from collection
iterator returns; object spread was distinguished from iterable spread.
The raw search receipts are local development artifacts, not committed evidence.

| Shared JS iterator return | Concrete consumer |
| --- | --- |
| MEXC stop-refresh timer values | `exchange_binding.rs::Host::for_of` uses the common protocol front door. |
| Kraken timeout values, polling keys and book entries | `binding/host_typed.rs::MapValues` captures next and steps it; `binding/sockets.rs` materializes entries through shared `iterable_values`. |
| OKX deduplicated order/history values | `binding/intrinsics.rs` uses shared `iterable_values` for `Map.values-spread`. |
| Paradex position-map values and OHLCV callback values | `binding/primitives.rs::iterate` uses shared `iterable_values`; both Array.from.values and socket callbacks call it. |
| WEEX socket collection values | `binding/sockets.rs::units_array` consumes values with shared `iterable_values`. |
| Lighter hint keys, historical-fill values and public book values | `binding/ops.rs::JsIterator` and `spread` step shared iterators; the shared Array.from binding is used when available, and the compatibility fallback delegates iterable admission to spread. The maintained collection-return calls do not use a mutating mapFn. |
| Polymarket token keys and active-market sets | `binding/mod.rs::Iter` captures iterator/next; host iteration and set traversal use it. |
| Bybit Hyro pending ACKs, pending groups and market values | Host-generic source ports use explicit next, for-of and spread capabilities. The native `hyro_native_iterator` witness covers reset with real shared Map iterators. The Hyro production facade remains separate unmerged work. Primary/Inverse timer/history ports traverse typed native containers. |

The wider collection census also reviewed Bitget, Bitrue, Crypto.com, WOO,
AsterDEX, Orderly and GRVT. Their additional shared-JS collection occurrences
are direct retained state operations, typed collection projections or source
Object/array operations; they do not feed new shared iterator objects to the
remaining array-only helpers. In particular Bitget's raw collection traversal,
WOO's native fill-cache traversal and GRVT's first-entry/nonce adapters do not
call the shared collection values/keys/entries producers.

Coinbase's prepared frame bridge, Backpack's prepared-data refusal, ordinary
serde_json::Map records, and Hyperliquid's independent realm were checked
separately. They are not receivers of the changed shared JS iterator objects.
MEXC public subscription replay already traverses its native IndexMaps directly.
The other maintained collection-return operations in source-only/typed ports
likewise use their own native collection/realm operations; this repair changes
no representation or classification for those APIs.

Validation is recorded in the PR: both iterator witnesses and compiled control,
all six reported native/registry failures, shared collection Node-oracle tests,
and the scoped venue consumer suites. This census does not claim arbitrary
caller-created JS objects are supported by every typed venue boundary.
