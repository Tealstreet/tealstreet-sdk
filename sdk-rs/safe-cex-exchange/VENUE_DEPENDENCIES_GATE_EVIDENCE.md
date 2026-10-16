# Exchange-core and venue dependency scaffold

Historical scaffold measurements at base `6fd427e73c`; branch: `feat/rust-p4a-venue-deps`.

The initial scaffold described below is superseded by the repair record at the end.

The initial layer order was types, numeric, services, exchange-core, venues, sdk,
bindings. Every one of the 26 venue Cargo manifests declares its downward
exchange-core dependency and its direct types, numeric, transport and crypto
uses. The 104 venue skip exceptions and two exchange-core model/numeric skips
name actual Cargo edges and explain their purpose. No exception permits a
lateral or upward dependency.

`safe-cex-exchange` is empty and claims zero rows. Its dependency declaration
includes P4a-core's requested serde/JSON, signing, service, regex and numeric
support, plus indexmap for Store-trait tests. Cargo.lock adds this local crate
and four regex packages; no pre-existing external package entry changed.
The 26 venue lock entries now carry their declared dependencies.

`safe-cex-registry` remains data-only. All 134 pending rows in the four registry
and public-data manifests retain their status and empty mapping; their blockers
now identify `safe-cex-exchange` and place registry constructors/loaders in the
SDK facade `safe-cex`, above venues. No implementation row was added or promoted.

## Positive gates

All returned exit 0, including after restoring the mutations below:

- `cargo fmt` scoped to safe-cex-exchange and the 26 venue packages, `-- --check`.
- Plain `cargo clippy` on those packages, `--all-targets --locked`; inherited
  workspace deny lints provide strictness without an extra warning flag.
- `cargo test` on those packages, `--all-targets --locked -- --include-ignored`.
  These are empty scaffolds: this checks compilation, not runtime behavior.
- `cargo build` on those packages, `--target wasm32-unknown-unknown --locked`.
- Node layer/wasm-policy suites: 32 tests pass. The new graph tests remove each
  of the 104 venue exceptions independently, reject venue-to-facade and
  core-to-venue upward edges and registry-to-venue lateral edges, and allow a
  facade-to-venue constructor edge.
- `yarn parity:extract --check`: 6,901 rows, zero changed/additions/orphans.
- `yarn parity:check`: 6,901 rows, 393 public Rust items.
- `yarn parity:report` and `yarn parity:progress`: both 277 / 6901. Their printed
  basis is validated manifest claims, not proof of behavioral equivalence.

Cargo runs were scoped; no workspace-wide Cargo build or repository test suite
ran. Yarn gates used the installed Yarn JS entrypoint so exit codes propagated.

## Negative controls

Each mutation ran alone and was restored before the next. All positive gates
were rerun after restoration.

| Gate | Mutation | Observed failure |
| --- | --- | --- |
| fmt | Unformatted const in core lib | exit 1, formatting diff |
| clippy | Unused private `unused_probe` | exit 101, named dead code |
| test | Injected `injected_failure` asserting 1 == 2 | exit 101, named failed test |
| wasm | wasm-only `compile_error!` | exit 101, `venue-core-wasm-negative` |
| layers | Remove Bybit-to-types exception | exit 1, named skip edge |
| extraction | Rename pending registry Bybit row to an absent symbol | exit 1, addition/orphan |
| invention/status | Add unmapped public `invented_probe` | exit 1, named invented item |
| report | Map GatewayMessage to `DOES_NOT_EXIST` | exit 1, named unresolved target; no progress table |
| progress | Same unresolved target | exit 1, named unresolved target; no progress fraction |

## Independent review

A reviewer used its own detached checkout at `bd2d016839`, ran 62 scoped Node
checks including locked metadata and CI wiring, and returned **MERGE**, with
no required fix. Removing skip enforcement failed the venue regression test;
removing the real Bybit model exception named the forbidden edge. The reviewer
restored both mutations and left its checkout clean.


## Follow-up ruling, 2026-10-12

Registry now has its own layer above venues and below SDK. Added its workspace
dependency declaration; no constructor stubs or new runtime edges were added.
The policy's live graph tests inject registry -> bybit and facade -> registry and
require both to pass without exceptions, while moving registry back alongside
venues fails the constructor edge and a venue -> registry edge fails upward.
Registry pending blockers now identify the registry crate as constructor owner.

