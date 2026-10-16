# Hyperliquid cross-cache runtime gates

Original integration parent: `645f6fda62f3fab6787db45ab45688376008b739`.
Implementation/proof base: `31ff644a5e5d800a959663bf0755211f1e1bb8b4`.
Integration merge: `00a724fa271c5e6ee02fec38b385880dfe26c951`, incorporating
`775a019fe06014b8b858b3f8f6cbdabac552abaf`. No unreviewed feature ancestry.

Eight rows move pending → ported; zero move to divergent. Python 3.14 tomllib
census over all Hyperliquid manifests at the first tested merge: `(308,106,117)` → `(316,106,109)` in
ported/divergent/pending order. Assigned nine source files: `(111,0,50)` →
`(119,0,42)`. Cross-cache alone: `(17,0,8)` → `(25,0,0)`.

The native class module supplies a shared prototype/constructor and singleton
slot. All eighteen instance method bodies execute mapped Rust algorithms.
Browser globals, native JS collection/property/coercion operations, constructor
handles and function allocation are mandatory host primitives. Native class
method writable/configurable descriptors are binding contracts; the witness
asserts non-enumerability, own names, constructor and shared-method identities.
This is an adapted composition, with no concrete browser implementation claim.

The oracle transpiles the complete unchanged TypeScript module. Its independent
floor is 27 cases, stopping at index26 (`class`): four constructors, nine receives,
three additions, nine matches, one module-wrapper record, and one class record.
Eight Rust tests inspect decoded native variants, own fields, UTF-16 units,
ordered Map/Set contents, return values, original argument/array aliases, retry
initialization and callback effects. No Rust result is encoded to establish its
native equality. Numeric fixtures are finite; no NaN-bit or exhaustive input
cardinality claim. Exotic accessors/proxies, sparse arrays, arbitrary coercions,
reentrant singleton construction, browser scheduling and live exchanges are
outside this finite witness.

## Discrimination at the proof base

Each isolated control runs the same target with one selected production mutation:
`SAFE_CEX_CACHE_MUTATION=N cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test cache_runtime TEST --jobs 2 -- --exact`.
The temporary selector is absent from restored production. The independently
selected patch SHA-256 was
`82ceaa630bc4bd798ed528199195348f3b9688cca8301a82dfba49778ccb2545`.
All controls used Node 24.20.0 and
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| N | Production change / call site | Test | Named assertion | Exit |
|---|---|---|---|---|
| 1 | new_module defines enumerable methods | class_descriptors | CACHE class nonenumerable | 101 |
| 2 | new_cache cleans storage before initialization | constructor_order | CACHE constructor initialization before cleanup | 101 |
| 3 | receive addPending writes pre-placement map | broadcast_receives | CACHE pending addPending-oid-false | 101 |
| 4 | AddValid replaces <=30000 with <30000 | add_boundaries | CACHE broadcast add-inclusive | 101 |
| 5 | match_pre_placement starts at FIFO index0 | match_priorities | CACHE result price-first | 101 |
| 6 | get_cross_cache ignores retained singleton | singleton_identity_and_retry | CACHE singleton reused | 101 |
| 7 | module has_seen_fill delegates markFillSeen | module_has_native_value | CACHE module has unseen: bool | 101 |
| 8 | module mark_fill_seen delegates hasSeenFill | module_mark_native_effect | CACHE module mark inserts | 101 |

The initial batch of all eight mutations also exited101 with all eight named
tests failed. Isolated controls remove masking ambiguity. No mutation survived.
Production bytes were restored exactly to the proof base before merge-down.

## Merge preservation

`git diff --exit-code 31ff644a5e HEAD --` all crate `src` and `tests`, the source
TypeScript file, cross-cache manifest, crate/workspace Cargo manifests,
Cargo.lock, package.json and yarn.lock exited0 at the integration merge.
A conservative Cargo.lock dependency walk included481 packages and eight local
packages; it stopped when the worklist emptied after539 pops. No feature pruning
claim. Package structures match the proof base, and the local `src`, `tests`,
Cargo.toml and build.rs diff exited0. Local packages were safe-cex-candle-cache,
safe-cex-crypto, safe-cex-exchange, safe-cex-num, safe-cex-store,
safe-cex-transport, safe-cex-types, safe-cex-venue-hyperliquid.

