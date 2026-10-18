# Accepted contract review

The compiler census contains 187 owners and seven public aliases. All 194 records
pass the ordinary native and target-specific Wasm package gates, each with 106
runtime results and 88 individually reviewed classifications. The gates retain
compiler, source, local, construction, alias and reexport checks. Kucoin's actual
reexport traversal selects no additional workspace package. These receipts do
not claim whole-workspace clearance.

The two final owner drafts retain all twenty compiler APIs: two typed
constructors and eighteen observed trait methods. All twelve ordinary method
adapters have executed on twenty-four genuine native inputs. The three inherited
refusals and three actual Spot constant/no-op overrides have separately executed
on twelve genuine inputs. Both complete owners execute in the formal package
receipts. Six exact reviewed finite policies retain equal outputs without
fixture echoes and pin both executable fixture hashes.

The contracts preserve the maintained
[UTF-8 boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[typed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[snapshot semantics](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and
[numeric ingress behavior](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).

## Native observations

Wire contracts inspect real enum variants, optional presence, correlated fields
and binary64 bits. Seven private Serde companions use module-level
`serde(remote)` definitions. Their public entries retain numeric capture and
map/string admission. The six maintained guard-removal controls compile and fail
at the named native array/tagged-object assertion, restore the exact source
bytes, then pass all seven field/shape tests. Complete sequence fixtures follow
the actual declaration order, including `code`, `data`, `msg` for token replies.

Function adapters execute real parsers, mappers and binding operations. Native
projections inspect returned fields, VM primitive variants, UTF-16 units,
`TableHit` identity, numeric bits, typed Store mutations and actual REST effects.
HMAC and broker-signature cases use independent standard HMAC-SHA256 answers over
explicitly synthetic material. Response parsing witnesses signed zero, overflow,
integer rounding, nested values and key order. Futures position leverage retains
the source `parseInt` truncation.

Each non-decoder function has its own complete-body review and reason in the
shard. These cover specific executable-capability construction, outgoing frame
production, event routing, lifetime management and typed metadata access. They do
not grant permission to neighboring parsers or to raw methods on an owner.

All seven alias policies name exact compiler raw/expanded ASTs and independently
specified concrete native types. Compiler artifact/root identities and trait
roles are authenticated by the actual gate before their receipts execute.
Unknown aliases receive no permission from this cohort.

## Concrete Exchange methods

The native I/O fixture reuses the existing E2E HTTP adapter, clock and task queue.
Its connected runtime only supplies scripted WebSocket dial/events. The two book
listener tests exercise actual shared `BaseWebSocket` connection, source message
routing, native callbacks and cleanup. No venue method or connection getter is
replaced. Futures first receives a valid sequential change that must emit a
second callback with independently expected native amount bits. After cleanup,
the next valid sequential change produces no further callback. Removing only the
production futures handler-removal statement compiles and fails the named
post-cleanup callback assertion; exact source restoration passes both book tests.
Spot witnesses the same real callback-before/after-cleanup boundary.

The six constant-method witnesses seed native orders and positions, install
Store/event observers, invoke literal concrete `Exchange` methods, and inspect
native return/error values plus unchanged state/options/clock and zero HTTP.
Both the I/O harness and observer implementation are executable dependencies of
the finite policy and must retain reviewed hashes.

## Reproduction

From `sdk-rs`, run the maintained native tests and guard controls serially:

```sh
RUSTC_BOOTSTRAP=1 cargo test --locked -p safe-cex-venue-kucoin --jobs 1 \
  --test accepted_native_guards --test accepted_constant_refusals \
  --test accepted_constant_overrides --test accepted_exchange_members
node safe-cex-venue-kucoin/tests/accepted-native-controls.mjs
node safe-cex-venue-kucoin/tests/accepted-book-cleanup-control.mjs
```

Run the ordinary package gate for native and Wasm separately. It follows
compiler-observed reexports; shared failures remain visible when selected rather
than being reclassified by this venue.

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-venue-kucoin --target=native --jobs=1
RUSTC_BOOTSTRAP=1 node safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-venue-kucoin --target=wasm32-unknown-unknown --jobs=1
```