The shared `safe_cex_num::js_trim` shim compares all 1,112,064 Unicode scalars
against live Node. Its two tests pass. Three deliberate mutations fail with exit
101: Rust trim, Rust whitespace plus BOM, and the correct Rust subset without
BOM. Each was restored. Plain numeric Clippy passes. Exact set and the correction
that U+001C–001F belong to neither predicate are recorded in num/CLAUDE.md.

## Review repairs, 2026-10-12

The current order is types, numeric, services, exchange-core, venues, registry,
SDK, bindings. Constructors belong in safe-cex-registry; its actual dependencies
and skip exceptions reach exchange-core, store and transport. FFI also declares
its registry edge. The facade-to-core edge remains owned by the core branch.
The four manifest blockers and both crate guides agree on this placement.

`node sdk-rs/scripts/venue-dependencies.mjs` records direct TypeScript imports
resolved through manifest rust_files ownership. Current evidence: types, crypto,
store and transport each reach 26 venues; numeric reaches 24; candle-cache one.
49 distinct imported source files have no declared ownership yet. This is an
incomplete ownership census, not an exhaustive dependency prediction.

The subsequent ruling explicitly grants all six named targets to all 26 venues:
156 real Cargo edges with 156 exceptions, one reason per target stating that
this is a whole-layer grant rather than an individual measurement. The test
removes each exception independently and requires failure. It pins the six
lower crates: a seventh requires a new architecture decision and receives no
automatic grant. Lateral, upward and private-derivation restrictions remain.
A layer-level exception syntax to compress 156 entries into six is a follow-up,
not a checker change in this review.

Regex is workspace-pinned to =1.13.1, optional behind health with default features
empty in exchange-core. Exact pins require deliberate security-patch updates;
cargo update alone cannot advance this requirement. Client-ID predicates are
not relocated and the facade's feature activation remains on the core branch.

The shared whitespace helpers now have production callers in store instrument
classification/risk parsing and candle-cache chunk-key parsing. The live Node
oracle checks trim, trimStart, trimEnd and whitespace-run splitting over all
1,112,064 Unicode scalars. The candle-cache regression compares its parseInt
boundary with live Node; the manifest note also records the remaining signed
64-bit key restriction rather than claiming unrestricted JS Number equivalence.

Repair validation (2026-10-12): all 28 layer tests pass, including all 156
independent exception removals. Scoped numeric/store/candle-cache tests with
ignored tests included pass, as does their plain Clippy. All 26 venue crates,
exchange-core, registry and FFI pass scoped Clippy and wasm checks. Cargo tree
confirms regex absent from all 26 normal default dependency closures. Extraction
reports zero changes/additions/orphans; parity check reports 6,901 rows and 397
public Rust items. These are this branch's dated measurements before merge-down.
Replacing candle-key parsing with bare Rust trim_start fails the named live-Node
boundary test at exit 101; restoring it passes. The public adapter regression
uses chunkSize=1 and arbitrary listed keys: i64 overflow is silently skipped,
not returned as an error or wrapped, while representable integers above 2^53
retain exact keys unlike JS rounding. This reachable divergence is in the note.

## Integration merge and visible refusal, 2026-10-12

Merged integration ed2afa07f8 after PR7213 landed. The previous repair record's
silent overflow skip is superseded: load_all_chunks now returns an error naming
any listed key whose namespace or signed-64-bit decimal prefix is invalid,
before reading any chunk. The public adapter regression at chunkSize=1 tests
positive/negative overflow, nonnumeric keys and NEL, alongside exact successful
keys above 2^53. Reverting to filter_map silent skipping fails this test with
exit 101; restoration passes. JS Number rounding remains a declared divergence.

The registry example retains the derived serde wire inventory and adds a
separate exhaustive Rust match. Injecting #[serde(skip)] Internal into
GatewayChannel fails compilation with E0004 naming Internal (exit 101). Thus a
wire-invisible variant still forces review; the match does not claim to add it
to the wire inventory. The mutant was restored before positive checks.

Merged-head checks after both mutants were restored: candle-cache and registry
all-target tests and Clippy pass; registry live oracle and its controls pass;
47 distinct layer/CI-wiring tests pass, of which 28 are the imported layer suite (the previous 75 count double-counted those 28). Extraction has zero changes/additions/
orphans; parity check passes with 6,901 rows and 401 public Rust items. Report
passes at 275/6901 verified ported rows, 331 distinct claimed Rust items,
49 divergent and seven not-applicable rows. These counts include the merged
registry's two ported-to-divergent moves; this repair promotes no manifest row.