## Positive gates

Restored gates at `00a724fa271c5e6ee02fec38b385880dfe26c951`:

| Gate | Measured exit / output |
|---|---|
| Node24.20.0 scoped cache_runtime test | 0; eight passed, zero failed |
| Node26.7.0 same scoped test | 0; eight passed, zero failed |
| Pinned Yarn parity:extract:check | 0; Would update 0 manifests; 0 added, 0 orphaned symbols |

Scoped test command: `cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test cache_runtime --jobs 2`. Node26 uses
`PATH=/opt/homebrew/opt/node@26/bin:$PATH`; both use the explicit TypeScript
path above. Cargo commands run sequentially. This restores all eight isolated
mutation witnesses to passing tests, including the latest own-key assertion.

`cargo check --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --all-targets --jobs 2` exited0 at the same SHA;
`Finished dev profile` was inspected. This is scoped host compilation, not
workspace compilation or wasm/clippy evidence.
The earlier production source test at52b231d013 exited0 (eight tests), before
the additional own-key assertion commit. It does not cover that newer assertion.

The manifest writer used the pinned Yarn JavaScript executable and exited0:
`Updated 1 manifests; 0 added, 0 orphaned symbols.` The pre-merge extractor check
exited0 with `Would update 0 manifests; 0 added, 0 orphaned symbols.` Output text
was inspected independently of Yarn's exit status.

Not run: workspace/full-crate/monorepo test suites, host clippy, wasm check/clippy,
Rust inventory/provenance gates, CI reruns, live exchange/browser integration.
No shared SDK trait/type changed. Both reviews were read-only sub-agent reviews;
the reviewers ran no gates. Fixture findings were corrected before isolated
controls: distinct class tokens, aligned absent-global instrumentation, populated
retry fields/independent allocation count, and own broadcast payload keys/values.

## Later integration merge

The last fetch incorporated `d8bc6fc533787b072aa5aaad1ee3b0ee09f8681e` at
`53d78f6e3000e3d9eb29afd138595a550003812c`. The sole conflict was in lib.rs;
both cache_runtime and private_message_dispatch exports were retained.
Whole-crate src/tests preservation from00a724fa271 exited1: lib.rs gained the
other lane's export, plus its dispatch module/test. The precise cache runtime,
three test/oracle/support files, cross-cache manifest, source TypeScript,
Cargo.lock, Cargo manifests, package.json and yarn.lock diff exited0.
At this exact SHA, the scoped all-targets cargo check exited0 (`Finished dev
profile`), Node24 and Node26 scoped cache_runtime tests each exited0 (eight
passed, zero failed), and pinned Yarn parity:extract:check exited0 with
`Would update 0 manifests; 0 added, 0 orphaned symbols.` These replace the
previous broader crate-input claims. The seven local dependency crates outside
this venue also diffed byte-unchanged, exit0, from the first tested merge.

Tomllib after this merge: full venue `(326,106,99)`; assigned nine files
`(119,0,42)`; cross-cache `(25,0,0)`. The other lane contributed ten additional
ported rows. This branch's own manifest change remains eight pending→ported,
zero→divergent. No perp TypeScript or manifest was edited by this slice.

## Push preservation

The final fetch merged `8cdc8f6eb4a04700bd2c7f3c706ecd9fdf1d3ebf` at
`872e296a436d69ac74ff50e0bb0b46550be19b26`. The whole venue crate's src/tests,
all seven local dependency crates, lock/workspace/crate Cargo files, own source
and manifest, package.json and yarn.lock diff from the rerun gate SHA53d78f6e300
exited0. The scoped cargo and Node24/26 exits therefore retain those measured
input bytes. Other venues' manifests changed; parity:extract:check was rerun at
this final merge and exited0, with `Would update 0 manifests; 0 added, 0 orphaned
symbols.` These gates are tied to their named SHAs, not to unmeasured ancestors.

A final tomllib per-symbol transition comparison of all nine assigned manifests
against645f6fda62 confirmed exactly eight pending→ported transitions and no
other status transitions. Assigned/full-venue triples above remain unchanged.
The evidence-only successor commit changes none of the measured gate inputs.