## Verification repair: layer grants and partial listings (2026-10-12)

This supersedes the earlier per-pair grant and whole-list error choices above.
Six layerGrants permit only venues to use the named downward runtime targets.
No actual edge is required for a layer grant; individual pair exceptions still
require an existing edge and reject stale entries. The decision test pins both
the venues source layer and the exact six targets; a seventh target fails that
pin. Tests inject all 156 current venue/target combinations, accept them, remove
their matching grant, and require each edge to fail. Adding a newly classified
venue inherits these same bounded permissions. Removing all venue dependencies
is legal. All 26 crates here are empty scaffolds, so their unused Cargo deps and
lock entries were removed; implementation branches declare their actual uses.

load_all_chunks now returns LoadedChunks { chunks, refused_keys } on success.
Only finite decimal prefixes outside i64 are refused. Nonnumeric/NaN and
nonfinite decimal prefixes skip, matching the source finite filter. The retained
wrong-prefix guard is CHOSEN: TS trusts listKeys(prefix) and slices blindly.
The live source expression proves the out-of-prefix fixture can become 808 in
TS, while Rust skips it. Exact i64 keys above 2^53 retain the declared difference
from JS binary64 rounding. Refusals retain original listed keys, not rounded IDs.

The trait and memory backend share the explicit success-result shape. Cache
hydration/read-through mechanically consume .chunks; backend callers can inspect
refused_keys. A regression hydrates a valid sibling despite overflow and invalid
keys, with no network fallback. Restoring filter_map silent refusal fails the
refusal assertion; restoring Err on overflow fails sibling hydration. Both
mutants compile and fail at exit 101, then the restored tests pass. This does
not change the separate pre-existing hydration error-reporting path.

Repaired-head verification: 35 candle-cache unit tests plus two public API tests
pass, strict Clippy and wasm check pass. The combined CI audit passes 58 DISTINCT
tests, including 39 layer tests; do not add the layer count again. All 26 empty
venue crates pass locked all-target checking. Layer command validates 37 crates.
Extractor reports 6,901 rows and zero changes/additions/orphans; parity check
passes with 402 public items (the new LoadedChunks result has one exact reasoned
exemption). Validated report remains 275/6901, 331 distinct claimed items, 49
divergent and seven not-applicable. No row status or mapping changed. This branch
predates the three exported mutable rows on integration; its denominator is not
Bybit's 6,904. Independent reviews of both repairs returned no remaining findings
once the hydration fixture and wrong-prefix note were corrected.

## Integration merge and populated-venue sweep (2026-10-12)

Merged integration c77ebb5357 at 1ec8e3bcf6. Both parents had 52 named rust.yml
steps; the merged file retains all 52, with no missing names or workflow conflict.
Numeric conflict resolution retains the public js_number_string and all four
trim helpers, the service caller consolidation, and both exact exemptions for
js_number_string and LoadedChunks. The extractor census is now 6,904 rows.

The first Bybit merge exposed a test-fixture defect: removing a shared grant
could reject an existing Bybit edge before the injected venue edge. The named
assertion failed, rather than going green for the wrong edge. Isolation now
removes competing venue edges to that target before injecting the tested edge.
The grant test first populates every venue with all six permitted targets, so
empty scaffolds cannot hide the defect. Removing that isolation fails both
fixture modes with the wrong edge named (exit 1); restoring it passes.

The entire layer suite now runs against the live graph and against all venues
populated with six targets plus exchange-core. This includes stale exceptions,
seventh-target pin, removal injections, architecture violations and the new
venue fixture. All 39 checks pass in each mode: 78 layer executions, not 78
different invariants. The combined CI audit reports 99 executions (78 layer,
21 other). No further empty-scaffold assumption surfaced in this sweep.

Merged-tree candle-cache tests pass (35 unit, two API, three compile-fail docs);
numeric tests including the live formatter and scalar whitespace oracles pass.
The pre-existing ignored numeric differential test and doc example were not run
by this cargo invocation. Extractor is clean; parity check passes at 6,904 rows
and 403 public items. Validated report is 275/6904 with 331 distinct claimed items,
49 divergent and seven not-applicable rows. These are dated measurements, not
new porting claims. Checker and policy are unchanged by this fixture follow-up.
